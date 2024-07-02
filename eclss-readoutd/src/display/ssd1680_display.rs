use super::*;
use rppal::gpio::Gpio;
use rppal::spi;
use ssd1680::{
    driver::Ssd1680,
    graphics::{self, Display},
};

impl Ssd1680Args {
    pub(crate) async fn run(self, mut client: Client) -> anyhow::Result<()> {
        tracing::debug!("Configuring SSD1680 display: {self:#?}");
        let gpio = Gpio::new().context("failed to access GPIO")?;
        let rst = gpio
            .get(self.rst_pin)
            .with_context(|| format!("failed to access RST pin (GPIO {})", self.rst_pin))?
            .into_output();
        let dc = gpio
            .get(self.dc_pin)
            .with_context(|| format!("failed to access DC pin (GPIO {})", self.dc_pin))?
            .into_output();
        let busy = gpio
            .get(self.busy_pin)
            .with_context(|| format!("failed to access BUSY pin (GPIO {})", self.busy_pin))?
            .into_input();
        let spi: spi::SimpleHalSpiDevice<spi::Spi> = {
            let ss = match self.cs_pin {
                CsPin::Ce0 => spi::SlaveSelect::Ss0,
                CsPin::Ce1 => spi::SlaveSelect::Ss1,
                CsPin::Ce2 => spi::SlaveSelect::Ss2,
            };
            let spi = spi::Spi::new(spi::Bus::Spi0, ss, 50_000, spi::Mode::Mode0)
                .context("failed to access SPI device")?;
            spi::SimpleHalSpiDevice::new(spi)
        };
        let mut delay = linux_embedded_hal::Delay;
        let mut ssd1680 = Ssd1680::new(spi, busy, dc, rst, &mut delay)
            .map_err(|err| anyhow::anyhow!("failed to construct SSD1680 driver: {err:?}"))?;
        ssd1680
            .init(&mut delay)
            .map_err(|err| anyhow::anyhow!("failed to initialize SSD1680 driver: {err:?}"))?;
        ssd1680
            .clear_bw_frame()
            .map_err(|err| anyhow::anyhow!("failed to clear SSD1680 B/W frame: {err:?}"))?;
        ssd1680
            .clear_red_frame()
            .map_err(|err| anyhow::anyhow!("failed to clear SSD1680 driver: {err:?}"))?;

        let mut display_bw = graphics::Display2in13::bw();
        display_bw.set_rotation(graphics::DisplayRotation::Rotate270);
        let mut display_red = graphics::Display2in13::red();
        display_red.set_rotation(graphics::DisplayRotation::Rotate270);

        display_bw
            .clear(BinaryColor::On)
            .map_err(|err| anyhow::anyhow!("failed to clear SSD1680 display: {err:?}"))?;

        let mut metrics = client.fetch().await?;
        let positions = render_labels(
            &mut display_bw,
            MonoTextStyle::new(&profont::PROFONT_12_POINT, BinaryColor::Off),
            metrics.location.as_deref().unwrap_or("<unknown>"),
        )?;

        ssd1680
            .update_bw_frame(display_bw.buffer())
            .map_err(|err| anyhow::anyhow!("failed to update SSD1680 B/W frame: {err:?}"))?;
        let values_style = MonoTextStyle::new(&profont::PROFONT_12_POINT, BinaryColor::On);
        let mut interval = tokio::time::interval(Duration::from_secs(180));
        loop {
            tracing::debug!(?metrics);
            display_red
                .clear(BinaryColor::Off)
                .map_err(|err| anyhow::anyhow!("failed to clear SSD1680 display: {err:?}"))?;
            tracing::trace!("cleared display");

            render_values(&mut display_red, values_style, positions, &metrics)?;
            tracing::trace!("rendered display");

            ssd1680
                .update_red_frame(display_red.buffer())
                .map_err(|err| anyhow::anyhow!("failed to update SSD1680 B/W frame: {err:?}"))?;
            tracing::trace!("updated red display");

            ssd1680
                .display_frame(&mut delay)
                .map_err(|err| anyhow::anyhow!("failed to display frame on SSD1680: {err:?}"))?;
            tracing::trace!("displayed frame");

            interval.tick().await;
            metrics = client.fetch().await?;
        }
    }
}
