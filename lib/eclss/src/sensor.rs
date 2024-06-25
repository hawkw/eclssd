use crate::{error::SensorError, Config, Eclss};
use core::num::Wrapping;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
pub use eclss_api::SensorName;
use embedded_hal_async::delay::DelayNs;
mod status;

#[cfg(feature = "bme680")]
pub mod bme680;
pub use bme680::Bme680;

#[cfg(feature = "pmsa003i")]
pub mod pmsa003i;
#[cfg(feature = "pmsa003i")]
pub use pmsa003i::Pmsa003i;

#[cfg(any(feature = "scd40", feature = "scd41", feature = "scd30"))]
pub mod scd;
#[cfg(feature = "scd30")]
pub use scd::Scd30;
#[cfg(feature = "scd40")]
pub use scd::Scd40;
#[cfg(feature = "scd41")]
pub use scd::Scd41;

#[cfg(feature = "sen55")]
pub mod sen55;
#[cfg(feature = "sen55")]
pub use sen55::Sen55;

#[cfg(feature = "sgp30")]
pub mod sgp30;
#[cfg(feature = "sgp30")]
pub use sgp30::Sgp30;

#[cfg(feature = "sht41")]
pub mod sht41;
#[cfg(feature = "sht41")]
pub use sht41::Sht41;

#[cfg(feature = "ens160")]
pub mod ens160;
#[cfg(feature = "ens160")]
pub use self::ens160::Ens160;

pub use self::status::{Status, StatusCell};

use tinymetrics::registry::RegistryMap;

#[allow(async_fn_in_trait)]
pub trait Sensor {
    type Error: SensorError;

    const NAME: eclss_api::SensorName;
    const POLL_INTERVAL: Duration;

    async fn init(&mut self) -> Result<(), Self::Error>;

    async fn poll(&mut self) -> Result<(), Self::Error>;
}

impl<I, const SENSORS: usize> Eclss<I, { SENSORS }> {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            name = "sensor",
            level = tracing::Level::INFO,
            skip(self, delay, sensor, config),
            fields(sensor = %S::NAME)
        )
    )]
    pub async fn run_sensor<S>(
        &'static self,
        mut sensor: S,
        config: Config,
        mut delay: impl DelayNs,
    ) -> Result<(), &'static str>
    where
        S: Sensor,
        S::Error: core::fmt::Display,
    {
        let State {
            status,
            backoff,
            poll_interval,
            ..
        } = self
            .sensors
            .get_or_register(
                S::NAME,
                State {
                    poll_interval: S::POLL_INTERVAL,
                    backoff: config.retries.backoff(),
                    ..Default::default()
                },
            )
            .ok_or("insufficient space in sensor registry")?;
        let errors = self
            .metrics
            .sensor_errors
            .register(S::NAME)
            .ok_or("insufficient space in sensor errors metric")?;
        let resets = self
            .metrics
            .sensor_reset_count
            .register(S::NAME)
            .ok_or("insufficient space in sensor reset count metric")?;
        let mut has_come_up = false;
        'initialize: loop {
            let mut attempts = 0;
            let what_are_we_doing = if has_come_up { "initialize" } else { "reset" };
            while let Err(error) = {
                status.set_status(Status::Initializing);
                sensor.init().await
            } {
                status.set_status(error.as_status());
                errors.fetch_add(1);
                attempts += 1;
                warn!(
                    %error,
                    "failed to {what_are_we_doing} {} (attempt {attempts}): {error}",
                    S::NAME
                );

                if Some(attempts) == config.max_init_attempts {
                    error!(
                        "Giving up on {} after {attempts} attempts to {what_are_we_doing}",
                        S::NAME
                    );
                    return Err("failed to initialize sensor after maximum attempts");
                }

                backoff.wait(&mut delay).await;
            }

            backoff.reset();
            if has_come_up {
                resets.fetch_add(1);
                info!("successfully reset {}", S::NAME);
            } else {
                info!("initialized {}", S::NAME);
                has_come_up = true;
            }

            loop {
                delay.delay_ms(poll_interval.as_millis() as u32).await;
                while let Err(error) = sensor.poll().await {
                    warn!(
                        %error,
                        retry_in = ?backoff.current(),
                        "failed to poll {}, retrying: {error}", S::NAME
                    );
                    status.set_status(error.as_status());
                    errors.fetch_add(1);

                    if error.should_reset() {
                        tracing::info!(
                            %error,
                            "attempting to clear {} error by resetting...",
                            S::NAME,
                        );
                        continue 'initialize;
                    } else {
                        backoff.wait(&mut delay).await;
                    }
                }
                status.set_status(Status::Up);
            }
        }
    }
}

