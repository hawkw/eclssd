#![no_std]
use core::time::Duration;
use serde::{Deserialize, Serialize};

pub const MAX_SENSORS: usize = 16;

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
#[cfg_attr(feature = "fmt", derive(Debug))]
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
