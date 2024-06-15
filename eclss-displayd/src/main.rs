use anyhow::Context;
use clap::Parser;
use eclss_app::{TraceArgs, TraceFormat};
use embedded_graphics::geometry::Point;
use embedded_graphics::mock_display::MockDisplay;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use embedded_graphics::Drawable;

#[derive(Debug, Parser)]
struct Args {
    #[clap(flatten)]
    trace: TraceArgs,

    /// The hostname of the `eclssd` instance to display data from.
    host: reqwest::Url,

    /// Refresh interval
    #[clap(long, short, default_value = "2s")]
    refresh: humantime::Duration,

    #[clap(subcommand)]
    display: DisplayCommand,
}

#[derive(clap::Subcommand, Debug)]
enum DisplayCommand {
    Terminal,
    /// Display ECLSS data in a window.
    Window,
}

impl Args {
    fn client(&self) -> anyhow::Result<Client> {
        let client = reqwest::Client::new();
        let interval = tokio::time::interval(self.refresh.into());
        let metrics_url = self.host.join("/metrics.json")?;
        Ok(Client {
            client,
            interval,
            metrics_url,
        })
    }
}

struct Client {
    client: reqwest::Client,
    interval: tokio::time::Interval,
    metrics_url: reqwest::Url,
}

impl Client {
    async fn fetch(&mut self) -> anyhow::Result<eclss_api::Metrics> {
        self.interval.tick().await;
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.trace.trace_init();
    let client = args.client()?;
    match args.display {
        DisplayCommand::Terminal => display_terminal(client).await,
        DisplayCommand::Window => display_window(client).await,
    }
}

async fn display_terminal(mut client: Client) -> anyhow::Result<()> {
    loop {
        let fetch = client.fetch().await?;
        println!("{:#?}\n", fetch);
    }
}

#[cfg(not(feature = "embedded-graphics-simulator"))]
async fn display_window(client: Client) -> anyhow::Result<()> {
    anyhow::bail!("windowed display mode requires the 'embedded-graphics-simulator' feature")
}

#[cfg(feature = "embedded-graphics-simulator")]
async fn display_window(mut client: Client) -> anyhow::Result<()> {
    use embedded_graphics::{
        mono_font::{ascii::FONT_6X10, MonoTextStyle},
        pixelcolor::BinaryColor,
    };
    use embedded_graphics_simulator::{
        BinaryColorTheme, OutputSettings, OutputSettingsBuilder, SimulatorDisplay, Window,
    };
    let mut display: SimulatorDisplay<BinaryColor> = SimulatorDisplay::new(Size::new(128, 64));

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::OledBlue)
        .build();
    let mut window = Window::new("eclss-displayd", &output_settings);
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    loop {
        let metrics = client.fetch().await?;
        render_embedded_graphics(&mut display, style, &metrics)?;
        window.update(&display);
    }
}

fn render_embedded_graphics<D>(
    target: &mut D,
    char_style: MonoTextStyle<'_, D::Color>,
    metrics: &eclss_api::Metrics,
) -> anyhow::Result<()>
where
    D: embedded_graphics::draw_target::DrawTarget,
    D::Error: core::fmt::Debug,
{
    use embedded_graphics::text::{Alignment, LineHeight, Text, TextStyleBuilder};
    const OFFSET: i32 = 5;
    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(150))
        .build();
    let temp = mean(&metrics.temp_c)
        .map(|t| format!("Temperature: {:.2} C", t))
        .unwrap_or_else(|| String::from("Temperature: ???"));
    let rel_humidity = mean(&metrics.rel_humidity_percent)
        .map(|h| format!("Rel. Humidity: {h:.2}%"))
        .unwrap_or_else(|| String::from("Rel. Humidity: ???"));
    let pt = Text::with_text_style(&temp, Point::new(0, 0), char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing temperature: {e:?}"))?;
    let pt2 = Point {
        x: OFFSET,
        y: dbg!(pt).y + OFFSET,
    };
    Text::with_text_style(&rel_humidity, pt2, char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing humidity: {e:?}"))?;
    Ok(())
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
