use super::{ScdError, Shared};
use crate::{
    error::{Context, EclssError},
    sensor::Sensor,
    SharedBus,
};

use eclss_api::SensorName;
use embedded_hal::i2c;
use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use libscd::asynchronous::scd4x;

pub struct Scd40<I: 'static, D> {
    sensor: scd4x::Scd40<&'static SharedBus<I>, D>,
    state: Shared,
    delay: D,
}

impl<I, D> Scd40<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs + Clone,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        delay: D,
    ) -> Self {
        Self {
            sensor: scd4x::Scd40::new(&eclss.i2c, delay.clone()),
            state: Shared::new(eclss, NAME),
            delay,
        }
    }

    pub fn with_abs_humidity_interval(mut self, interval: usize) -> Self {
        self.state = self.state.with_abs_humidity_interval(interval);
        self
    }
}

const NAME: SensorName = SensorName::Scd40;

impl<I, D> Sensor for Scd40<I, D>
where
    I: I2c + 'static,
    I::Error: i2c::Error,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(5);
    type Error = EclssError<ScdError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        self.sensor
            .stop_periodic_measurement()
            .await
            .context("error stopping SCD40 periodic measurement")?;
        self.sensor
            .reinit()
            .await
            .context("error starting SCD40 periodic measurement")?;

        let serial = self
            .sensor
            .serial_number()
            .await
            .context("error reading SCD40 serial number")?;
        info!(serial, "Connected to SCD40 sensor");
        if !self
            .sensor
            .perform_self_test()
            .await
            .context("error performing SCD40 self test")?
        {
            Err(ScdError::SelfTest).context("SCD40 self test failed")?;
        }

        self.sensor
            .start_periodic_measurement()
            .await
            .context("error starting SCD40 periodic measurement")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        while !self
            .sensor
            .data_ready()
            .await
            .context("error seeing if SCD40 data is ready")?
        {
            self.delay.delay_ms(1).await;
        }
        let scd4x::Measurement {
            co2,
            temperature,
            humidity,
        } = self
            .sensor
            .read_measurement()
            .await
            .context("error reading SCD40 measurement")?;
        self.state.record_measurement(co2, temperature, humidity);
        Ok(())
    }
}
