use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{Gauge, HUMIDITY_METRICS},
    sensor::{PollCount, Sensor},
    storage::Store,
    SharedBus,
};
use core::fmt;
use core::time::Duration;
use eclss_api::SensorName;

use embedded_hal_async::{
    delay::DelayNs,
    i2c::{self, I2c},
};
use sgp30::{AsyncSgp30, Baseline};

pub struct Sgp30<I: 'static, D, S = ()> {
    sensor: AsyncSgp30<&'static SharedBus<I>, D>,
    tvoc: &'static Gauge,
    eco2: &'static Gauge,
    abs_humidity: &'static tinymetrics::GaugeFamily<'static, HUMIDITY_METRICS, SensorName>,
    calibration_polls: u32,
    last_good_baseline: Option<sgp30::Baseline>,
    polls: PollCount,
    store: S,
}

/// Wrapper type to add a `Display` implementation to the `sgp30` crate's error
/// type.
#[derive(Debug)]
pub enum Sgp30Error<E> {
    Sgp30(sgp30::Error<E>),
    SelfTestFailed,
    Saturated,
}

/// The baseline values.
#[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
struct StoredBaseline {
    /// CO₂eq baseline
    co2eq: u16,
    /// TVOC baseline
    tvoc: u16,
}

impl From<Baseline> for StoredBaseline {
    fn from(Baseline { co2eq, tvoc }: Baseline) -> Self {
        Self { co2eq, tvoc }
    }
}

impl From<StoredBaseline> for Baseline {
    fn from(StoredBaseline { co2eq, tvoc }: StoredBaseline) -> Self {
        Self { co2eq, tvoc }
    }
}

impl<I, D> Sgp30<I, D>
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
        Self {
            sensor: AsyncSgp30::new(&eclss.i2c, ADAFRUIT_SGP30_ADDR, delay),
            tvoc: metrics.tvoc_ppb.register(NAME).unwrap(),
            eco2: metrics.eco2_ppm.register(NAME).unwrap(),
            abs_humidity: &metrics.abs_humidity_grams_m3,
            calibration_polls: 0,
            last_good_baseline: None,
            store: (),
            polls: config.poll_counter(POLL_INTERVAL),
        }
    }

    pub fn with_storage<S: Store>(self, store: S) -> Sgp30<I, D, S> {
        Sgp30 {
            sensor: self.sensor,
            tvoc: self.tvoc,
            eco2: self.eco2,
            abs_humidity: self.abs_humidity,
            calibration_polls: self.calibration_polls,
            last_good_baseline: self.last_good_baseline,
            store,
            polls: self.polls,
        }
    }
}

const ADAFRUIT_SGP30_ADDR: u8 = 0x58;
const MAX_TVOC: u16 = 60_000;
const NAME: SensorName = SensorName::Sgp30;
// The SGP30 must be polled every second in order to ensure that the dynamic
// baseline calibration algorithm works correctly. Performing a measurement
// takes 12 ms, reading the raw H2 and ETOH signals takes 25 ms, and
// setting the humidity compensation and/or reading the baseline takes up to
// 10 ms, so we poll every 1000ms - 12ms - 10ms - 10ms - 25ms = 943 ms.
const POLL_INTERVAL: Duration = Duration::from_millis(1000 - 12 - 10 - 10 - 25);

impl<I, D, S> Sgp30<I, D, S>
where
    I: I2c,
    D: DelayNs,
    S: Store,
    S::Error: core::fmt::Display,
{
    async fn refresh_baseline(&mut self) {
        if self.last_good_baseline.is_some() {
            return;
        }

        match self.store.load::<StoredBaseline>().await {
            Ok(Some(baseline)) => {
                let baseline = baseline.into();
                info!("{NAME} loaded baseline from storage: {baseline:?}");
                self.last_good_baseline = Some(baseline);
            }
            Ok(None) => {}
            Err(error) => warn!("error loading {NAME} baseline from storage: {error}"),
        }
    }
}