pub type Registry<const N: usize> = RegistryMap<SensorName, State, { N }>;

pub(crate) struct PollCount {
    polls: Wrapping<u32>,
    abs_humidity_interval: u32,
    log_info_interval: u32,
}

impl Config {
    pub(in crate::sensor) fn poll_counter(&self, poll_interval: Duration) -> PollCount {
        let log_info_interval = {
            let mut interval = self.log_reading_interval;
            let mut i = 0;
            while !interval.is_zero() {
                interval = interval.saturating_sub(poll_interval);
                i += 1;
            }
            i
        };

        PollCount {
            polls: Wrapping(0),
            abs_humidity_interval: self.abs_humidity_interval,
            log_info_interval,
        }
    }
}

impl PollCount {
    pub(crate) fn add(&mut self) {
        self.polls += 1;
    }

    pub fn should_calc_abs_humidity(&self) -> bool {
        self.polls.0 % self.abs_humidity_interval == 0
    }

    pub fn should_log_info(&self) -> bool {
        self.polls.0 % self.log_info_interval == 0
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct State {
    status: StatusCell,

    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_atomic_bool"))]
    found: AtomicBool,
    poll_interval: Duration,
    #[cfg_attr(feature = "serde", serde(skip))]
    backoff: crate::retry::ExpBackoff,
}

impl Default for State {
    fn default() -> Self {
        Self {
            status: StatusCell::new(),
            found: AtomicBool::new(false),
            poll_interval: Duration::from_secs(2),
            backoff: crate::retry::ExpBackoff::default(),
        }
    }
}

#[cfg(feature = "serde")]
fn serialize_atomic_bool<S: serde::Serializer>(
    found: &AtomicBool,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    found.load(Ordering::Relaxed).serialize(serializer)
}

/// Given a temperature in Celcius and a relative humidity percentage, returns
/// an absolute humidity in grams/m^3.
// TODO(eliza): can we avoid some of the float math?
pub(crate) fn absolute_humidity(temp_c: f32, rel_humidity_percent: f32) -> f32 {
    // first, determine the saturation vapor pressure (`P_sat`) at `temp_c`
    // degrees --- the pressure when the relative humidity is 100%. we compute
    // this using a variant of the Magnus-Tetens formula:
    // (see https://doi.org/10.1175/1520-0493(1980)108%3C1046:TCOEPT%3E2.0.CO;2)
    let p_sat = 6.112 * ((17.64 * temp_c) / (temp_c + 243.5)).exp();
    // using `P_sat`, the pressure at 100% RH, we can compute `P`, the pressure
    // at the given relative humidity percentage, by multiplying:
    //     P = P_sat * (rel_humidity_percent / 100)
    // knowing the pressure, we then multiply `P` by the molecular weight
    // of water (18.02) to give the absolute humidity in grams/m^3.
    //
    // this calculation simplifies to:
    (p_sat * rel_humidity_percent * 2.1674) / (273.15 + temp_c)
    // see https://carnotcycle.wordpress.com/2012/08/04/how-to-convert-relative-humidity-to-absolute-humidity/
}
