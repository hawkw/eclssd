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
}

#[derive(Debug, Parser)]
#[command(next_help_heading = "Sensor Retries")]
struct RetryArgs {
    /// initial value for sensor retry backoffs
    #[clap(long, default_value = "500ms")]
    initial_backoff: humantime::Duration,
    /// maximum backoff duration for sensor retries
    #[clap(long, default_value = "60s")]
    max_backoff: humantime::Duration,
}

#[derive(Debug, Parser)]
#[command(next_help_heading = "Tracing")]
struct TraceArgs {
    /// Tracing-subscriber filter configuration
    #[clap(env = "ECLSS_LOG", long = "trace", default_value = "info,eclss=debug")]
    filter: tracing_subscriber::filter::Targets,

    /// Trace output format
    #[clap(
        env = "ECLSS_LOG_FORMAT",
        long = "trace-format",
        default_value = "text"
    )]
    format: TraceFormat,

    /// If true, disable timestamps in trace events.
    ///
    /// This is intended for use in environments where timestamps are added by
    /// an external logging system, such as when running as a systemd service or
    /// in a container runtime.
    #[clap(long, env = "ECLSS_LOG_NO_TIMESTAMPS")]
    no_timestamps: bool,

    /// If true, disable ANSI formatting escape codes in tracing output.
    #[clap(long, env = "NO_COLOR")]
    no_color: bool,
}

#[derive(clap::ValueEnum, Debug, Clone)]
#[clap(rename_all = "lower")]
enum TraceFormat {
    /// Human-readable text logging format.
    Text,
    /// JSON logging format.
    Json,
    /// Log to journald, rather than to stdout.
    Journald,
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

impl TraceArgs {
    fn trace_init(&self) {
        let registry = tracing_subscriber::registry().with(self.filter.clone());
        match self.format {
            #[cfg(target_os = "linux")]
            TraceFormat::Journald => match tracing_journald::Layer::new() {
                Ok(journald) => {
                    registry.with(journald).init();
                    return;
                }
                Err(err) => {
                    eprintln!("failed to connect to journald, falling back to text format: {err}");
                }
            },
            #[cfg(not(target_os = "linux"))]
            TraceFormat::Journald => {
                eprintln!(
                    "journald format is only supported on Linux, falling back to text format"
                );
            }
            TraceFormat::Json => {
                let fmt = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(false)
                    .with_span_list(true)
                    .flatten_event(true)
                    .with_thread_ids(true);
                if self.no_timestamps {
                    registry.with(fmt.without_time()).init();
                } else {
                    registry.with(fmt).init();
                }
                return;
            }
            TraceFormat::Text => {
                // do nothing, as we also want to fall through to the text
                // format if journald init fails.
            }
        }
        let fmt = tracing_subscriber::fmt::layer()
            .with_thread_ids(true)
            .with_ansi(!self.no_color);
        if self.no_timestamps {
            registry.with(fmt.without_time()).init();
        } else {
            registry.with(fmt).init();
        }
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