impl<I, D, S> Sensor for Sgp30<I, D, S>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
    S: Store + 'static,
    S::Error: core::fmt::Display,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: Duration = POLL_INTERVAL;
    type Error = EclssError<Sgp30Error<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let serial = self
            .sensor
            .serial()
            .await
            .context("error reading SGP30 serial")?;
        info!("SGP30 serial number: {serial:?}");
        let featureset = self
            .sensor
            .get_feature_set()
            .await
            .context("error reading SGP30 feature set")?;
        info!("SGP30 featureset: {featureset:?}");
        let selftest = self
            .sensor
            .selftest()
            .await
            .context("error performing SGP30 self-test")?;
        if !selftest {
            return Err(Sgp30Error::SelfTestFailed.into());
        }

        self.sensor
            .force_init()
            .await
            .context("error initializing SGP30")?;

        self.refresh_baseline().await;

        if let Some(ref baseline) = self.last_good_baseline {
            info!("setting {NAME} baseline to {baseline:?}");
            self.sensor
                .set_baseline(baseline)
                .await
                .context("error setting SGP30 baseline")?;
        }

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let abs_h = self.abs_humidity.mean().and_then(|abs_h| {
            match sgp30::Humidity::from_f32(abs_h as f32) {
                Ok(h) => Some(h),
                Err(error) => {
                    warn!(
                        ?error,
                        "error converting absolute humidity {abs_h} to {NAME} format"
                    );
                    None
                }
            }
        });

        if let Some(h) = abs_h {
            self.sensor
                .set_humidity(Some(&h))
                .await
                .context("error setting humidity for SGP30")?;
        }

        let baseline = self
            .sensor
            .get_baseline()
            .await
            .map_err(|e| {
                let error = Sgp30Error::from(e);
                warn!(%error, "{NAME}: error reading baseline: {error}");
            })
            .ok();

        let sgp30::Measurement {
            tvoc_ppb,
            co2eq_ppm,
        } = self
            .sensor
            .measure()
            .await
            .context("error reading SGP30 measurements")?;

        let raw = self
            .sensor
            .measure_raw_signals()
            .await
            .map_err(|e| {
                let error = Sgp30Error::from(e);
                warn!(%error, "{NAME}: error reading raw signals: {error}");
            })
            .ok();

        if self.polls.should_log_info() {
            info!("{NAME:>9}: CO₂eq: {co2eq_ppm:>4} ppm, TVOC: {tvoc_ppb:>4} ppb");
            if let Some(sgp30::RawSignals { h2, ethanol }) = raw {
                info!("{NAME:>9}: H₂: {h2:>4}, Ethanol: {ethanol:>4}");
            }
        } else {
            debug!("{NAME}: CO₂eq: {co2eq_ppm} ppm, TVOC: {tvoc_ppb} ppb");
            if let Some(sgp30::RawSignals { h2, ethanol }) = raw {
                debug!("{NAME}: H₂: {h2}, Ethanol: {ethanol}");
            }
        }

        // Skip updating metrics until calibration completes.
        if self.calibration_polls <= 15 {
            info!(
                ?baseline,
                "{NAME} calibrating baseline for {}/15 seconds...", self.calibration_polls,
            );
            self.calibration_polls += 1;
            return Ok(());
        }

        // Sometimes the sensor just reads 60,000 ppb TVOC, which is its
        // maximum value. This generally seems to indicate that the sensor
        // is misbehaving, so just bail out and give it some time to settle down.
        if tvoc_ppb == MAX_TVOC {
            return Err(Sgp30Error::Saturated.into());
        }

        self.tvoc.set_value(tvoc_ppb as f64);
        self.eco2.set_value(co2eq_ppm as f64);

        if let Some(baseline) = baseline {
            if self.last_good_baseline.as_ref() != Some(&baseline) {
                trace!("{NAME}: new basaeline: {baseline:?}");
                let stored = StoredBaseline::from(baseline.clone());
                self.last_good_baseline = Some(baseline);
                if let Err(error) = self.store.store(&stored).await {
                    warn!("error loading {NAME} baseline from storage: {error}")
                }
            }
        }

        self.polls.add();

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

    fn should_reset(&self) -> bool {
        matches!(self, Self::Saturated)
    }
}

impl<E: fmt::Display> fmt::Display for Sgp30Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sgp30(sgp30::Error::I2cRead(i)) => write!(f, "{NAME} I2C read error: {i}"),
            Self::Sgp30(sgp30::Error::I2cWrite(i)) => write!(f, "{NAME} I2C write error: {i}"),
            Self::Sgp30(sgp30::Error::Crc) => write!(f, "{NAME} CRC checksum validation failed"),
            Self::Sgp30(sgp30::Error::NotInitialized) => write!(f, "{NAME} not initialized"),
            Self::SelfTestFailed => write!(f, "{NAME} self-test failed"),
            Self::Saturated => write!(f, "{NAME} TVOC measurement saturated"),
        }
    }
}
