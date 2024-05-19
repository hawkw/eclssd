use anyhow::Context;
use clap::Parser;
use embedded_hal::i2c::{self, I2c as BlockingI2c};
use embedded_hal_async::i2c::I2c;
use linux_embedded_hal::I2cdev;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "/dev/i2c-1")]
    i2cdev: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    let dev = I2cdev::new(&args.i2cdev)
        .with_context(|| format!("failed to open I2C device {}", args.i2cdev.display()))?;
    let eclss: &'static eclss::Eclss<_, 16> =
        Box::leak::<'static>(Box::new(eclss::Eclss::<_, 16>::new(AsyncI2c(dev))));
    tokio::spawn(async move {
        tracing::info!("starting PMSA003i...");
        eclss
            .run_sensor::<eclss::sensor::pmsa003i::Pmsa003i<AsyncI2c<I2cdev>>>(
                std::time::Duration::from_millis(500),
                &mut linux_embedded_hal::Delay,
            )
            .await
            .unwrap()
    })
    .await?;

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
