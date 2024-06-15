use anyhow::{Context, Ok};
use clap::Parser;
use eclss_app::{TraceArgs, TraceFormat};
use futures::stream::{self, Stream, StreamExt};
use std::collections::{BTreeMap, HashSet};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

#[derive(Debug, Parser)]
struct Args {
    #[clap(flatten)]
    trace: TraceArgs,

    #[clap(subcommand)]
    cmd: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// discover and list ECLSS services
    Discover {
        /// how long to browse for services
        #[clap(long, short, default_value = "1s")]
        duration: humantime::Duration,
    },

    /// lookup sensor status for a node
    Status {
        #[clap(flatten)]
        query: NodeQuery,
    },
}

#[derive(Debug, Parser)]
#[command(next_help_heading = "Node Selection")]
struct NodeQuery {
    #[clap(long = "url", short)]
    urls: Vec<reqwest::Url>,

    #[clap(long = "location", short)]
    locations: Vec<String>,
}

#[derive(Debug)]
struct NodeInfo {
    hostname: String,
    port: u16,
    addrs: HashSet<std::net::IpAddr>,
    version: Option<String>,
    location: Option<String>,
}

impl NodeQuery {
    async fn urls(
        &mut self,
        background: &mut JoinSet<anyhow::Result<()>>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<(String, reqwest::Url)>>>>> {
        let discover_all = self.locations.is_empty() && self.urls.is_empty();
        if discover_all {
            let rx = discover(Duration::from_secs(1), background)?;
            let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|info| {
                let addr = info.addrs.iter().next().ok_or_else(|| {
                    anyhow::anyhow!("no addresses resolved for {}", info.hostname)
                })?;
                let url = format!("http://{addr}:{}/", info.port)
                    .parse()
                    .with_context(|| format!("failed to parse URL for {}", info.hostname))?;
                Ok((info.hostname, url))
            });
            return Ok(Box::pin(stream));
        }
        Ok(Box::pin(stream::iter(
            std::mem::take(&mut self.urls)
                .into_iter()
                .map(|url| Ok((url.host_str().unwrap().to_owned(), url))),
        )))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    eprintln!("eclssctl v{}\nEliza Laboratores", env!("CARGO_PKG_VERSION"));

    let args = Args::parse();
    args.trace
        .trace_init_with_default_format(TraceFormat::Pretty);
    let mut background = tokio::task::JoinSet::new();
    match args.cmd {
        Command::Discover { duration } => discover_cmd(duration, &mut background).await?,
        Command::Status { mut query } => {
            tracing::info!(?query, "querying node status...");
            let mut urls = query.urls(&mut background).await?;
            while let Some(url) = urls.next().await {
                let (name, url) = url?;
                background.spawn(async move {
                    let result = reqwest::get(url.join("/sensors.json")?)
                        .await?
                        .json::<BTreeMap<String, eclss_api::SensorState>>()
                        // .text()
                        .await?;
                    println!("node: {name} ({url})");
                    for (name, state) in result {
                        println!("    sensor {name}: {state:?}");
                    }

                    Ok(())
                });
            }
        }
    };

    while let Some(bg) = background.join_next().await {
        bg.context("a background task panicked")?
            .context("a background task returned an error")?;
    }

    Ok(())
}

async fn discover_cmd(
    duration: humantime::Duration,
    background: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    eprintln!("\n discovering ECLSS services...");
    let mut svcs = discover(duration, background)?;

    while let Some(NodeInfo {
        hostname,
        port,
        addrs,
        version,
        location,
    }) = svcs.recv().await
    {
        let hostname = hostname.trim_end_matches('.');
        println!("\n  hostname: {hostname}");
        let (before, after) = if addrs.len() == 1 {
            ("  ", "")
        } else {
            ("", "es")
        };
        print!(" {before}address{after}: ",);
        let mut addrs = addrs.iter();
        if let Some(addr) = addrs.next() {
            print!("{addr}:{port}");
            for addr in addrs {
                print!(", {addr}:{port}");
            }
        }
        let version = version.as_deref().unwrap_or("<unknown>");
        let location = location.as_deref().unwrap_or("<unknown>");
        println!("\n   version: {version}\n  location: {location}");
    }
    Ok(())
}

fn discover(
    duration: impl Into<std::time::Duration>,
    background: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<mpsc::Receiver<NodeInfo>> {
    let (tx, rx) = mpsc::channel(16);
    let mdns = mdns_sd::ServiceDaemon::new().context("failed to initialize mDNS daemon")?;
    let browse = mdns
        .browse("_eclss._tcp.local.")
        .context("failed to start mDNS browse")?;

    async fn discover_inner(
        tx: mpsc::Sender<NodeInfo>,
        duration: Duration,
        browse: mdns_sd::Receiver<mdns_sd::ServiceEvent>,
    ) -> anyhow::Result<()> {
        tracing::debug!("browsing mDNS services for {duration:?}...");
        let timeout = tokio::time::sleep(duration);
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                _ = &mut timeout => {
                    tracing::debug!("done browsing mDNS services");
                    return Ok(());
                },
                evt = browse.recv_async() => {
                    if let mdns_sd::ServiceEvent::ServiceResolved(svc) = evt? {
                        tracing::trace!(?svc, "found mDNS service");
                        tx.send(NodeInfo {
                            hostname: svc.get_hostname().to_string(),
                            port: svc.get_port(),
                            addrs: svc.get_addresses().clone(),
                            version: svc.get_property_val_str("eclss_version").map(ToOwned::to_owned),
                            location: svc.get_property_val_str("eclss_location").map(ToOwned::to_owned),
                        }).await?;
                    }
                }
            }
        }
    }

    let duration = duration.into();
    background.spawn(async move {
        if let Err(e) = discover_inner(tx, duration, browse).await {
            tracing::error!(?e, "error during mDNS discovery");
        }
        let shutdown = mdns
            .shutdown()
            .context("failed to shutdown mDNS daemon")?
            .recv_async()
            .await
            .context("failed to await mDNS daemon shutdown")?;
        anyhow::ensure!(
            shutdown == mdns_sd::DaemonStatus::Shutdown,
            "mDNS daemon did not shut down properly"
        );
        Ok(())
    });

    Ok(rx)
}
