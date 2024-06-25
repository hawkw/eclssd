use crate::{
    error::{Context, EclssError, SensorError},
    metrics::Gauge,
    sensor::Sensor,
    SharedBus,
};
use core::{fmt, time::Duration};
use eclss_api::SensorName;
use embedded_hal_async::{
    delay::DelayNs,
    i2c::{self, I2c},
};
use sht4x::AsyncSht4x;
pub use sht4x::Precision;

use super::PollCount;

#[must_use = "sensors do nothing unless polled"]
pub struct Sht41<I: 'static, D> {
    sensor: AsyncSht4x<&'static SharedBus<I>, D>,
    temp: &'static Gauge,
    rel_humidity: &'static Gauge,
    abs_humidity: &'static Gauge,
    precision: Precision,
    polls: PollCount,
    delay: D,
}

pub struct Sht4xError<E>(sht4x::Error<E>);

const NAME: SensorName = SensorName::Sht41;

impl<I, D> Sht41<I, D>
where
    I: I2c + 'static,
    D: DelayNs,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        config: &crate::Config,
        delay: D,
    ) -> Self {
        let metrics = &eclss.metrics;
        // This is the default I2C address of the Adafruit breakout board.
        // TODO(eliza): make this configurable
        let address = sht4x::Address::Address0x44;

        Self {
            sensor: AsyncSht4x::new_with_address(&eclss.i2c, address),
            temp: metrics.temp_c.register(NAME).unwrap(),
            rel_humidity: metrics.rel_humidity_percent.register(NAME).unwrap(),
            abs_humidity: metrics.abs_humidity_grams_m3.register(NAME).unwrap(),
            polls: config.poll_counter(POLL_INTERVAL),
            precision: Precision::Medium,
            delay,
        }
    }

    pub fn with_precision(self, precision: Precision) -> Self {
        Self { precision, ..self }
    }
}

const POLL_INTERVAL: Duration = Duration::from_secs(1);

impl<I, D> Sensor for Sht41<I, D>
where
    I: I2c + 'static,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: Duration = POLL_INTERVAL;
    type Error = EclssError<Sht4xError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let serial = self
            .sensor
            .serial_number(&mut self.delay)
            .await
            .context("error reading SHT41 serial number")?;
        info!("Connected to {NAME}, serial number: {serial:#x}");
        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let reading = self
            .sensor
            .measure(self.precision, &mut self.delay)
            .await
            .context("error reading SHT41 measurement")?;

        let temp = reading.temperature_celsius().to_num::<f64>();
        let rel_humidity = reading.humidity_percent().to_num::<f64>();
        self.temp.set_value(temp);
        self.rel_humidity.set_value(rel_humidity);
        if self.polls.should_log_info() {
            info!("{NAME:>9}: Temp: {temp:>3.2}°C, Humidity: {rel_humidity:>3.2}%");
        } else {
            debug!("{NAME}: Temp: {temp}°C, Humidity: {rel_humidity}%");
        }

        if self.polls.should_calc_abs_humidity() {
            let abs_humidity = super::absolute_humidity(temp as f32, rel_humidity as f32);
            self.abs_humidity.set_value(abs_humidity.into());
            if self.polls.should_log_info() {
                info!("{NAME:>9}: Absolute humidity: {abs_humidity:02.2} g/m³");
            } else {
                debug!("{NAME}: Absolute humidity: {abs_humidity} g/m³");
            }
        }

        self.polls.add();

        Ok(())
    }
}

impl<E> From<sht4x::Error<E>> for Sht4xError<E> {
    fn from(value: sht4x::Error<E>) -> Self {
        Self(value)
    }
}

impl<E: i2c::Error> SensorError for Sht4xError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            Self(sht4x::Error::I2c(i)) => Some(i.kind()),
            _ => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for Sht4xError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self(sht4x::Error::I2c(i)) => fmt::Display::fmt(i, f),
            Self(sht4x::Error::Crc) => write!(f, "{NAME} CRC checksum validation failed"),
            Self(_) => write!(f, "unknown {NAME} error"),
        }
    }
}
