#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[cfg(feature = "fmt")]
use core::fmt;

/// Driver for the PMSA003i sensor using the [`embedded_hal::i2c::I2c`] or
/// [`embedded_hal_async::i2c::I2c`] traits.
#[cfg_attr(feature = "fmt", derive(Debug))]
#[cfg(any(feature = "embedded-hal", feature = "embedded-hal-async"))]
pub struct Pmsa003i<I> {
    i2c: I,
    addr: u8,
}

/// A sensor reading from the PMSA003i sensor.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "fmt", derive(Debug))]
pub struct Reading {
    /// Particulate concentrations in µg/𝑚³.
    pub concentrations: Concentrations,

    /// Counts of particles of various diameters in 0.1L of air.
    pub counts: ParticleCounts,

    /// The sensor version field.
    pub sensor_version: u8,
}

/// Particulate concentrations in µg/㎥.
///
/// This is a separate struct from [`ParticleCounts`] so that they can have
/// separate [`fmt::Display`] implementations.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "fmt", derive(Debug))]
pub struct Concentrations {
    /// PM1.0 concentration in µg/𝑚³, under environmental atmospheric
    /// conditions.
    ///
    /// *Note*: I don't actually know what "under atmospheric environment" means
    /// but it says that in the datasheet. I am guessing this refers to humidity
    /// compensation?
    pub pm1_0: u16,
    /// PM1.0 concentration in µg/𝑚³, under standard atmospheric conditions.
    pub pm1_0_standard: u16,

    /// PM2.5 concentration in µg/𝑚³, under environmental atmospheric
    /// conditions.
    ///
    /// Note: I don't actually know what "under atmospheric environment" means
    /// but it says that in the datasheet...
    pub pm2_5: u16,
    /// PM2.5 concentration in µg/𝑚³, under standard atmospheric conditions.
    pub pm2_5_standard: u16,

    /// PM10.0 concentration in µg/𝑚³, under environmental atmospheric
    /// conditions.
    ///
    /// Note: I don't actually know what "under atmospheric environment" means
    /// but it says that in the datasheet...
    pub pm10_0: u16,
    /// PM10.0 concentration in µg/𝑚³, under standard atmospheric conditions.
    pub pm10_0_standard: u16,
}

/// Counts of particles of various diameters in 0.1L of air.
///
/// This is a separate struct from [`Concentrations`] so that they can have
/// separate [`fmt::Display`] implementations.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "fmt", derive(Debug))]
pub struct ParticleCounts {
    /// Number of particles with diameter >= 0.3 µm in 0.1L of air.
    pub particles_0_3um: u16,
    /// Number of particles with diameter >= 0.5 µm in 0.1L of air.
    pub particles_0_5um: u16,
    /// Number of particles with diameter >= 1.0 µm in 0.1L of air.
    pub particles_1_0um: u16,
    /// Number of particles with diameter >= 2.5 µ𝑚 in 0.1L of air.
    pub particles_2_5um: u16,
    /// Number of particles with diameter >= 5.0 µm in 0.1L of air.
    pub particles_5_0um: u16,
    /// Number of particles with diameter >= 10.0 µm in 0.1L of air.
    pub particles_10_0um: u16,
}

/// Errors returned by the [`Pmsa003i::read_blocking`] and
/// [`Pmsa003i::read_async`] methods.
#[cfg_attr(feature = "fmt", derive(Debug))]
#[cfg(any(feature = "embedded-hal", feature = "embedded-hal-async"))]
pub enum SensorError<E> {
    /// An error occurred while reading from the I²C bus.
    I2c(E),
    /// An error occurred while decoding the reading.
    Reading(ReadingError),
}

/// Errors returned while decoding a reading in [`Reading::from_bytes`].
#[cfg_attr(feature = "fmt", derive(Debug))]
pub enum ReadingError {
    /// The sum of the packet did not match the checksum.
    Checksum { sum: u16, checksum: u16 },
    /// The packet was missing the magic word.
    BadMagic(u16),
    /// The sensor sent an error code.
    ///
    /// **Note**: I couldn't find any documentation of what these error codes
    /// actually mean in the data sheet. I assume if it's non-zero, that's bad?
    ErrorCode(u8),
}

const MAGIC: u16 = 0x424d;
const PACKET_LEN: usize = 32;
pub const DEFAULT_I2C_ADDR: u8 = 0x12;

