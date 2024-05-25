use anyhow::Context;
use clap::Parser;
use eclss::sensor::{self};
use embedded_hal::i2c::{self, I2c as BlockingI2c};
use embedded_hal_async::i2c::I2c;
use linux_embedded_hal::I2cdev;
use std::path::PathBuf;
use tracing_subscriber::prelude::*;

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "/dev/i2c-1")]
    i2cdev: PathBuf,

    #[clap(long, default_value = "500ms")]
    initial_backoff: humantime::Duration,

    #[clap(long, default_value = "60s")]
    max_backoff: humantime::Duration,

    #[clap(env = "ECLSS_LOG", long = "log", default_value = "info")]
    trace_filter: tracing_subscriber::filter::Targets,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(args.trace_filter)
        .init();

    let dev = I2cdev::new(&args.i2cdev)
        .with_context(|| format!("failed to open I2C device {}", args.i2cdev.display()))?;
    let eclss: &'static eclss::Eclss<_, 16> =
        Box::leak::<'static>(Box::new(eclss::Eclss::<_, 16>::new(AsyncI2c(dev))));
    let backoff = eclss::retry::ExpBackoff::new(args.initial_backoff.into())
        .with_max(args.max_backoff.into());
    let mut sensors = tokio::task::JoinSet::new();
    sensors.spawn({
        let sensor = sensor::Pmsa003i::new(eclss);
        let backoff = backoff.clone();
        async move {
            tracing::info!("starting PMSA003I...");
            eclss
                .run_sensor(sensor, backoff, linux_embedded_hal::Delay)
                .await
                .unwrap()
        }
    });
    sensors.spawn({
        let sensor = sensor::Scd4x::new(eclss, linux_embedded_hal::Delay);

        let backoff = backoff.clone();
        async move {
            tracing::info!("starting SCD4x...");
            eclss
                .run_sensor(sensor, backoff.clone(), linux_embedded_hal::Delay)
                .await
                .unwrap()
        }
    });

    while let Some(join) = sensors.join_next().await {
        join.unwrap();
    }

    Ok(())
}

struct AsyncI2c<I>(I);
impl<I, A> I2c<A> for AsyncI2c<I>
where
    I: BlockingI2c<A>,
    A: i2c::AddressMode,
{
    async fn transaction(
        &mut self,
        address: A,
        operations: &mut [i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.0.transaction(address, operations)
    }
}

impl<I> i2c::ErrorType for AsyncI2c<I>
where
    I: i2c::ErrorType,
{
    type Error = I::Error;
}
