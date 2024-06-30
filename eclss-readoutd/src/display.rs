use super::*;
use clap::Parser;
use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use std::path::PathBuf;

#[cfg(feature = "ssd1680")]
mod ssd1680_display;

#[derive(Debug, Parser)]
pub(crate) struct WindowArgs {
    /// Refresh interval
    #[clap(long, short, default_value = "2s")]
    refresh: humantime::Duration,

    /// X dimension of the display in pixels.
    #[clap(long, short, default_value_t = 250)]
    x: u32,

    /// Y dimension of the display in pixels.
    #[clap(long, short, default_value_t = 122)]
    y: u32,
}

#[derive(Debug, Parser)]
pub(crate) struct Ssd1680Args {
    // /// SPI device path.
    // #[clap(long, short, default_value = "/dev/spidev0.0")]
    // spidev: PathBuf,

    // /// X dimension of the display in pixels.
    // #[clap(long, short)]
    // x: u32,

    // /// Y dimension of the display in pixels.
    // #[clap(long, short)]
    // y: u32,
    /// Chip select (CS) pin.
    #[clap(long, value_enum, default_value_t = CsPin::Ce0)]
    cs_pin: CsPin,

    /// RST select pin.
    #[clap(long, default_value_t = 27)]
    rst_pin: u8,

    /// DC pin.
    #[clap(long, default_value_t = 25)]
    dc_pin: u8,

    /// BUSY pin
    #[clap(long, default_value_t = 17)]
    busy_pin: u8,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum CsPin {
    Ce0,
    Ce1,
    Ce2,
}

impl WindowArgs {
    #[cfg(not(feature = "window"))]
    pub(crate) async fn run(self, _: Client) -> anyhow::Result<()> {
        anyhow::bail!("windowed display mode requires the 'window' feature flag")
    }

    #[cfg(feature = "window")]
    pub(crate) async fn run(self, mut client: Client) -> anyhow::Result<()> {
        use embedded_graphics_simulator::{
            BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
        };
        let mut display: SimulatorDisplay<BinaryColor> =
            SimulatorDisplay::new(Size::new(self.x, self.y));

        let output_settings = OutputSettingsBuilder::new()
            .theme(BinaryColorTheme::OledBlue)
            .build();
        let mut window = Window::new("eclss-displayd", &output_settings);
        let style = MonoTextStyle::new(&profont::PROFONT_12_POINT, BinaryColor::On);
        let mut interval = tokio::time::interval(self.refresh.into());
        loop {
            let metrics = client.fetch().await?;
            tracing::trace!(?metrics);
            display.clear(BinaryColor::Off)?;
            render_embedded_graphics(&mut display, style, &metrics)?;
            window.update(&display);
            interval.tick().await;
        }
    }
}

impl Ssd1680Args {
    #[cfg(not(feature = "ssd1680"))]
    pub(crate) async fn run(self, _: Client) -> anyhow::Result<()> {
        anyhow::bail!("SSD1680 display mode requires the 'ssd1680' feature flag")
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
    const OFFSET: i32 = 2;
    const TEMP: &str = "TEMP:";
    const HUMIDITY: &str = "HUMIDITY:";
    const TVOC: &str = "TVOC:";
    const CO2: &str = "CO2:";
    const STATION: &str = "STATION:";

    const WIDTH: usize = {
        let labels = [STATION, TEMP, HUMIDITY, TVOC, CO2];
        let mut max = 0;
        let mut i = 0;
        while i < labels.len() {
            let len = labels[i].len();
            if len > max {
                max = len;
            }
            i += 1;
        }
        max
    };

    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(110))
        .build();
    let center = target.bounding_box().center();

    let pt = Text::with_text_style(
        "ECLSS READOUT\n",
        Point::new(center.x, OFFSET),
        char_style,
        text_style,
    )
    .draw(target)
    .map_err(|e| anyhow::anyhow!("error drawing title: {e:?}"))?;

    let mut pt = Point::new(OFFSET, pt.y);
    if let Some(location) = metrics.location.as_ref() {
        pt = Text::with_text_style(
            &format!("{STATION:<WIDTH$}: {location}\n"),
            pt,
            char_style,
            text_style,
        )
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing location: {e:?}"))?;
    }
    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(110))
        .build();
    let temp = mean(&metrics.temp_c)
        .map(|temp_c| {
            let temp_f = temp_c_to_f(temp_c);
            format!("{TEMP:<WIDTH$} {temp_c:2.2} °C / {temp_f:3.2} °F\n")
        })
        .unwrap_or_else(|| format!("{TEMP:<WIDTH$} ??? °C / ??? °F\n"));

    let pt = Text::with_text_style(&temp, pt, char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing temperature: {e:?}"))?;

    let rel_humidity = mean(&metrics.rel_humidity_percent)
        .map(|h| format!("{HUMIDITY:<WIDTH$} {h:.2}%\n"))
        .unwrap_or_else(|| format!("{HUMIDITY:<WIDTH$}: ???%\n"));

    let pt = Text::with_text_style(&rel_humidity, pt, char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing humidity: {e:?}"))?;

    let co2_ppm = mean(&metrics.co2_ppm)
        .map(|c| format!("{CO2:<WIDTH$} {c:.2} ppm\n"))
        .unwrap_or_else(|| format!("{CO2:<WIDTH$} ??? ppm\n"));

    let pt = Text::with_text_style(&co2_ppm, pt, char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing CO2: {e:?}"))?;

    let tvoc_ppb = mean(&metrics.tvoc_ppb)
        .map(|c| format!("{TVOC:<WIDTH$} {c:.2} ppb\n"))
        .unwrap_or_else(|| format!("{TVOC:<WIDTH$} ??? ppb\n"));

    Text::with_text_style(&tvoc_ppb, pt, char_style, text_style)
        .draw(target)
        .map_err(|e| anyhow::anyhow!("error drawing tVOC: {e:?}"))?;
    Ok(())
}
