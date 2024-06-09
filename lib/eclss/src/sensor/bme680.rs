use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{Gauge, SensorLabel},
    sensor::Sensor,
    SharedBus,
};
use bosch_bme680::{AsyncBme680, BmeError, MeasurmentData as MeasurementData};
use core::fmt;
use core::num::Wrapping;
use embedded_hal_async::{
    delay::DelayNs,
    i2c::{self, Error as _, I2c},
};
pub struct Bme680<I: 'static, D> {
    sensor: AsyncBme680<&'static SharedBus<I>, D>,
    temp: &'static Gauge,
    rel_humidity: &'static Gauge,
    abs_humidity: &'static Gauge,
    pressure: &'static Gauge,
    gas_resistance: &'static Gauge,
    abs_humidity_interval: usize,
    polls: Wrapping<usize>,
}

impl<I, D> Bme680<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        delay: D,
    ) -> Self {
        let metrics = &eclss.metrics;
        const LABEL: SensorLabel = SensorLabel(NAME);

        // the default I2C address of the Adafruit BME680 breakout board
        // is the "secondary" address, 0x77.
        let address = bosch_bme680::DeviceAddress::Secondary;
        // TODO(eliza): get this from an ambient measurement...
        let ambient_temp = 20;
        Self {
            sensor: AsyncBme680::new(&eclss.i2c, address, delay, ambient_temp),
            temp: metrics.temp.register(LABEL).unwrap(),
            pressure: metrics.pressure.register(LABEL).unwrap(),
            rel_humidity: metrics.rel_humidity.register(LABEL).unwrap(),
            abs_humidity: metrics.abs_humidity.register(LABEL).unwrap(),
            gas_resistance: metrics.gas_resistance.register(LABEL).unwrap(),
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
pub struct Error<E: embedded_hal::i2c::ErrorType>(BmeError<E>);

const NAME: &str = "BME680";

impl<I, D> Sensor for Bme680<I, D>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
{
    const NAME: &'static str = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(2);
    type Error = EclssError<Error<&'static SharedBus<I>>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let config = bosch_bme680::Configuration::default();
        self.sensor
            .initialize(&config)
            .await
            .context("error initializing BME680")?;
        tracing::info!("initialized BME680 with config: {config:?}");
        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let MeasurementData {
            temperature,
            humidity,
            pressure,
            gas_resistance,
        } = self
            .sensor
            .measure()
            .await
            .context("error reading BME680 measurements")?;
        self.polls += 1;

        // pretty sure the `bosch-bme680` library is off by a factor of 100 when
        // representing pressures as hectopascals...
        let pressure = pressure / 100f32;
        self.pressure.set_value(pressure.into());
        self.temp.set_value(temperature.into());
        self.rel_humidity.set_value(humidity.into());
        tracing::debug!("Temp: {temperature}°C, Humidity: {humidity}%, Pressure: {pressure} hPa");

        if let Some(gas_resistance) = gas_resistance {
            self.gas_resistance.set_value(gas_resistance.into());
            tracing::debug!("Gas resistance: {gas_resistance} Ohms");
        }

        if self.polls.0 % self.abs_humidity_interval == 0 {
            let abs_humidity = super::absolute_humidity(temperature, humidity);
            self.abs_humidity.set_value(abs_humidity.into());
            debug!("Absolute humidity: {abs_humidity} g/m³");
        }

        Ok(())
    }
}

impl<E> SensorError for Error<E>
where
    E: embedded_hal::i2c::ErrorType,
{
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self.0 {
            BmeError::WriteError(ref e) => Some(e.kind()),
            BmeError::WriteReadError(ref e) => Some(e.kind()),
            _ => None,
        }
    }
}

impl<E> fmt::Display for Error<E>
where
    E: embedded_hal::i2c::ErrorType,
    E::Error: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            BmeError::WriteError(ref e) => write!(f, "I2C write error: {e}"),
            BmeError::WriteReadError(ref e) => write!(f, "I2C write-read error: {e}"),
            BmeError::MeasuringTimeOut => f.write_str("BME680 measurement timed out"),
            BmeError::UnexpectedChipId(id) => {
                write!(f, "unexpected BME680 chip ID {id:#04x} (expected 0x61)")
            }
            BmeError::Uninitialized => f.write_str("BME680 sensor hasn't been initialized yet"),
        }
    }
}

impl<E> From<BmeError<E>> for Error<E>
where
    E: embedded_hal::i2c::ErrorType,
{
    fn from(e: BmeError<E>) -> Self {
        Self(e)
    }
}
