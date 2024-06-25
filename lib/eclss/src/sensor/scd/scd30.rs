use super::{ScdError, Shared};
use crate::{
    error::{Context, EclssError},
    sensor::Sensor,
    SharedBus,
};
use eclss_api::SensorName;
use embedded_hal::i2c;
use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use libscd::asynchronous::scd30;

pub struct Scd30<I: 'static, D> {
    sensor: scd30::Scd30<&'static SharedBus<I>, D>,
    delay: D,
    state: Shared,
}

impl<I, D> Scd30<I, D>
where
    I: I2c<i2c::SevenBitAddress>,
    D: DelayNs + Clone,
{
    pub fn new<const SENSORS: usize>(
        eclss: &'static crate::Eclss<I, { SENSORS }>,
        config: &crate::Config,
        delay: D,
    ) -> Self {
        Self {
            sensor: scd30::Scd30::new(&eclss.i2c, delay.clone()),
            state: Shared::new(eclss, config, NAME, POLL_INTERVAL),
            delay,
        }
    }
}

const NAME: SensorName = SensorName::Scd30;
const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(2);

impl<I, D> Sensor for Scd30<I, D>
where
    I: I2c + 'static,
    I::Error: i2c::Error,
    D: DelayNs,
{
    const NAME: SensorName = NAME;
    const POLL_INTERVAL: core::time::Duration = POLL_INTERVAL;
    type Error = EclssError<ScdError<I::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        // self.sensor
        //     .soft_reset()
        //     .await
        //     .context("error sending SCD30 soft reset")?;

        let (major, minor) = self
            .sensor
            .read_firmware_version()
            .await
            .context("error reading SCD30 firmware version")?;
        info!("Connected to SCD30 sensor, firmware v{major}.{minor}");
        self.sensor
            .stop_continuous_measurement()
            .await
            .context("error stopping SCD30 continuous measurement")?;
        self.sensor
            .set_measurement_interval(Self::POLL_INTERVAL.as_secs() as u16)
            .await
            .context("error setting SCD30 measurement interval")?;

        self.sensor
            // TODO(calculate ambient pressure hPa here
            .start_continuous_measurement(1001)
            .await
            .context("error starting SCD30 continuous measurement")?;

        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        while !self
            .sensor
            .data_ready()
            .await
            .context("error seeing if SCD30 data is ready")?
        {
            self.delay.delay_ms(1).await;
        }
        let scd30::Measurement {
            co2,
            temperature,
            humidity,
        } = self
            .sensor
            .measurement()
            .await
            .context("error reading SCD30 measurement")?;
        self.state.record_measurement(co2, temperature, humidity);
        Ok(())
    }
}
