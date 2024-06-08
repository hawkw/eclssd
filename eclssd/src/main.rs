use anyhow::Context;
use clap::Parser;
use eclss::sensor::{self};
use eclss_app::TraceArgs;
use embedded_hal::i2c::{self, I2c as BlockingI2c};
use embedded_hal_async::i2c::I2c;
use linux_embedded_hal::I2cdev;
use std::path::PathBuf;

#[cfg(feature = "mdns")]
mod mdns;

#[derive(Debug, Parser)]
struct Args {
    /// Path to the Linux i2cdev I²C device to use to communicate with sensors.
    #[clap(short, long, env = "ECLSS_I2C_DEV", default_value = "/dev/i2c-1")]
    i2cdev: PathBuf,

    /// Address to bind the HTTP server on.
    #[clap(
        short,
        long,
        env = "ECLSS_LISTEN_ADDR",
        default_value = "127.0.0.1:4200"
    )]
    listen_addr: std::net::SocketAddr,

    #[clap(flatten)]
    retries: RetryArgs,

    #[clap(flatten)]
    trace: TraceArgs,

    #[clap(long = "location", env = "ECLSS_LOCATION")]
    location: Option<String>,

    /// enable mDNS advertisement
    #[clap(long = "mdns", default_value_t = cfg!(feature = "mdns"))]
    mdns: bool,
}

#[derive(Debug, Parser)]
#[command(next_help_heading = "mDNS Advertisement")]
struct RetryArgs {
    /// initial value for sensor retry backoffs
    #[clap(long, default_value = "500ms")]
    initial_backoff: humantime::Duration,
    /// maximum backoff duration for sensor retries
    #[clap(long, default_value = "60s")]
    max_backoff: humantime::Duration,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.trace.trace_init();

    let dev = I2cdev::new(&args.i2cdev)
        .with_context(|| format!("failed to open I2C device {}", args.i2cdev.display()))?;
    tracing::info!(path = "opened I²C device");

    let eclss: &'static eclss::Eclss<_, 16> =
        Box::leak::<'static>(Box::new(eclss::Eclss::<_, 16>::new(AsyncI2c(dev))));
    let backoff = args.retries.backoff();

    let listener = tokio::net::TcpListener::bind(args.listen_addr).await?;
    tracing::info!(listen_addr = ?args.listen_addr, "listening...");
    let server = tokio::spawn(async move {
        eclss_axum::axum::serve(listener, eclss_axum::app(eclss))
            .await
            .unwrap();
    });

    if args.mdns {
        #[cfg(feature = "mdns")]
        mdns::advertise(&args)?;
        #[cfg(not(feature = "mdns"))]
        anyhow::bail!("mDNS advertisement requires the `mdns` feature to be enabled");
    }

    let mut sensors = tokio::task::JoinSet::new();
    #[cfg(feature = "pmsa003i")]
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

    #[cfg(any(feature = "scd41", feature = "scd40"))]
    sensors.spawn({
        let sensor = sensor::Scd4x::new(eclss, AsyncBlockingDelayNs(linux_embedded_hal::Delay));

        let backoff = backoff.clone();
        async move {
            tracing::info!("starting SCD4x...");
            eclss
                .run_sensor(sensor, backoff.clone(), linux_embedded_hal::Delay)
                .await
                .unwrap()
        }
    });

    #[cfg(feature = "sgp30")]
    sensors.spawn({
        let sensor = sensor::Sgp30::new(eclss, AsyncBlockingDelayNs(linux_embedded_hal::Delay));

        let backoff = backoff.clone();
        async move {
            tracing::info!("starting SGP30...");
            eclss
                .run_sensor(sensor, backoff.clone(), linux_embedded_hal::Delay)
                .await
                .unwrap()
        }
    });

    #[cfg(feature = "ens160")]
    sensors.spawn({
        let sensor = sensor::Ens160::new(eclss, linux_embedded_hal::Delay);

        let backoff = backoff.clone();
        async move {
            tracing::info!("starting ENS160...");
            eclss
                .run_sensor(sensor, backoff.clone(), linux_embedded_hal::Delay)
                .await
                .unwrap()
        }
    });

    while let Some(join) = sensors.join_next().await {
        join.unwrap();
    }

    server.await.unwrap();

    Ok(())
}

impl RetryArgs {
    fn backoff(&self) -> eclss::retry::ExpBackoff {
        let &Self {
            initial_backoff,
            max_backoff,
        } = self;
        tracing::info!(
            %initial_backoff,
            %max_backoff,
            "configuring sensor retries...",
        );
        eclss::retry::ExpBackoff::new(initial_backoff.into()).with_max(max_backoff.into())
    }
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

/// The `embedded_hal_async` implementation for `linux_embedded_hal`'s delay
/// type is not very precise. Use blocking delays for short sleeps in timing
/// critical sensor wire protocols, and use the async delay for longer sleeps
/// like in the poll loop.
struct AsyncBlockingDelayNs<D>(D);

impl<D: BlockingDelayNs> DelayNs for AsyncBlockingDelayNs<D> {
    async fn delay_ns(&mut self, ns: u32) {
        self.0.delay_ns(ns);
    }
}
