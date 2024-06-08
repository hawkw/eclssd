use anyhow::Context;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(next_help_heading = "Tracing")]
pub struct TraceArgs {
    /// Tracing-subscriber filter configuration
    #[clap(env = "ECLSS_LOG", long = "trace", default_value = "info,eclss=debug")]
    filter: tracing_subscriber::filter::Targets,

    /// Trace output format
    #[clap(env = "ECLSS_LOG_FORMAT", long = "trace-format")]
    format: Option<TraceFormat>,

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

#[derive(clap::ValueEnum, Debug, Copy, Clone, Default)]
#[clap(rename_all = "lower")]
pub enum TraceFormat {
    /// Human-readable text logging format.
    #[default]
    Text,
    /// Multi-line human-readable text logging format.
    Pretty,
    /// JSON logging format.
    Json,
    /// Log to journald, rather than to stdout.
    Journald,
}

impl TraceArgs {
    pub fn trace_init(&self) {
        self.trace_init_with_default_format(TraceFormat::default())
    }

    pub fn trace_init_with_default_format(&self, default: TraceFormat) {
        if let Some(format) = self.format {
            match self.init_format(format) {
                Ok(()) => return,
                Err(error) => eprintln!(
                    "failed to initialize {format:?} trace format, falling back \
                    to default {default:?} format: {error:#?}",
                ),
            }
        }
        self.init_format(default)
            .expect("default format must initialize")
    }

    fn init_format(&self, format: TraceFormat) -> anyhow::Result<()> {
        use tracing_subscriber::prelude::*;
        match format {
            #[cfg(target_os = "linux")]
            TraceFormat::Journald => {
                let journald =
                    tracing_journald::Layer::new().context("failed to connect to journald")?;
                tracing_subscriber::registry()
                    .with(self.filter.clone())
                    .with(journald)
                    .init();
            }
            #[cfg(not(target_os = "linux"))]
            TraceFormat::Journald => {
                anyhow::bail!("journald is only supported on Linux")
            }
            TraceFormat::Json => {
                let fmt = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(false)
                    .with_span_list(true)
                    .flatten_event(true)
                    .with_thread_ids(true);

                let registry = tracing_subscriber::registry().with(self.filter.clone());
                if self.no_timestamps {
                    registry.with(fmt.without_time()).init();
                } else {
                    registry.with(fmt).init();
                }
            }
            TraceFormat::Pretty => {
                let registry = tracing_subscriber::registry().with(self.filter.clone());
                let fmt = tracing_subscriber::fmt::layer()
                    .with_thread_ids(true)
                    .with_ansi(!self.no_color)
                    .pretty();
                if self.no_timestamps {
                    registry.with(fmt.without_time()).init();
                } else {
                    registry.with(fmt).init();
                }
            }
            TraceFormat::Text => {
                let registry = tracing_subscriber::registry().with(self.filter.clone());
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
        Ok(())
    }
}
