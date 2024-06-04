use crate::{
    metrics::{DiameterLabel, Gauge},
    sensor::{Sensor, SensorError},
    SharedBus,
};

use embedded_hal::i2c;
use embedded_hal_async::i2c::I2c;

pub struct Pmsa003i<I: 'static> {
    sensor: pmsa003i::Pmsa003i<&'static SharedBus<I>>,
    pm2_5: &'static Gauge,
    pm1_0: &'static Gauge,
    pm10_0: &'static Gauge,
    particles_0_3um: &'static Gauge,
    particles_0_5um: &'static Gauge,
    particles_1_0um: &'static Gauge,
    particles_2_5um: &'static Gauge,
    particles_5_0um: &'static Gauge,
    particles_10_0um: &'static Gauge,
}

impl<I> Pmsa003i<I> {
    pub fn new<const SENSORS: usize>(eclss: &'static crate::Eclss<I, { SENSORS }>) -> Self {
        let metrics = &eclss.metrics;
        Self {
            sensor: pmsa003i::Pmsa003i::new(&eclss.i2c),
            pm2_5: metrics.pm_conc.register(DiameterLabel("2.5")).unwrap(),
            pm1_0: metrics.pm_conc.register(DiameterLabel("1.0")).unwrap(),
            pm10_0: metrics.pm_conc.register(DiameterLabel("10.0")).unwrap(),
            particles_0_3um: metrics.pm_count.register(DiameterLabel("0.3")).unwrap(),
            particles_0_5um: metrics.pm_count.register(DiameterLabel("0.5")).unwrap(),
            particles_1_0um: metrics.pm_count.register(DiameterLabel("1.0")).unwrap(),
            particles_2_5um: metrics.pm_count.register(DiameterLabel("2.5")).unwrap(),
            particles_5_0um: metrics.pm_count.register(DiameterLabel("5.0")).unwrap(),
            particles_10_0um: metrics.pm_count.register(DiameterLabel("10.0")).unwrap(),
        }
    }
}

const NAME: &str = "PMSA003I";

impl<I> Sensor for Pmsa003i<I>
where
    I: I2c + 'static,
    I::Error: core::fmt::Display,
{
    const NAME: &'static str = NAME;
    const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_secs(2);
    type Error = pmsa003i::SensorError<I::Error>;
    // type InitFuture = impl Future<Output = Result<Self, Self::Error>>;
    // type PollFuture = impl Future<Output = Result<Self, Self::Error>>;

    async fn init(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Self::Error> {
        let pmsa003i::Reading {
            concentrations,
            counts,
            sensor_version: _,
        } = self.sensor.read_async().await?;

        debug!("particulate concentrations:\n{concentrations:>#3}");
        debug!("particulates {counts:>#3}");

        macro_rules! set_metrics {
            ($src:ident => $($name:ident),+) => {
                $(
                    self.$name.set_value($src.$name.into());
                )+
            }
        }
        set_metrics!(concentrations => pm1_0, pm2_5, pm10_0);
        set_metrics!(counts =>
            particles_0_3um,
            particles_0_5um,
            particles_1_0um,
            particles_2_5um,
            particles_5_0um,
            particles_10_0um
        );
        Ok(())
    }
}

impl<E: i2c::Error> SensorError for pmsa003i::SensorError<E> {
    fn i2c_error(&self) -> Option<i2c::ErrorKind> {
        match self {
            pmsa003i::SensorError::I2c(e) => Some(e.kind()),
            pmsa003i::SensorError::Reading(_) => None,
        }
    }
}
