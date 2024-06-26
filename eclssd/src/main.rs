use anyhow::Context;
use clap::Parser;
use eclss::sensor::{self, SensorName};
use eclss::Eclss;
use eclss_app::TraceArgs;
use embedded_hal::i2c::{self, I2c as BlockingI2c};
use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use linux_embedded_hal::I2cdev;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "mdns")]
mod mdns;
mod storage;

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

    #[clap(long = "location", env = "ECLSS_LOCATION")]
    location: Option<Arc<str>>,

    /// enable mDNS advertisement
    #[clap(
        long = "mdns",
        action = clap::ArgAction::Set,
        value_parser = clap::value_parser!(bool),
        default_value_t = cfg!(feature = "mdns")
    )]
    mdns: bool,

    /// List of sensors to enable.
    ///
    /// If no sensors are provided here, the ECLSS daemon will attempt to
    /// connect to all sensors that are enabled at compile time.
    #[clap(long = "sensor", short, default_values_t = DEFAULT_SENSORS)]
    sensors: Vec<SensorName>,

    /// Sensor configuration
    #[clap(flatten)]
    sensor_config: eclss::Config,

    /// Storage configuration.
    #[clap(flatten)]
    storage: storage::StorageArgs,

    #[clap(flatten)]
    trace: TraceArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.trace.trace_init();
    use eclss::metrics::*;

    tracing::info!(
        location = ?args.location,
        version = %env!("CARGO_PKG_VERSION"),
        listen_addr = ?args.listen_addr,
        mdns = args.mdns,
        storage = ?args.storage,
        config = ?args.sensor_config,
        "starting environmental controls and life support systems..."
    );
    tracing::debug!(
        TEMP_METRICS,
        CO2_METRICS,
        ECO2_METRICS,
        HUMIDITY_METRICS,
        PRESSURE_METRICS,
        VOC_RESISTANCE_METRICS,
        TVOC_METRICS,
        PM_CONC_METRICS,
        PM_COUNT_METRICS,
        SENSORS
    );

    let dev = I2cdev::new(&args.i2cdev)
        .with_context(|| format!("failed to open I2C device {}", args.i2cdev.display()))?;
    tracing::info!(path = %args.i2cdev.display(), "opened I²C device");

    let eclss: &'static eclss::Eclss<_, 16> =
        Box::leak::<'static>(Box::new(eclss::Eclss::<_, 16>::new(AsyncI2c(dev))));

    let listener = tokio::net::TcpListener::bind(args.listen_addr).await?;
    tracing::info!(listen_addr = ?args.listen_addr, "listening...");
    let server = tokio::spawn({
        let location = args.location.clone();
        async move {
            eclss_axum::axum::serve(listener, eclss_axum::app(eclss, location))
                .await
                .unwrap();
        }
    });

    if args.mdns {
        #[cfg(feature = "mdns")]
        mdns::advertise(&args)?;
        #[cfg(not(feature = "mdns"))]
        anyhow::bail!("mDNS advertisement requires the `mdns` feature to be enabled");
    }
    let state_dir = args.storage.ensure_state_dir().await?;

    let mut sensor_tasks = tokio::task::JoinSet::new();
    tracing::info!("Enabling the following sensors: {:?}", args.sensors);
    for sensor in args.sensors {
        sensor_tasks.spawn(run_sensor(eclss, &state_dir, &args.sensor_config, sensor));
    }

    while let Some(join) = sensor_tasks.join_next().await {
        join.context("a sensor task panicked")??;
    }

    server.await.context("HTTP server panicked")?;

    Ok(())
}

const DEFAULT_SENSORS: &[SensorName] = &[
    #[cfg(feature = "pmsa003i")]
    SensorName::Pmsa003i,
    #[cfg(feature = "scd41")]
    SensorName::Scd41,
    #[cfg(feature = "scd30")]
    SensorName::Scd30,
    #[cfg(feature = "sen55")]
    SensorName::Sen55,
    #[cfg(feature = "sgp30")]
    SensorName::Sgp30,
    #[cfg(feature = "sht41")]
    SensorName::Sht41,
    #[cfg(feature = "ens160")]
    SensorName::Ens160,
    #[cfg(feature = "bme680")]
    SensorName::Bme680,
];

fn run_sensor(
    eclss: &'static Eclss<AsyncI2c<I2cdev>, 16>,
    state_dir: &storage::StateDir,
    sensor_config: &eclss::Config,
    name: SensorName,
) -> impl Future<Output = anyhow::Result<()>> + Send + 'static {
    let config = sensor_config.clone();
    let state_dir = state_dir.clone();
    async move {
        match name {
            #[cfg(feature = "pmsa003i")]
            SensorName::Pmsa003i => {
                let sensor = sensor::Pmsa003i::new(eclss, &config);
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "scd41")]
            SensorName::Scd41 => {
                let sensor = sensor::Scd41::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "scd40")]
            SensorName::Scd40 => {
                let sensor = sensor::Scd40::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "scd30")]
            SensorName::Scd30 => {
                let sensor = sensor::Scd30::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "sen55")]
            SensorName::Sen55 => {
                let sensor = sensor::Sen55::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "sgp30")]
            SensorName::Sgp30 => {
                let state = state_dir
                    .sensor_state(name)
                    .await
                    .with_context(|| format!("failed to open state file for {name}"))?;
                let sensor =
                    sensor::Sgp30::new(eclss, &config, GoodDelay::default()).with_storage(state);
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "sht41")]
            SensorName::Sht41 => {
                let sensor = sensor::Sht41::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "ens160")]
            SensorName::Ens160 => {
                let sensor = sensor::Ens160::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            #[cfg(feature = "bme680")]
            SensorName::Bme680 => {
                let sensor = sensor::Bme680::new(eclss, &config, GoodDelay::default());
                eclss
                    .run_sensor(sensor, config, GoodDelay::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("error running {name}: {e}"))
            }
            sensor => anyhow::bail!("sensor {sensor} not enabled at compile time!"),
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

/// The `embedded_hal_async` implementation for `linux_embedded_hal`'s delay
/// type is not very precise. Use blocking delays for short sleeps in timing
/// critical sensor wire protocols, and use the async delay for longer sleeps
/// like in the poll loop.ca
#[derive(Default, Copy, Clone)]
struct GoodDelay(spin_sleep::SpinSleeper);
impl GoodDelay {
    const ONE_MS_NANOS: u32 = Duration::from_millis(1).as_nanos() as u32;
}

impl DelayNs for GoodDelay {
    async fn delay_ns(&mut self, ns: u32) {
        if ns >= Self::ONE_MS_NANOS {
            tokio::time::sleep(Duration::from_nanos(ns as u64)).await;
        } else {
            self.0.sleep_ns(ns as u64);
        }
    }
}
