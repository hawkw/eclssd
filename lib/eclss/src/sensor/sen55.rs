use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{DiameterLabel, Gauge},
    sensor::{PollCount, Sensor},
    SharedBus,
};
use core::time::Duration;
use eclss_api::SensorName;

use embedded_hal_async::{
    delay::DelayNs,
    i2c::{self, I2c},
};
use sensor_sen5x::{AsyncSen5x, Error as Sen5xError, ParticulateMode};

pub struct Sen55<I: 'static, D> {
    sensor: sensor_sen5x::AsyncSen5x<&'static SharedBus<I>>,
    rel_humidity: &'static Gauge,
    abs_humidity: &'static Gauge,
    temp: &'static Gauge,
    pm1_0: &'static Gauge,
    pm2_5: &'static Gauge,
    pm4_0: &'static Gauge,
    pm10_0: &'static Gauge,
    nox_index: &'static Gauge,
    voc_index: &'static Gauge,
    delay: D,
    last_warm_start_param: Option<u16>,
    polls: PollCount,
}

impl<I, D> Sen55<I, D>
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
        const fn diameter(diameter: &'static str) -> DiameterLabel {
            DiameterLabel {
                diameter,
                sensor: NAME,
            }
        }
        Self {
            sensor: AsyncSen5x::new(&eclss.i2c),
            rel_humidity: metrics.rel_humidity_percent.register(NAME).unwrap(),
            abs_humidity: metrics.abs_humidity_grams_m3.register(NAME).unwrap(),
            temp: metrics.temp_c.register(NAME).unwrap(),
            pm1_0: metrics.pm_conc.register(diameter("1.0")).unwrap(),
            pm2_5: metrics.pm_conc.register(diameter("2.5")).unwrap(),
            pm4_0: metrics.pm_conc.register(diameter("4.0")).unwrap(),
            pm10_0: metrics.pm_conc.register(diameter("10.0")).unwrap(),
            nox_index: metrics.nox_iaq_index.register(NAME).unwrap(),
            voc_index: metrics.tvoc_iaq_index.register(NAME).unwrap(),
            delay,
            polls: config.poll_counter(POLL_INTERVAL),
            last_warm_start_param: None,
        }
    }
}

const NAME: SensorName = SensorName::Sen55;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

impl<I, D> Sensor for Sen55<I, D>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: Duration = POLL_INTERVAL;
    type Error = EclssError<Sen5xError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        self.sensor
            .reset(&mut self.delay)
            .await
            .context("failed to reset SEN5x")?;

        let product_name = self
            .sensor
            .read_product_name(&mut self.delay)
            .await
            .context("failed to read SEN5x product name")?;
        let name = product_name.as_str();
        info!("Connected to {name}...");

        if let Some(param) = self.last_warm_start_param {
            info!("Setting {name} warm start param to {param}");
            self.sensor
                .set_warm_start_parameter(&mut self.delay, param)
                .await
                .context("failed to set SEN5x warm start parameter")?;
        }

        self.sensor
            .start_measurement(ParticulateMode::Enabled, &mut self.delay)
            .await
            .context("failed to start SEN5x measurement")?;

        info!("Started {name} measurements");

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let ready = self
            .sensor
            .data_ready(&mut self.delay)
            .await
            .context("failed to check if SEN5x data is ready")?;

        let measurement = self
            .sensor
            .read_measurements(&mut self.delay)
            .await
            .context("failed to read SEN5x measurement data")?;
        let temp = measurement.temp_c();
        let rel_humidity = measurement.relative_humidity();
        let voc_index = measurement.voc_index();
        let nox_index = measurement.nox_index();
        let pm1_0 = measurement.pm1_0();
        let pm2_5 = measurement.pm2_5();
        let pm4_0 = measurement.pm4_0();
        let pm10_0 = measurement.pm10_0();

        if self.polls.should_log_info() && ready {
            if let (Some(temp), Some(rh), Some(voc), Some(nox)) =
                (temp, rel_humidity, voc_index, nox_index)
            {
                info!("{NAME:>8}: Temp: {temp:>3.2}°C, Humidity: {rh:>3.2}%, VOC Index: {voc:>4.2}, NOx Index: {nox:>4.2}");
            }
        } else {
            debug!(
                "{NAME:>8}: Temp: {temp:?}°C, Humidity: {rel_humidity:?}, \
                VOC Index: {voc_index:?}, NOx Index: {nox_index:?}, ready: {ready}"
            );
            debug!("{NAME:>8}: PM1.0: {pm1_0:?}, PM2.5: {pm2_5:?}, PM4.0: {pm4_0:?}, PM10.0: {pm10_0:?}, ready: {ready}");
        }

        if ready {
            macro_rules! update_metrics {
                ($($name:ident),+) => {
                    $(
                        if let Some(pm) = $name {
                            self.$name.set_value(pm.into());
                        }
                    )+
                }
            }

            update_metrics!(
                rel_humidity,
                temp,
                nox_index,
                voc_index,
                pm1_0,
                pm2_5,
                pm4_0,
                pm10_0
            );

            if let (Some(temp), Some(humidity)) = (temp, rel_humidity) {
                if self.polls.should_calc_abs_humidity() {
                    let abs_humidity = super::absolute_humidity(temp, humidity);
                    self.abs_humidity.set_value(abs_humidity.into());
                    if self.polls.should_log_info() {
                        info!("{NAME:>8}: Absolute humidity: {abs_humidity:>3.2} g/m³",);
                    } else {
                        debug!("{NAME:>8}: Absolute humidity: {abs_humidity} g/m³",);
                    }
                }
            }

            self.polls.add();
        }

        match self.sensor.read_warm_start_parameter(&mut self.delay).await {
            Ok(param) => {
                self.last_warm_start_param = Some(param);
                trace!("{NAME:>8}: Warm start parameter: {param}");
            }
            Err(error) => warn!("{NAME:>8}: error reading warm start parameter: {error}"),
        }

        Ok(())
    }
}

impl<E: i2c::Error> SensorError for Sen5xError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            Self::I2cRead(i) => Some(i.kind()),
            Self::I2cWrite(i) => Some(i.kind()),
            _ => None,
        }
    }
}
