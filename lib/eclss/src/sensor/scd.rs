use crate::{
    error::SensorError,
    metrics::{Gauge, SensorLabel, PRESSURE_METRICS},
};
use core::fmt;
use core::num::Wrapping;

use embedded_hal::i2c;

#[cfg(feature = "scd30")]
mod scd30;
#[cfg(feature = "scd41")]
pub use self::scd30::Scd30;
#[cfg(feature = "scd40")]
mod scd40;
#[cfg(feature = "scd40")]
pub use self::scd40::Scd40;
#[cfg(feature = "scd41")]
mod scd41;
#[cfg(feature = "scd41")]
pub use self::scd41::Scd41;

#[derive(Debug)]
pub enum ScdError<E> {
    Libscd(libscd::error::Error<E>),
    SelfTest,
}

struct Shared {
    temp_c: &'static Gauge,
    rel_humidity: &'static Gauge,
    abs_humidity: &'static Gauge,
    co2_ppm: &'static Gauge,
    abs_humidity_interval: usize,
    pressure: &'static tinymetrics::GaugeFamily<'static, PRESSURE_METRICS, SensorLabel>,
    polls: Wrapping<usize>,
    name: &'static str,
}

impl Shared {
    fn new<I, const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        name: &'static str,
    ) -> Self {
        let metrics = &eclss.metrics;
        Self {
            temp_c: metrics.temp.register(SensorLabel(name)).unwrap(),
            rel_humidity: metrics.rel_humidity.register(SensorLabel(name)).unwrap(),
            abs_humidity: metrics.abs_humidity.register(SensorLabel(name)).unwrap(),
            co2_ppm: metrics.co2.register(SensorLabel(name)).unwrap(),
            pressure: &metrics.pressure,
            polls: Wrapping(0),
            abs_humidity_interval: 1,
            name,
        }
    }

    fn with_abs_humidity_interval(mut self, interval: usize) -> Self {
        self.abs_humidity_interval = interval;
        self
    }

    fn pressure_pascals(&self) -> Option<u32> {
        let pressure_hpa = self.pressure.mean()?;
        let pressure_pascals = (pressure_hpa * 100.0) as u32;
        // Valid pressure compensation values per the SCDxx datasheet.
        const VALID_PRESSURES: core::ops::Range<u32> = 70_000..120_000;
        if VALID_PRESSURES.contains(&pressure_pascals) {
            Some(pressure_pascals)
        } else {
            None
        }
    }

    fn record_measurement(&mut self, co2: u16, temperature: f32, humidity: f32) {
        debug!(
            "{}: CO2: {co2} ppm, Temp: {temperature}°C, Humidity: {humidity}%",
            self.name
        );
        self.co2_ppm.set_value(co2.into());
        self.temp_c.set_value(temperature.into());
        self.rel_humidity.set_value(humidity.into());

        if self.polls.0 % self.abs_humidity_interval == 0 {
            let abs_humidity = super::absolute_humidity(temperature, humidity);
            self.abs_humidity.set_value(abs_humidity.into());
            debug!("{}: Absolute humidity: {abs_humidity} g/m³", self.name);
        }

        self.polls += 1;
    }
}

impl<E: i2c::Error> SensorError for ScdError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            Self::Libscd(libscd::error::Error::I2C(ref e)) => Some(e.kind()),
            _ => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for ScdError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Libscd(libscd::error::Error::I2C(ref e)) => write!(f, "I2C error: {e}"),
            Self::Libscd(libscd::error::Error::CRC) => {
                f.write_str("CRC checksum validation failed")
            }
            Self::Libscd(libscd::error::Error::InvalidInput) => f.write_str("invalid input"),
            Self::Libscd(libscd::error::Error::NotAllowed) => {
                f.write_str("not allowed when periodic measurement is running")
            }
            Self::SelfTest => f.write_str("self-test validation failed"),
        }
    }
}

impl<E> From<libscd::error::Error<E>> for ScdError<E> {
    fn from(e: libscd::error::Error<E>) -> Self {
        Self::Libscd(e)
    }
}
