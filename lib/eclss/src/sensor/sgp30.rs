use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{Gauge, SensorLabel, MAX_METRICS},
    sensor::Sensor,
    SharedBus,
};
use core::fmt;
use core::time::Duration;

use embedded_hal_async::{
    delay::DelayNs,
    i2c::{self, I2c},
};
use sgp30::AsyncSgp30;

pub struct Sgp30<I: 'static, D> {
    sensor: AsyncSgp30<&'static SharedBus<I>, D>,
    tvoc: &'static Gauge,
    eco2: &'static Gauge,
    abs_humidity: &'static tinymetrics::GaugeFamily<'static, MAX_METRICS, SensorLabel>,
    calibration_polls: usize,
}

/// Wrapper type to add a `Display` implementation to the `sgp30` crate's error
/// type.
#[derive(Debug)]
pub enum Sgp30Error<E> {
    Sgp30(sgp30::Error<E>),
    SelfTestFailed,
}

impl<I, D> Sgp30<I, D>
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
        Self {
            sensor: AsyncSgp30::new(&eclss.i2c, ADAFRUIT_SGP30_ADDR, delay),
            tvoc: metrics.tvoc.register(LABEL).unwrap(),
            eco2: metrics.eco2.register(LABEL).unwrap(),
            abs_humidity: &metrics.abs_humidity,
            calibration_polls: 0,
        }
    }
}

const NAME: &str = "SGP30";
const ADAFRUIT_SGP30_ADDR: u8 = 0x58;

impl<I, D> Sensor for Sgp30<I, D>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
{
    const NAME: &'static str = NAME;
    // The SGP30 must be polled every second in order to ensure that the dynamic
    // baseline calibration algorithm works correctly. Performing a measurement
    // takes 12 ms, reading the raw H2 and ETOH signals takes 25 ms, and
    // setting the humidity compensation takes up to 10 ms, so
    // we poll every 1000ms - 12ms - 10ms - 25ms = 953 ms.
    const POLL_INTERVAL: Duration = Duration::from_millis(1000 - 12 - 10 - 25);
    type Error = EclssError<Sgp30Error<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let serial = self
            .sensor
            .serial()
            .await
            .context("error reading SGP30 serial")?;
        tracing::info!("SGP30 serial number: {serial:?}");
        let featureset = self
            .sensor
            .get_feature_set()
            .await
            .context("error reading SGP30 feature set")?;
        tracing::info!("SGP30 featureset: {featureset:?}");
        let selftest = self
            .sensor
            .selftest()
            .await
            .context("error performing SGP30 self-test")?;
        if !selftest {
            return Err(Sgp30Error::SelfTestFailed.into());
        }
        self.sensor
            .init()
            .await
            .context("error initializing SGP30")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let abs_h = self.abs_humidity.mean().and_then(|abs_h| {
            match sgp30::Humidity::from_f32(abs_h as f32) {
                Ok(h) => Some(h),
                Err(error) => {
                    warn!(
                        ?error,
                        "error converting absolute humidity {abs_h} to SGP30 format"
                    );
                    None
                }
            }
        });
        let baseline = if let Some(h) = abs_h {
            self.sensor
                .set_humidity(Some(&h))
                .await
                .context("error setting humidity for SGP30")?;
            None
        } else {
            let baseline = self
                .sensor
                .get_baseline()
                .await
                .context("error reading SGP30 baseline")?;
            Some(baseline)
        };

        let sgp30::Measurement {
            tvoc_ppb,
            co2eq_ppm,
        } = self
            .sensor
            .measure()
            .await
            .context("error reading SGP30 measurements")?;
        if self.calibration_polls <= 15 {
            tracing::info!(
                ?baseline,
                "SGP30 calibrating baseline for {}/15 seconds...",
                self.calibration_polls,
            );
            self.calibration_polls += 1;
        } else {
            self.tvoc.set_value(tvoc_ppb.into());
            self.eco2.set_value(co2eq_ppm.into());
        }
        tracing::debug!("CO₂eq: {co2eq_ppm} ppm, TVOC: {tvoc_ppb} ppb");

        let sgp30::RawSignals { h2, ethanol } = self
            .sensor
            .measure_raw_signals()
            .await
            .context("error reading SGP30 raw signals")?;
        tracing::debug!("H₂: {h2}, Ethanol: {ethanol}");

        if let Some(baseline) = baseline {
            tracing::trace!("SGP30 baseline: {baseline:?}");
        }

        Ok(())
    }
}

impl<E: i2c::Error> From<sgp30::Error<E>> for Sgp30Error<E> {
    fn from(e: sgp30::Error<E>) -> Self {
        Self::Sgp30(e)
    }
}

impl<E: i2c::Error> SensorError for Sgp30Error<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            Self::Sgp30(sgp30::Error::I2cRead(i)) => Some(i.kind()),
            Self::Sgp30(sgp30::Error::I2cWrite(i)) => Some(i.kind()),
            _ => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for Sgp30Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sgp30(sgp30::Error::I2cRead(i)) => write!(f, "SGP30 I2C read error: {i}"),
            Self::Sgp30(sgp30::Error::I2cWrite(i)) => write!(f, "SGP30 I2C write error: {i}"),
            Self::Sgp30(sgp30::Error::Crc) => write!(f, "SGP30 CRC checksum validation failed"),
            Self::Sgp30(sgp30::Error::NotInitialized) => write!(f, "SGP30 not initialized"),
            Self::SelfTestFailed => f.write_str("SGP30 self-test failed"),
        }
    }
}
