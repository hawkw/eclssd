use super::{SensorMetrics, SharedBus};
use crate::{metrics, Eclss};
use core::future::Future;
use core::time::Duration;
use embedded_hal_async::delay::DelayNs;
mod status;

#[cfg(feature = "pmsa003i")]
pub mod pmsa003i;

pub use self::status::{Status, StatusCell};

use tinymetrics::registry::RegistryMap;

pub trait Sensor<I>: Sized {
    type Error: core::fmt::Display;
    // type InitFuture: Future<Output = Result<Self, Self::Error>>;
    // type PollFuture: Future<Output = Result<(), Self::Error>>;

    const NAME: &'static str;
    const POLL_INTERVAL: Duration;

    async fn init(
        i2c: &'static SharedBus<I>,
        metrics: &'static SensorMetrics,
    ) -> Result<Self, Self::Error>;
    async fn poll(&mut self) -> Result<(), Self::Error>;
}

impl<I, const SENSORS: usize> Eclss<I, { SENSORS }> {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = tracing::Level::INFO,
            skip(self, retry_backoff, delay),
            fields(sensor = %S::NAME)
        )
    )]
    pub async fn run_sensor<S>(
        &'static self,
        retry_backoff: Duration,
        delay: &mut impl DelayNs,
    ) -> Result<(), &'static str>
    where
        S: Sensor<I>,
    {
        let Manager {
            status,
            backoff,
            poll_interval,
            ..
        } = self
            .sensors
            .get_or_register(
                S::NAME,
                Manager {
                    status: StatusCell::new(),
                    poll_interval: S::POLL_INTERVAL,
                    backoff: crate::retry::ExpBackoff::new(retry_backoff),
                },
            )
            .ok_or("insufficient space in sensor registry")?;
        let errors = self
            .metrics
            .sensor_errors
            .register(metrics::SensorLabel(S::NAME))
            .ok_or("insufficient space in sensor errors metric")?;

        let mut sensor = loop {
            match S::init(&self.i2c, &self.metrics).await {
                Ok(sensor) => {
                    backoff.reset();
                    info!("initialized {}", S::NAME);
                    break sensor;
                }
                Err(error) => {
                    warn!(%error, "failed to initialize {}, retrying...", S::NAME);
                    errors.fetch_add(1);
                    backoff.wait(delay).await;
                }
            }
        };
        loop {
            while let Err(error) = sensor.poll().await {
                warn!(%error, "failed to poll {}, retrying...", S::NAME);
                status.set_status(Status::Down);
                errors.fetch_add(1);
                backoff.wait(delay).await;
            }
            status.set_status(Status::Up);
            delay.delay_ms(poll_interval.as_millis() as u32).await;
        }
    }
}

pub(crate) type Registry<const N: usize> = RegistryMap<&'static str, Manager, { N }>;

pub(crate) struct Manager {
    status: StatusCell,
    poll_interval: Duration,
    backoff: crate::retry::ExpBackoff,
}
