use super::*;
use clap::Parser;
use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::text::{Alignment, LineHeight, Text, TextStyleBuilder};

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

#[derive(Copy, Clone)]
struct ValuePositions {
    time: Point,
    temp: Point,
    humidity: Point,
    tvoc: Point,
    co2: Point,
}

const OFFSET: i32 = 2;

fn render_embedded_graphics<D>(
    target: &mut D,
    char_style: MonoTextStyle<'_, D::Color>,
    metrics: &eclss_api::Metrics,
) -> anyhow::Result<()>
where
    D: embedded_graphics::draw_target::DrawTarget,
    D::Error: core::fmt::Debug,
{
    let positions = render_labels(
        target,
        char_style,
        metrics.location.as_deref().unwrap_or("<unknown>"),
    )?;

    render_values(target, char_style, positions, metrics)?;

    Ok(())
}

fn render_labels<D>(
    target: &mut D,
    char_style: MonoTextStyle<'_, D::Color>,
    location: impl std::fmt::Display,
) -> anyhow::Result<ValuePositions>
where
    D: embedded_graphics::draw_target::DrawTarget,
    D::Error: core::fmt::Debug,
{
    const TIME: &str = "TIME:     ";
    const TEMP: &str = "TEMP:     ";
    const HUMI: &str = "HUMIDITY: ";
    const CO_2: &str = "CO2:      ";
    const TVOC: &str = "TVOC:     ";

    // const WIDTH: usize = {
    //     let labels = [TIME, TEMP, HUMIDITY, TVOC, CO2];
    //     let mut max = 0;
    //     let mut i = 0;
    //     while i < labels.len() {
    //         let len = labels[i].len();
    //         if len > max {
    //             max = len;
    //         }
    //         i += 1;
    //     }
    //     max
    // };

    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(110))
        .build();
    let center = target.bounding_box().center();

    let pt = Text::with_text_style(
        &format!("ECLSS READOUT - {location}\n"),
        Point::new(center.x, OFFSET),
        char_style,
        text_style,
    )
    .draw(target)
    .map_err(|e| anyhow::anyhow!("error drawing title: {e:?}"))?;

    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(110))
        .build();
    let line_height_px = text_style
        .line_height
        .to_absolute(char_style.font.character_size.height) as i32;

    let mut draw_label = |label: &str, pt: Point| {
        let label = Text::with_text_style(label, pt, char_style, text_style)
            .draw(target)
            .map_err(|e| anyhow::anyhow!("error drawing label {label:?}: {e:?}"))?;
        let pt = Point::new(OFFSET, label.y + line_height_px);
        Ok::<_, anyhow::Error>((label, pt))
    };

    let pt = Point::new(OFFSET, pt.y);
    let (time, pt) = draw_label(TIME, pt)?;
    let (temp, pt) = draw_label(TEMP, pt)?;
    let (humidity, pt) = draw_label(HUMI, pt)?;
    let (co2, pt) = draw_label(CO_2, pt)?;
    let (tvoc, _) = draw_label(TVOC, pt)?;
    Ok(ValuePositions {
        time,
        temp,
        humidity,
        tvoc,
        co2,
    })
}

fn render_values<D>(
    target: &mut D,
    char_style: MonoTextStyle<'_, D::Color>,
    positions: ValuePositions,
    metrics: &eclss_api::Metrics,
) -> anyhow::Result<()>
where
    D: embedded_graphics::draw_target::DrawTarget,
    D::Error: core::fmt::Debug,
{
    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(embedded_graphics::text::Baseline::Top)
        .line_height(LineHeight::Percent(110))
        .build();

    Text::with_text_style(
        &format!("{}", chrono::Local::now().format("%I:%M %p")),
        positions.time,
        char_style,
        text_style,
    )
    .draw(target)
    .map_err(|e| anyhow::anyhow!("error drawing time: {e:?}"))?;

    let mut draw_value = |value: Option<String>, pt: Point| {
        let s = value.as_deref().unwrap_or("???");
        Text::with_text_style(s, pt, char_style, text_style)
            .draw(target)
            .map_err(|e| anyhow::anyhow!("error drawing value {value:?}: {e:?}"))
    };

    draw_value(
        mean(&metrics.temp_c).map(|temp_c| {
            let temp_f = temp_c_to_f(temp_c);
            format!("{temp_c:2.2} °C / {temp_f:3.2} °F")
        }),
        positions.temp,
    )?;

    draw_value(
        mean(&metrics.rel_humidity_percent).map(|h| format!("{h:2.2}%")),
        positions.humidity,
    )?;

    draw_value(
        mean(&metrics.co2_ppm).map(|co2| format!("{co2:.2} ppm")),
        positions.co2,
    )?;

    draw_value(
        mean(&metrics.tvoc_ppb).map(|tvoc| format!("{tvoc:.2} ppb")),
        positions.tvoc,
    )?;
    Ok(())
}
