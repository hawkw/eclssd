use super::*;
use crossterm::{
    event::{self, EventStream, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use eclss_api::Metrics;
use futures::stream::StreamExt;
use ratatui::{
    prelude::*,
    symbols::border,
    widgets::{
        block::{Block, Position, Title},
        Borders, Paragraph,
    },
};
use std::io::stdout;

impl TerminalArgs {
    pub(super) async fn run(self, client: Client) -> anyhow::Result<()> {
        stdout().execute(EnterAlternateScreen)?;
        enable_raw_mode()?;
        let result = tokio::task::spawn(self.run_inner(client)).await;

        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;
        result.context("terminal task panicked")??;
        Ok(())
    }

    async fn run_inner(self, mut client: Client) -> anyhow::Result<()> {
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        terminal.clear()?;

        let mut input = Box::pin(EventStream::new());
        let mut interval = client.refresh_interval();
        let fetch = client.fetch().await;
        let mut app = App {
            args: self,
            fetch,
            conn: Line::from(vec![
                "Connected to: ".into(),
                Span::styled(client.metrics_url.to_string(), Style::new().underlined()),
            ]),
        };
        loop {
            terminal.draw(|frame| {
                frame.render_widget(&app, frame.size());
            })?;

            let fetch = async {
                interval.tick().await;
                client.fetch().await
            };
            tokio::select! {
                biased;

                event = input.next() => {
                    let event = event
                        .ok_or_else(|| anyhow::anyhow!("keyboard event stream ended early, this is a bug"))?
                        .context("keyboard event stream error")?;
                    if let event::Event::Key(event::KeyEvent {
                        kind: KeyEventKind::Press,
                        code: KeyCode::Char(c),
                        ..
                    }) = event
                    {
                        if c == 'q' || c == 'Q' {
                            return Ok(());
                        }
                    }
                }

                fetch = fetch => {
                    app.fetch = fetch;
                },
            }
            interval.tick().await;
            app.fetch = client.fetch().await;
        }
    }
}

struct App {
    #[allow(dead_code)]
    args: TerminalArgs,
    conn: Line<'static>,
    fetch: anyhow::Result<Metrics>,
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.fetch {
            Ok(Metrics {
                location: Some(ref location),
                ..
            }) => Title::from(format!(" ECLSS READOUT - {location} ").bold()),
            _ => Title::from(" ECLSS READOUT ".bold()),
        };
        let instructions = Title::from(Line::from(vec![" Quit ".into(), "<q/Q> ".blue().bold()]));
        let block = Block::default()
            .title(title.alignment(Alignment::Center))
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);
        let reading_style = Style::new().bold();

        let text = match self.fetch {
            Ok(ref metrics) => {
                let temp = mean(&metrics.temp_c)
                    .map(|temp_c| {
                        let temp_f = temp_c_to_f(temp_c);
                        Line::from(vec![
                            "Temperature: ".into(),
                            Span::styled(format!("{temp_f:.2} °F"), reading_style),
                            " / ".into(),
                            Span::styled(format!("{temp_c:.2} °C"), reading_style),
                        ])
                    })
                    .unwrap_or_else(|| {
                        Line::from(vec![
                            "Temperature: ".into(),
                            Span::styled("???", reading_style),
                        ])
                    });

                let rel_humidity = mean(&metrics.rel_humidity_percent)
                    .map(|h| {
                        Line::from(vec![
                            "Relative Humidity: ".into(),
                            Span::styled(format!("{h:02.2}"), reading_style),
                            "%".into(),
                        ])
                    })
                    .unwrap_or_else(|| {
                        Line::from(vec![
                            "Relative Humidity: ".into(),
                            Span::styled("???", reading_style),
                        ])
                    });

                let abs_humidity = mean(&metrics.abs_humidity_grams_m3)
                    .map(|h| {
                        Line::from(vec![
                            "Absolute Humidity: ".into(),
                            Span::styled(format!("{h:02.2}"), reading_style),
                            " g/m³".into(),
                        ])
                    })
                    .unwrap_or_else(|| {
                        Line::from(vec![
                            "Absolute Humidity: ".into(),
                            Span::styled("???", reading_style),
                        ])
                    });

                let co2 = mean(&metrics.co2_ppm)
                    .map(|co2| {
                        Line::from(vec![
                            "CO₂: ".into(),
                            Span::styled(format!("{co2:03.2}"), reading_style),
                            " ppm".into(),
                        ])
                    })
                    .unwrap_or_else(|| {
                        Line::from(vec!["CO₂: ".into(), Span::styled("???", reading_style)])
                    });

                let tvoc = mean(&metrics.tvoc_ppb)
                    .map(|t| {
                        Line::from(vec![
                            "tVOC: ".into(),
                            Span::styled(format!("{t:03.2}"), reading_style),
                            " ppb".into(),
                        ])
                    })
                    .unwrap_or_else(|| {
                        Line::from(vec!["tVOC: ".into(), Span::styled("???", reading_style)])
                    });
                Text::from(vec![
                    self.conn.clone(),
                    Line::from(Vec::new()),
                    temp,
                    rel_humidity,
                    abs_humidity,
                    co2,
                    tvoc,
                ])
            }
            Err(ref error) => {
                let mut text = Text::from(vec![
                    self.conn.clone(),
                    Line::from(Vec::new()),
                    Line::from(vec![Span::styled(
                        "METRICS FETCH ERROR",
                        Style::new().bold().fg(Color::Red),
                    )]),
                    Line::from(Vec::new()),
                ]);
                text.extend(
                    format!("{error:?}")
                        .lines()
                        .map(|l| Line::from(l.to_string())),
                );
                text
            }
        };

        Paragraph::new(text).block(block).render(area, buf)
    }
}
