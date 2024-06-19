use anyhow::Context;
use clap::Parser;
use eclss_app::TraceArgs;
use embedded_graphics::prelude::*;

mod display;
#[cfg(feature = "terminal")]
mod terminal;

#[derive(Debug, Parser)]
struct Args {
    /// The hostname of the `eclssd` instance to display data from.
    host: reqwest::Url,

    #[clap(subcommand)]
    display: DisplayCommand,

    #[clap(flatten)]
    trace: TraceArgs,
}

#[derive(clap::Subcommand, Debug)]
enum DisplayCommand {
    Terminal(TerminalArgs),
    /// Display ECLSS data in a window.
    Window(display::WindowArgs),
    /// Display ECLSS data on an SSD1680 e-ink display.
    ///
    /// Default arguments are for the Adafruit 2.13" e-ink display.
    Ssd1680(display::Ssd1680Args),
}

#[derive(Debug, Parser)]
struct TerminalArgs {
    /// Refresh interval
    #[clap(long, short, default_value = "2s")]
    refresh: humantime::Duration,
}

impl Args {
    fn client(&self) -> anyhow::Result<Client> {
        let client = reqwest::Client::new();
        let metrics_url = self.host.join("/metrics.json")?;
        Ok(Client {
            client,
            metrics_url,
        })
    }
}

struct Client {
    client: reqwest::Client,
    pub(crate) metrics_url: reqwest::Url,
}

impl Client {
    async fn fetch(&mut self) -> anyhow::Result<eclss_api::Metrics> {
        tracing::debug!("fetching sensor data...");
        let rsp = self
            .client
            .get(self.metrics_url.clone())
            .send()
            .await
            .with_context(|| format!("sending request to {} failed", self.metrics_url))?;
        tracing::debug!("received response: {:?}", rsp.status());
        rsp.json().await.context("reading request body failed")
    }
}

#[cfg(not(feature = "terminal"))]
impl TerminalArgs {
    async fn run(self, _: Client) -> anyhow::Result<()> {
        anyhow::bail!("terminal display mode requires the 'terminal' feature flag")
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.trace.trace_init();
    let client = args.client()?;
    match args.display {
        DisplayCommand::Terminal(cmd) => cmd.run(client).await,
        DisplayCommand::Window(cmd) => cmd.run(client).await,
        DisplayCommand::Ssd1680(cmd) => cmd.run(client).await,
    }
}

fn temp_c_to_f(temp_c: f64) -> f64 {
    (temp_c * 1.8) + 32.0
}

fn mean(measurements: impl AsRef<[eclss_api::Measurement]>) -> Option<f64> {
    let measurements = measurements.as_ref();
    let len = measurements.len();
    if len == 0 {
        return None;
    }
    let sum: f64 = measurements.iter().map(|m| m.value).sum();
    Some(sum / len as f64)
}