impl Reading {
    pub fn from_bytes(bytes: &[u8; PACKET_LEN]) -> Result<Self, ReadingError> {
        // Each PMSA003I packet consists of 16 16-bit words, read from I2C as 32
        // bytes. The last word is a checksum.

        // reads a 16-bit word from `offset`
        macro_rules! words {
                [$offset:expr] => {
                    u16::from_be_bytes([bytes[$offset], bytes[$offset + 1]])
                }
            }

        let magic = words![0];
        if magic != MAGIC {
            // you didn't say the magic words!
            return Err(ReadingError::BadMagic(magic));
        }

        if words![29] != 0 {
            // byte 29 is an error code
            return Err(ReadingError::ErrorCode(bytes[27]));
        }

        // last two bytes are the checksum so dont include them in the checksum.
        let sum = bytes[0..PACKET_LEN - 2]
            .iter()
            .map(|&byte| byte as u16)
            .sum();
        let checksum = words![PACKET_LEN - 2];
        if sum != checksum {
            return Err(ReadingError::Checksum { sum, checksum });
        }

        // bytes 0 and 1 are the magic, which we already looked at
        // bytes 2 and 3 are the length field, which i don't get why they send,
        // because the data sheet also tells us how long the packet is lol

        // now we get to the good stuff:
        let reading = Reading {
            concentrations: Concentrations {
                pm1_0_standard: words![4],
                pm2_5_standard: words![6],
                pm10_0_standard: words![8],

                pm1_0: words![10],
                pm2_5: words![12],
                pm10_0: words![14],
            },

            counts: ParticleCounts {
                particles_0_3um: words![16],
                particles_0_5um: words![18],
                particles_1_0um: words![20],
                particles_2_5um: words![22],
                particles_5_0um: words![24],
                particles_10_0um: words![26],
            },

            // remaining bytes are version, error code (not documented lol), and
            // the checksum, which we already looked at
            sensor_version: bytes[28],
        };

        Ok(reading)
    }
}

impl core::convert::TryFrom<&'_ [u8; PACKET_LEN]> for Reading {
    type Error = ReadingError;

    fn try_from(bytes: &[u8; PACKET_LEN]) -> Result<Self, Self::Error> {
        Reading::from_bytes(bytes)
    }
}

#[cfg(any(feature = "embedded-hal", feature = "embedded-hal-async"))]
impl<I> Pmsa003i<I> {
    /// Returns a new `Pmsa003i` instance with the default I²C address ([`DEFAULT_I2C_ADDR`]).
    #[must_use]
    pub const fn new(i2c: I) -> Self {
        Self {
            i2c,
            addr: DEFAULT_I2C_ADDR,
        }
    }

    /// Returns a new `Pmsa003i` instance with the specified I²C address.
    #[must_use]
    pub const fn with_addr(i2c: I, addr: u8) -> Self {
        Self { i2c, addr }
    }

    /// Take a reading from the sensor using [`embedded_hal::i2c`] blocking I²C.
    #[cfg(feature = "embedded-hal")]
    pub fn read_blocking(
        &mut self,
    ) -> Result<Reading, SensorError<<I as embedded_hal::i2c::ErrorType>::Error>>
    where
        I: embedded_hal::i2c::I2c,
    {
        let mut bytes = [0; PACKET_LEN];
        self.i2c
            .read(self.addr, &mut bytes[..])
            .map_err(SensorError::I2c)?;
        Reading::from_bytes(&bytes).map_err(SensorError::Reading)
    }

    /// Take a reading from the sensor using [`embedded_hal_async::i2c`] async I²C.
    #[cfg(feature = "embedded-hal-async")]
    pub async fn read_async(
        &mut self,
    ) -> Result<Reading, SensorError<<I as embedded_hal_async::i2c::ErrorType>::Error>>
    where
        I: embedded_hal_async::i2c::I2c,
    {
        let mut bytes = [0; PACKET_LEN];
        self.i2c
            .read(self.addr, &mut bytes[..])
            .await
            .map_err(SensorError::I2c)?;
        Reading::from_bytes(&bytes).map_err(SensorError::Reading)
    }
}

// === impl Error ===

#[cfg(feature = "fmt")]
impl<E> fmt::Display for SensorError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I2c(err) => write!(f, "PMSA003I I²C error: {err}"),
            Self::Reading(err) => fmt::Display::fmt(err, f),
        }
    }
}

