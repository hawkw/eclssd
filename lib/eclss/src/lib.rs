// #![feature(impl_trait_in_assoc_type)]
use embedded_hal::i2c;
use embedded_hal_async::i2c::I2c;
use maitake_sync::Mutex;
#[macro_use]
mod trace;

pub use self::metrics::SensorMetrics;
pub mod metrics;
pub mod retry;
pub mod sensor;

pub struct Eclss<I, const SENSORS: usize> {
    pub(crate) metrics: SensorMetrics,
    pub(crate) i2c: SharedBus<I>,
    pub(crate) sensors: sensor::Registry<SENSORS>,
}

impl<I, const SENSORS: usize> Eclss<I, { SENSORS }> {
    pub const fn new(i2c: I) -> Self {
        Self {
            metrics: SensorMetrics::new(),
            i2c: SharedBus::new(i2c),
            sensors: sensor::Registry::new(),
        }
    }

    pub fn sensors(&self) -> &sensor::Registry<SENSORS> {
        &self.sensors
    }

    pub fn metrics(&self) -> &SensorMetrics {
        &self.metrics
    }
}

#[derive(Debug)]
pub struct SharedBus<I>(Mutex<I>);

impl<I> SharedBus<I> {
    pub const fn new(i2c: I) -> Self {
        SharedBus(Mutex::new(i2c))
    }
}

impl<I, A> I2c<A> for &'_ SharedBus<I>
where
    I: I2c<A>,
    A: i2c::AddressMode,
{
    async fn transaction(
        &mut self,
        address: A,
        operations: &mut [i2c::Operation<'_>],
    ) -> Result<(), <Self as i2c::ErrorType>::Error> {
        self.0.lock().await.transaction(address, operations).await
    }
}

impl<I> i2c::ErrorType for &'_ SharedBus<I>
where
    I: i2c::ErrorType,
{
    type Error = I::Error;
}
