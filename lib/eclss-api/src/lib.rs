#![cfg_attr(not(any(feature = "std", test)), no_std)]
use core::time::Duration;
use serde::{Deserialize, Serialize};

pub const MAX_SENSORS: usize = 16;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "fmt", derive(Debug))]
pub struct Metrics {
    pub abs_humidity_grams_m3: heapless::Vec<Measurement, MAX_SENSORS>,
    pub rel_humidity_percent: heapless::Vec<Measurement, MAX_SENSORS>,
    pub temp_c: heapless::Vec<Measurement, MAX_SENSORS>,
    pub co2_ppm: heapless::Vec<Measurement, MAX_SENSORS>,
    pub eco2_ppm: heapless::Vec<Measurement, MAX_SENSORS>,
    pub tvoc_ppb: heapless::Vec<Measurement, MAX_SENSORS>,
    pub pressure_hpa: heapless::Vec<Measurement, MAX_SENSORS>,
    pub sensor_errors: heapless::Vec<Measurement, MAX_SENSORS>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "fmt", derive(Debug))]
pub struct Measurement {
    pub value: f64,
    pub sensor: SensorName,
    pub timestamp: Option<u64>,
}

#[derive(
    Copy, Clone, PartialEq, Serialize, Deserialize, strum::IntoStaticStr, strum::EnumString,
)]
#[cfg_attr(feature = "fmt", derive(Debug, strum::Display))]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
#[repr(u8)]
#[non_exhaustive]
pub enum SensorName {
    Bme680,
    Ens160,
    Pmsa003i,
    Scd30,
    Scd40,
    Scd41,
    Sht41,
    Sgp30,
    Sen55,
}

#[cfg(feature = "tinymetrics")]
impl tinymetrics::FmtLabels for SensorName {
    fn fmt_labels(&self, f: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(f, "sensor=\"{}\"", self)
    }
}

// #[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
// #[cfg_attr(feature = "fmt", derive(Debug))]
// #[serde(rename_all = "UPPERCASE")]
// pub struct RhtMetrics {
//     pub bme680: Option<f64>,
//     pub scd30: Option<f64>,
//     pub scd40: Option<f64>,
//     pub scd41: Option<f64>,
//     pub sht41: Option<f64>,
// }

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "fmt", derive(Debug))]
#[non_exhaustive]
pub struct SensorState {
    status: SensorStatus,
    found: bool,
    poll_interval: Duration,
}

/// Represents the status of an I2C sensor.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "fmt", derive(Debug, strum::Display))]
#[repr(u8)]
#[non_exhaustive]
pub enum SensorStatus {
    /// A sensor of this type has never been initialized. It is likely that a
    /// sensor of this type is not connected to the bus.
    Unknown = 1,

    /// The sensor is initializing.
    Initializing,

    /// I2C address NAKed
    NoAcknowledge,

    /// The sensor is connected and healthy.
    Up,

    /// An error was returned by the sensor driver (not an I2C error).
    SensorError,

    /// An I2C bus error occurred while communicating with the sensor.
    BusError,

    /// Other errors
    OtherI2cError,
}

impl SensorStatus {
    pub fn from_u8(u: u8) -> Self {
        match u {
            u if u == Self::Unknown as u8 => Self::Unknown,
            u if u == Self::Initializing as u8 => Self::Initializing,
            u if u == Self::NoAcknowledge as u8 => Self::NoAcknowledge,
            u if u == Self::Up as u8 => Self::Up,
            u if u == Self::SensorError as u8 => Self::SensorError,
            u if u == Self::BusError as u8 => Self::BusError,
            u if u == Self::OtherI2cError as u8 => Self::OtherI2cError,
            // Weird status, assume missing?
            _ => Self::Unknown,
        }
    }

    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Unknown | Self::NoAcknowledge)
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Self::SensorError | Self::BusError | Self::OtherI2cError
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENSOR_KINDS: &[(&str, SensorName)] = &[
        ("BME680", SensorName::Bme680),
        ("ENS160", SensorName::Ens160),
        ("PMSA003I", SensorName::Pmsa003i),
        ("SCD30", SensorName::Scd30),
        ("SCD40", SensorName::Scd40),
        ("SCD41", SensorName::Scd41),
        ("SHT41", SensorName::Sht41),
        ("SGP30", SensorName::Sgp30),
        ("SEN55", SensorName::Sen55),
    ];

    #[test]
    fn sensor_name_from_str() {
        for &(s, name) in SENSOR_KINDS {
            assert_eq!(s.parse::<SensorName>(), Ok(name));
        }
    }

    #[test]
    fn sensor_name_from_str_lowercase() {
        for &(s, name) in SENSOR_KINDS {
            assert_eq!(s.to_ascii_lowercase().parse::<SensorName>(), Ok(name));
        }
    }

    #[test]
    fn sensor_name_display() {
        for &(s, name) in SENSOR_KINDS {
            assert_eq!(s, name.to_string());
        }
    }
}