#[cfg(feature = "fmt")]
impl fmt::Display for ReadingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checksum { sum, checksum } => write!(
                f,
                "PMSA003I packet checksum did not match (expected {checksum}, got {sum})"
            ),
            Self::BadMagic(actual) => write!(
                f,
                "PMSA003I didn't say the magic word (expected {MAGIC:#x}. got {actual:#x})"
            ),
            Self::ErrorCode(code) => write!(f, "PMSA003I sent error code {code:#x}"),
        }
    }
}

// === impl Reading ===

#[cfg(feature = "fmt")]
impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            concentrations,
            counts,
            sensor_version: _,
        } = self;
        concentrations.fmt(f)?;
        f.write_str(if f.alternate() { "\n" } else { "; " })?;
        counts.fmt(f)?;
        Ok(())
    }
}

// === impl Concentrations ===

#[cfg(feature = "fmt")]
impl Concentrations {
    pub const UNIT: &'static str = "µg/𝑚³";
}

#[cfg(feature = "fmt")]
impl fmt::Display for Concentrations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const UNIT: &str = Concentrations::UNIT;
        let Self {
            pm1_0,
            pm1_0_standard,
            pm2_5,
            pm2_5_standard,
            pm10_0,
            pm10_0_standard,
        } = self;
        let (delim, leading, extra_pad) = if f.alternate() {
            // let line_len =
            //     // width of "PM1.0:  " or "PM10.0: "
            //     8 +
            //     // width of a number plus two spaces and a comma
            //     width + 2 + UNIT.len() + 1 +
            //     // width of a number plus two spaces and (std)""
            //     width + 2 + UNIT.len() + 5;
            ("\n\t", "\t", " ")
        } else {
            const DELIM: &str = ", ";
            // let one_conc_len =
            // // width of "PM1.0: " or "PM10.0:"
            // 7 +
            // // width of a number plus two spaces and a comma
            // width + 2 + UNIT.len() + 1 +
            // // width of a number plus two spaces and (std)""
            // width + 2 + UNIT.len() + 5;
            // let line_len =
            //     // three concentrations
            //     (one_conc_len * 3) +
            //     1 + // one extra character for "PM10.0"
            //     (DELIM.len() * 2);
            (DELIM, "", "")
        };
        let width = f.width().unwrap_or(0);

        write!(
            f,
            "{leading}{extra_pad}PM1.0: {pm1_0:>width$} {UNIT}, {pm1_0_standard:>width$} {UNIT} (std){delim}\
            {extra_pad}PM2.5: {pm2_5:>width$} {UNIT}, {pm2_5_standard:>width$} {UNIT} (std){delim}\
            PM10.0: {pm10_0:>width$} {UNIT}, {pm10_0_standard:>width$} {UNIT} (std)",
        )?;

        Ok(())
    }
}

// === impl ParticleCounts ===
#[cfg(feature = "fmt")]
impl ParticleCounts {
    pub const UNIT: &'static str = "per 0.1L";
}

#[cfg(feature = "fmt")]
impl fmt::Display for ParticleCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const UNIT: &str = ParticleCounts::UNIT;
        const UM: &str = "µ𝑚";
        let Self {
            particles_0_3um,
            particles_0_5um,
            particles_1_0um,
            particles_2_5um,
            particles_5_0um,
            particles_10_0um,
        } = self;
        let (delim, leading, extra_pad) = if f.alternate() {
            ("\n\t", "\n\t", " ")
        } else {
            (", ", " ", "")
        };
        // TODO(eliza): support using the formatter's fill char?
        write!(
            f,
            "{UNIT} of air:{leading}\
            >= {extra_pad}0.3{UM}: {particles_0_3um:>width$}{delim}\
            >= {extra_pad}0.5{UM}: {particles_0_5um:>width$}{delim}\
            >= {extra_pad}1.0{UM}: {particles_1_0um:>width$}{delim}\
            >= {extra_pad}2.5{UM}: {particles_2_5um:>width$}{delim}\
            >= {extra_pad}5.0{UM}: {particles_5_0um:>width$}{delim}\
            >= 10.0{UM}: {particles_10_0um:>width$}",
            width = f.width().unwrap_or(0),
        )?;

        Ok(())
    }
}
