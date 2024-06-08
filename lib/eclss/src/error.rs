use crate::sensor;
use core::fmt;
use embedded_hal::i2c;

pub trait SensorError {
    fn i2c_error(&self) -> Option<i2c::ErrorKind>;

    fn as_status(&self) -> sensor::Status {
        match self.i2c_error() {
            None => sensor::Status::SensorError,
            Some(i2c::ErrorKind::NoAcknowledge(_)) => sensor::Status::NoAcknowledge,
            Some(i2c::ErrorKind::Bus) => sensor::Status::BusError,
            Some(_) => sensor::Status::OtherI2cError,
        }
    }
}

pub trait Context<T, E> {
    fn context(self, msg: &'static str) -> Result<T, EclssError<E>>;
}

#[derive(Debug)]
pub struct EclssError<E> {
    msg: Option<&'static str>,
    error: E,
}

#[derive(Debug)]
pub struct I2cSensorError<E>(pub(crate) E);

impl<E: i2c::Error> SensorError for I2cSensorError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        Some(self.0.kind())
    }
}

impl<E: fmt::Display> fmt::Display for I2cSensorError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<T, E, E2> Context<T, E2> for Result<T, E>
where
    E2: From<E>,
{
    fn context(self, msg: &'static str) -> Result<T, EclssError<E2>> {
        self.map_err(move |error| EclssError {
            error: error.into(),
            msg: Some(msg),
        })
    }
}

impl<E: SensorError> SensorError for EclssError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        self.error.i2c_error()
    }
}

impl<E: fmt::Display> fmt::Display for EclssError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { msg, error } = self;
        if let Some(msg) = msg {
            write!(f, "{msg}: {error}")
        } else {
            fmt::Display::fmt(error, f)
        }
    }
}

impl<E> From<E> for EclssError<E> {
    fn from(error: E) -> Self {
        Self { error, msg: None }
    }
}
