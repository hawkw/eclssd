use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{self, Gauge},
    sensor::Sensor,
    SharedBus,
};
use core::fmt;
use core::num::Wrapping;
use scd4x::AsyncScd4x;

use embedded_hal::delay::DelayNs as BlockingDelayNs;
use embedded_hal::i2c;
use embedded_hal_async::delay::DelayNs as AsyncDelayNs;
use embedded_hal_async::i2c::I2c;

pub struct Scd4x<I: 'static, D> {
    sensor: AsyncScd4x<&'static SharedBus<I>, AsyncBlockingDelayNs<D>>,
    temp_c: &'static Gauge,
    rel_humidity: &'static Gauge,
    abs_humidity: &'static Gauge,
    co2_ppm: &'static Gauge,
    abs_humidity_interval: usize,
    polls: Wrapping<usize>,
}

impl<I, D> Scd4x<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: BlockingDelayNs,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        delay: D,
    ) -> Self {
        let metrics = &eclss.metrics;
        const LABEL: metrics::SensorLabel = metrics::SensorLabel(NAME);
        Self {
            sensor: AsyncScd4x::new(&eclss.i2c, AsyncBlockingDelayNs(delay)),
            temp_c: metrics.temp.register(LABEL).unwrap(),
            rel_humidity: metrics.rel_humidity.register(LABEL).unwrap(),
            abs_humidity: metrics.abs_humidity.register(LABEL).unwrap(),
            co2_ppm: metrics.co2.register(LABEL).unwrap(),
            polls: Wrapping(0),
            abs_humidity_interval: 1,
        }
    }

    pub fn with_abs_humidity_interval(mut self, interval: usize) -> Self {
        self.abs_humidity_interval = interval;
        self
    }
}

#[derive(Debug)]
pub struct Error<E>(scd4x::Error<E>);

struct AsyncBlockingDelayNs<D>(D);

impl<D: BlockingDelayNs> AsyncDelayNs for AsyncBlockingDelayNs<D> {
    async fn delay_ns(&mut self, ns: u32) {
        self.0.delay_ns(ns);
    }
}

#[cfg(feature = "scd41")]
const NAME: &str = "SCD41";
#[cfg(not(feature = "scd41"))]
const NAME: &str = "SCD40";

impl<I, D> Sensor for Scd4x<I, D>
where
    I: I2c + 'static,
    I::Error: i2c::Error,
    D: BlockingDelayNs,
{
    const NAME: &'static str = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(5);
    type Error = EclssError<Error<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        #[cfg(feature = "scd41")]
        self.sensor.wake_up().await;

        self.sensor
            .stop_periodic_measurement()
            .await
            .context("error stopping SCD4x periodic measurement")?;
        self.sensor
            .reinit()
            .await
            .context("error starting SCD4x periodic measurement")?;

        let serial = self
            .sensor
            .serial_number()
            .await
            .context("error reading SCD4x serial number")?;
        info!(serial, "Connected to {NAME} sensor");

        self.sensor
            .start_periodic_measurement()
            .await
            .context("error starting SCD4x periodic measurement")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let scd4x::types::SensorData {
            co2,
            temperature,
            humidity,
        } = self.sensor.measurement().await.map_err(Error)?;
        self.polls += 1;
        debug!("CO2: {co2} ppm, Temp: {temperature}°C, Humidity: {humidity}%");
        self.co2_ppm.set_value(co2.into());
        self.temp_c.set_value(temperature.into());
        self.rel_humidity.set_value(humidity.into());

        if self.polls.0 % self.abs_humidity_interval == 0 {
            let abs_humidity = super::absolute_humidity(temperature, humidity);
            self.abs_humidity.set_value(abs_humidity.into());
            debug!("Absolute humidity: {abs_humidity} g/m³");
        }
        Ok(())
    }
}

impl<E: i2c::Error> SensorError for Error<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self.0 {
            scd4x::Error::I2c(ref e) => Some(e.kind()),
            _ => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            scd4x::Error::I2c(ref e) => write!(f, "I2C error: {e}"),
            scd4x::Error::Crc => f.write_str("CRC checksum validation failed"),
            scd4x::Error::SelfTest => f.write_str("self-test measure failure"),
            scd4x::Error::NotAllowed => {
                f.write_str("not allowed when periodic measurement is running")
            }
            scd4x::Error::Internal => f.write_str("internal error"),
        }
    }
}

impl<E> From<scd4x::Error<E>> for Error<E> {
    fn from(e: scd4x::Error<E>) -> Self {
        Self(e)
    }
}
