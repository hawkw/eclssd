use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{Gauge, SensorLabel, MAX_METRICS},
    sensor::Sensor,
    SharedBus,
};
use core::fmt;

use embedded_hal::i2c;
use embedded_hal_async::i2c::I2c;

pub struct Ens160<I: 'static> {
    sensor: ens160::Ens160<&'static SharedBus<I>>,
    tvoc: &'static Gauge,
    eco2: &'static Gauge,
    temp: &'static tinymetrics::GaugeFamily<'static, MAX_METRICS, SensorLabel>,
    rel_humidity: &'static tinymetrics::GaugeFamily<'static, MAX_METRICS, SensorLabel>,
}

#[derive(Debug)]
pub enum Ens160Error<E> {
    I2c(E),
    Invalid,
}

// I2C address of the Adafruit breakout board.
// TODO(eliza): allow configuring this to support other ENS160 parts...
const ADAFRUIT_ENS160_ADDR: u8 = 0x53;

impl<I> Ens160<I>
where
    I: I2c<i2c::SevenBitAddress>,
{
    pub fn new<const SENSORS: usize>(eclss: &'static crate::Eclss<I, { SENSORS }>) -> Self {
        let metrics = &eclss.metrics;
        const LABEL: SensorLabel = SensorLabel(NAME);
        Self {
            sensor: ens160::Ens160::new(&eclss.i2c, ADAFRUIT_ENS160_ADDR),
            tvoc: metrics.tvoc.register(LABEL).unwrap(),
            eco2: metrics.eco2.register(LABEL).unwrap(),
            temp: &metrics.temp,
            rel_humidity: &metrics.rel_humidity,
        }
    }
}

const NAME: &str = "ENS160";

impl<I> Sensor for Ens160<I>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
{
    const NAME: &'static str = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(2);
    type Error = EclssError<Ens160Error<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let part_id = self
            .sensor
            .part_id()
            .await
            .map_err(Ens160Error::I2c)
            .context("error reading ENS160 part ID")?;
        info!("ENS160 part ID: 0x{part_id:04x}");

        let (min, minor, patch) = self
            .sensor
            .firmware_version()
            .await
            .map_err(Ens160Error::I2c)
            .context("error reading ENS160 firmware version")?;
        info!("ENS160 firmware version: v{min}.{minor}.{patch}");

        self.sensor
            .operational()
            .await
            .map_err(Ens160Error::I2c)
            .context("error setting ENS160 to operational mode")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        if let Some(avg_temp) = self.temp.mean() {
            // per the docs: Unit is scaled by 100. For example, a temperature
            // value of 2550 should be used for 25.50 °C.
            let integer = avg_temp.trunc() as i16 * 100;
            let fractional = (avg_temp.fract() * 100.0) as i16;
            let temp = integer + fractional;
            debug!("setting ENS160 temp compensation to {temp} ({avg_temp} C)");
            self.sensor
                .set_temp(temp)
                .await
                .map_err(Ens160Error::I2c)
                .context("error setting current temperature for ENS160")?;
        }

        if let Some(avg_rh) = self.rel_humidity.mean() {
            // per the docs: Unit is scaled by 100. For example, a temperature
            // value of 2550 should be used for 25.50 °C.
            let integer = avg_rh.trunc() as i16 * 100;
            let fractional = (avg_rh.fract() * 100.0) as i16;
            let temp = integer + fractional;
            debug!("setting ENS160 relative humidity compensation to {temp} ({avg_rh}%)");
            self.sensor
                .set_temp(temp)
                .await
                .map_err(Ens160Error::I2c)
                .context("error setting current temperature for ENS160")?;
        }

        let status = self
            .sensor
            .status()
            .await
            .map_err(Ens160Error::I2c)
            .context("error reading ENS160 status")?;
        match status.validity_flag() {
            // we are in operating mode. read the sensor!
            ens160::Validity::NormalOperation => {}
            v @ ens160::Validity::WarmupPhase | v @ ens160::Validity::InitStartupPhase => {
                info!("ENS160 is not ready yet: {v:?}");
                return Ok(());
            }
            ens160::Validity::InvalidOutput => {
                warn!("ENS160 status: invalid output!");
                return Err(Ens160Error::Invalid.into());
            }
        }

        let tvoc = self
            .sensor
            .tvoc()
            .await
            .map_err(Ens160Error::I2c)
            .context("error reading ENS160 tVOC")?;
        debug!("ENS160 TVOC: {tvoc} ppb",);
        self.tvoc.set_value(tvoc.into());

        let eco2 = self
            .sensor
            .eco2()
            .await
            .map_err(Ens160Error::I2c)
            .context("error reading ENS160 eCO2")?;
        let eco2 = *eco2;
        debug!("ENS160 eCO2: {eco2} ppm");
        self.eco2.set_value(eco2.into());

        Ok(())
    }
}

impl<E: i2c::Error> SensorError for Ens160Error<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            Self::I2c(i) => Some(i.kind()),
            _ => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for Ens160Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I2c(i) => fmt::Display::fmt(i, f),
            Self::Invalid => write!(f, "invalid ENS160 sensor data"),
        }
    }
}
