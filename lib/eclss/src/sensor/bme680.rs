use crate::{
    error::{Context, EclssError, SensorError},
    metrics::Gauge,
    sensor::{PollCount, Sensor},
    SharedBus,
};
use bosch_bme680::{AsyncBme680, BmeError, MeasurmentData as MeasurementData};
use core::fmt;
use eclss_api::SensorName;
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
    polls: PollCount,
}

impl<I, D> Bme680<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        config: &crate::Config,
        delay: D,
    ) -> Self {
        let metrics = &eclss.metrics;

        // the default I2C address of the Adafruit BME680 breakout board
        // is the "secondary" address, 0x77.
        let address = bosch_bme680::DeviceAddress::Secondary;
        // TODO(eliza): get this from an ambient measurement...
        let ambient_temp = 20;
        Self {
            sensor: AsyncBme680::new(&eclss.i2c, address, delay, ambient_temp),
            temp: metrics.temp_c.register(NAME).unwrap(),
            pressure: metrics.pressure_hpa.register(NAME).unwrap(),
            rel_humidity: metrics.rel_humidity_percent.register(NAME).unwrap(),
            abs_humidity: metrics.abs_humidity_grams_m3.register(NAME).unwrap(),
            gas_resistance: metrics.gas_resistance.register(NAME).unwrap(),
            polls: config.poll_counter(POLL_INTERVAL),
        }
    }
}

#[derive(Debug)]
pub struct Error<E: embedded_hal::i2c::ErrorType>(BmeError<E>);

const NAME: SensorName = SensorName::Bme680;
const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(2);

impl<I, D> Sensor for Bme680<I, D>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
{
    const NAME: SensorName = SensorName::Bme680;
    const POLL_INTERVAL: core::time::Duration = POLL_INTERVAL;

    type Error = EclssError<Error<&'static SharedBus<I>>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let config = bosch_bme680::Configuration::default();
        self.sensor
            .initialize(&config)
            .await
            .context("error initializing BME680")?;
        info!("{NAME:>8}: initialized with config: {config:?}");
        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let mut timeouts = 0;
        let data = loop {
            match self.sensor.measure().await {
                Ok(data) => break data,
                // don't get into long backoffs on timeouts...
                Err(BmeError::MeasuringTimeOut) if timeouts < 5 => {
                    timeouts += 1;
                    info!("{NAME:>8}: measurement timed out, retrying...");
                }
                Err(BmeError::MeasuringTimeOut) => {
                    warn!("{NAME:>8}: timed out a bunch of times, giving up...");
                    return Ok(());
                }
                Err(e) => return Err(e).context("error reading BME680 measurements"),
            }
        };
        let MeasurementData {
            temperature,
            humidity,
            pressure,
            gas_resistance,
        } = data;
        self.polls.add();

        // pretty sure the `bosch-bme680` library is off by a factor of 100 when
        // representing pressures as hectopascals...
        let pressure = pressure / 100f32;
        self.pressure.set_value(pressure.into());
        self.temp.set_value(temperature.into());
        self.rel_humidity.set_value(humidity.into());
        if self.polls.should_log_info() {
            info!(
                "{NAME:>8}: Temp: {temperature:>3.2}°C, \
                Humidity: {humidity:>3.2}%, \
                Pressure: {pressure:>3.2} hPa",
            );
        } else {
            debug!(
                "{NAME:>8}: Temp: {temperature}°C, Humidity: {humidity}%, \
                 Pressure: {pressure} hPa"
            );
        }

        if let Some(gas_resistance) = gas_resistance {
            self.gas_resistance.set_value(gas_resistance.into());
            debug!("{NAME:>8}: Gas resistance: {gas_resistance} Ohms");
        }

        if self.polls.should_calc_abs_humidity() {
            let abs_humidity = super::absolute_humidity(temperature, humidity);
            self.abs_humidity.set_value(abs_humidity.into());
            if self.polls.should_log_info() {
                info!("{NAME:>8}: Absolute humidity: {abs_humidity:3.2} g/m³");
            } else {
                debug!("{NAME:>8}: Absolute humidity: {abs_humidity} g/m³");
            }
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
