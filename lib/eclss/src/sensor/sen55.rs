use crate::{
    error::{Context, EclssError, SensorError},
    metrics::{DiameterLabel, Gauge},
    sensor::Sensor,
    SharedBus,
};
use core::num::Wrapping;
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
    delay: D,
    abs_humidity_interval: usize,
    polls: Wrapping<usize>,
}

impl<I, D> Sen55<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
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
            delay,
            polls: Wrapping(0),
            abs_humidity_interval: 1,
        }
    }
}

const NAME: SensorName = SensorName::Sen55;

impl<I, D> Sensor for Sen55<I, D>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    // The SGP30 must be polled every second in order to ensure that the dynamic
    // baseline calibration algorithm works correctly. Performing a measurement
    // takes 12 ms, reading the raw H2 and ETOH signals takes 25 ms, and
    // setting the humidity compensation takes up to 10 ms, so
    // we poll every 1000ms - 12ms - 10ms - 25ms = 953 ms.
    const POLL_INTERVAL: Duration = Duration::from_secs(1);
    type Error = EclssError<Sen5xError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        let product_name = self
            .sensor
            .read_product_name(&mut self.delay)
            .await
            .context("failed to read SEN5x product name")?;
        let name = product_name.as_str();
        tracing::info!("Connected to {name}...");
        self.sensor
            .start_measurement(ParticulateMode::Enabled, &mut self.delay)
            .await
            .context("failed to start SEN5x measurement")?;

        tracing::info!("Started {name} measurements");

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
        let voc = measurement.voc_index();
        let nox_index = measurement.nox_index();

        debug!(
            "{NAME}: Temp: {temp:?}°C, Humidity: {rel_humidity:?}, VOC: {voc:?}, NOx: {nox_index:?}, ready: {ready}"
        );
        let pm1_0 = measurement.pm1_0();
        let pm2_5 = measurement.pm2_5();
        let pm4_0 = measurement.pm4_0();
        let pm10_0 = measurement.pm10_0();
        debug!("{NAME}: PM1.0: {pm1_0:?}, PM2.5: {pm2_5:?}, PM4.0: {pm4_0:?}, PM10.0: {pm10_0:?}, ready: {ready}");

        if ready {
            if let Some(humidity) = rel_humidity {
                self.rel_humidity.set_value(humidity as f64);
            }
            if let Some(temp) = temp {
                self.temp.set_value(temp as f64);
            }

            macro_rules! update_particulates {
                ($($name:ident),+) => {
                    $(
                        if let Some(pm) = $name {
                            self.$name.set_value(pm.into());
                        }
                    )+
                }
            }

            update_particulates!(pm1_0, pm2_5, pm4_0, pm10_0);

            if let (Some(temp), Some(humidity)) = (temp, rel_humidity) {
                if self.polls.0 % self.abs_humidity_interval == 0 {
                    let abs_humidity = super::absolute_humidity(temp, humidity);
                    self.abs_humidity.set_value(abs_humidity.into());
                    debug!("{NAME}: Absolute humidity: {abs_humidity} g/m³",);
                }

                self.polls += 1;
            }
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
