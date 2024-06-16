use super::{ScdError, SensorName, Shared};
use crate::{
    error::{Context, EclssError},
    sensor::Sensor,
    SharedBus,
};

use embedded_hal::i2c;
use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use libscd::asynchronous::scd4x;

pub struct Scd41<I: 'static, D> {
    sensor: scd4x::Scd41<&'static SharedBus<I>, D>,
    state: Shared,
    delay: D,
}

impl<I, D> Scd41<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs + Clone,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        delay: D,
    ) -> Self {
        Self {
            sensor: scd4x::Scd41::new(&eclss.i2c, delay.clone()),
            state: Shared::new(eclss, NAME),
            delay,
        }
    }

    pub fn with_abs_humidity_interval(mut self, interval: usize) -> Self {
        self.state = self.state.with_abs_humidity_interval(interval);
        self
    }
}

const NAME: SensorName = SensorName::Scd41;

impl<I, D> Sensor for Scd41<I, D>
where
    I: I2c + 'static,
    I::Error: i2c::Error + core::fmt::Debug,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(5);
    type Error = EclssError<ScdError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        match self.sensor.wake_up().await {
            Ok(()) => tracing::debug!("Woke up SCD41"),
            Err(error) => tracing::warn!(?error, "error waking up SCD41: {error:?}"),
        }

        self.sensor
            .stop_periodic_measurement()
            .await
            .context("error stopping SCD41 periodic measurement")?;
        self.sensor
            .reinit()
            .await
            .context("error starting SCD41 periodic measurement")?;

        let serial = self
            .sensor
            .serial_number()
            .await
            .context("error reading SCD41 serial number")?;
        info!(serial, "Connected to SCD41 sensor");
        if !self
            .sensor
            .perform_self_test()
            .await
            .context("error performing SCD41 self test")?
        {
            Err(ScdError::SelfTest).context("SCD41 self test failed")?;
        }

        self.sensor
            .start_periodic_measurement()
            .await
            .context("error starting SCD41 periodic measurement")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        while !self
            .sensor
            .data_ready()
            .await
            .context("error seeing if SCD41 data is ready")?
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
            .context("error reading SCD41 measurement")?;
        self.state.record_measurement(co2, temperature, humidity);
        if let Some(pressure) = self.state.pressure_pascals() {
            self.sensor
                .set_ambient_pressure(pressure)
                .await
                .context("error setting SCD41 ambient pressure")?;
        }
        Ok(())
    }
}
