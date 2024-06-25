#![cfg_attr(not(feature = "std"), no_std)]
use core::time::Duration;
use embedded_hal::i2c;
use embedded_hal_async::i2c::I2c;
use maitake_sync::Mutex;

#[macro_use]
mod trace;

pub use self::metrics::SensorMetrics;
pub mod error;
pub mod metrics;
pub mod retry;
pub mod sensor;
pub mod storage;

pub struct Eclss<I, const SENSORS: usize> {
    pub(crate) metrics: SensorMetrics,
    pub(crate) i2c: SharedBus<I>,
    pub(crate) sensors: sensor::Registry<SENSORS>,
}

/// Global ECLSS configuration.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[non_exhaustive]
pub struct Config {
    /// Maximum number of attempts to initialize a sensor.
    ///
    /// If this argument is present, sensor initialization will permanently fail
    /// after this many attempts. Otherwise, the ECLSS daemon will continue to
    /// retry sensor initialization indefinitely.
    ///
    /// Use this setting if the daemon should fail to start up if some expected
    /// sensors are missing. Do not use this setting if you intend to hot-plug
    /// sensors.
    #[cfg_attr(feature = "clap", clap(long))]
    pub max_init_attempts: Option<usize>,

    /// Interval for calculating absolute humidity from relative humidity
    /// readings.
    ///
    /// By default, this is calculated every time a relative humidity sensor is
    /// polled. To reduce CPU load, especially on devices without hardware
    /// floating-point units, this frequency can be reduced.
    #[cfg_attr(feature = "clap", clap(long, default_value_t = 1))]
    pub abs_humidity_interval: u32,

    /// If provided, sensor readings will be logged at the INFO level with this
    /// frequency.
    #[cfg_attr(
        feature = "clap",
        clap(
            long,
            default_value = "30s",
            value_parser = humantime::parse_duration,
        ),
    )]
    pub log_reading_interval: Duration,

    /// Retry configuration.
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub retries: retry::RetryConfig,
}

impl<I, const SENSORS: usize> Eclss<I, { SENSORS }> {
    pub const fn new(i2c: I) -> Self {
        Self {
            metrics: SensorMetrics::new(),
            i2c: SharedBus::new(i2c),
            sensors: sensor::Registry::new(),
        }
    }

    pub fn sensors(&self) -> &sensor::Registry<SENSORS> {
        &self.sensors
    }

    pub fn metrics(&self) -> &SensorMetrics {
        &self.metrics
    }
}

#[derive(Debug)]
pub struct SharedBus<I>(Mutex<I>);

impl<I> SharedBus<I> {
    pub const fn new(i2c: I) -> Self {
        SharedBus(Mutex::new(i2c))
    }
}

impl<I, A> I2c<A> for &'_ SharedBus<I>
where
    I: I2c<A>,
    A: i2c::AddressMode,
{
    async fn transaction(
        &mut self,
        address: A,
        operations: &mut [i2c::Operation<'_>],
    ) -> Result<(), <Self as i2c::ErrorType>::Error> {
        self.0.lock().await.transaction(address, operations).await
    }
}

impl<I> i2c::ErrorType for &'_ SharedBus<I>
where
    I: i2c::ErrorType,
{
    type Error = I::Error;
}
