use crate::{metrics, retry, Eclss};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use embedded_hal::i2c;
use embedded_hal_async::delay::DelayNs;
mod status;

#[cfg(feature = "pmsa003i")]
pub mod pmsa003i;

#[cfg(feature = "pmsa003i")]
pub use pmsa003i::Pmsa003i;

#[cfg(feature = "scd40")]
pub mod scd40;

#[cfg(feature = "scd40")]
pub use scd40::Scd40;

pub use self::status::{Status, StatusCell};

use tinymetrics::registry::RegistryMap;

#[allow(async_fn_in_trait)]
pub trait Sensor {
    type Error: SensorError;

    const NAME: &'static str;
    const POLL_INTERVAL: Duration;

    async fn init(&mut self) -> Result<(), Self::Error>;

    async fn poll(&mut self) -> Result<(), Self::Error>;
}

pub trait SensorError {
    fn i2c_error(&self) -> Option<i2c::ErrorKind>;

    fn as_status(&self) -> Status {
        match self.i2c_error() {
            None => Status::SensorError,
            Some(i2c::ErrorKind::NoAcknowledge(_)) => Status::NoAcknowledge,
            Some(i2c::ErrorKind::Bus) => Status::BusError,
            Some(_) => Status::OtherI2cError,
        }
    }
}

impl<I, const SENSORS: usize> Eclss<I, { SENSORS }> {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            name = "sensor",
            level = tracing::Level::INFO,
            skip(self, retry_backoff, delay, sensor),
            fields(message = %S::NAME)
        )
    )]
    pub async fn run_sensor<S>(
        &'static self,
        mut sensor: S,
        retry_backoff: impl Into<retry::ExpBackoff>,
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
                    backoff: retry_backoff.into(),
                    ..Default::default()
                },
            )
            .ok_or("insufficient space in sensor registry")?;
        let errors = self
            .metrics
            .sensor_errors
            .register(metrics::SensorLabel(S::NAME))
            .ok_or("insufficient space in sensor errors metric")?;

        while let Err(error) = sensor.init().await {
            warn!(%error, "failed to initialize {}, retrying...", S::NAME);
            errors.fetch_add(1);
            backoff.wait(&mut delay).await;
        }

        backoff.reset();
        info!("initialized {}", S::NAME);

        loop {
            while let Err(error) = sensor.poll().await {
                warn!(
                    %error,
                    retry_in = ?backoff.current(),
                    "failed to poll {}, retrying...", S::NAME
                );
                status.set_status(error.as_status());
                errors.fetch_add(1);
                backoff.wait(&mut delay).await;
            }
            status.set_status(Status::Up);
            delay.delay_ms(poll_interval.as_millis() as u32).await;
        }
    }
}

pub(crate) type Registry<const N: usize> = RegistryMap<&'static str, State, { N }>;

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub(crate) struct State {
    status: StatusCell,

    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_atomic_bool"))]
    found: AtomicBool,
    poll_interval: Duration,
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
