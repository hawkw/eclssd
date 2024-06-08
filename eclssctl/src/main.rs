use anyhow::Context;
use clap::Parser;
use eclss_app::{TraceArgs, TraceFormat};
use std::collections::BTreeMap;

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

    Info {
        /// the hostname of the ECLSS service
        hostname: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    eprintln!("eclssctl v{}\nEliza Laboratores", env!("CARGO_PKG_VERSION"));

    let args = Args::parse();
    args.trace
        .trace_init_with_default_format(TraceFormat::Pretty);

    match args.cmd {
        Command::Discover { duration } => discover_cmd(duration).await,
        Command::Info { .. } => anyhow::bail!("not yet implemented"),
    }
}

async fn discover_cmd(duration: humantime::Duration) -> anyhow::Result<()> {
    eprintln!();
    let svcs = discover(duration).await?;
    let len = svcs.len();
    eprintln!(
        "found {len} ECLSS service{}",
        match len {
            0 => "s",
            1 => ":",
            _ => "s:",
        }
    );

    for (hostname, svc) in svcs {
        let hostname = hostname.trim_end_matches('.');
        println!("\n  hostname: {hostname}");
        let port = svc.get_port();

        let addrs = svc.get_addresses();
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
        let v = svc
            .get_property_val_str("eclss_version")
            .unwrap_or("<unknown>");
        let location = svc
            .get_property_val_str("eclss_location")
            .unwrap_or("<unknown>");
        println!("\n   version: {v}\n  location: {location}");
    }
    Ok(())
}

async fn discover(
    duration: impl Into<std::time::Duration>,
) -> anyhow::Result<BTreeMap<String, mdns_sd::ServiceInfo>> {
    let mut svcs = BTreeMap::new();
    let mdns = mdns_sd::ServiceDaemon::new().context("failed to initialize mDNS daemon")?;
    let browse = mdns
        .browse("_eclss._tcp.local.")
        .context("failed to start mDNS browse")?;

    let duration = duration.into();
    tracing::debug!("browsing mDNS services for {duration:?}...");
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(1));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => {
                tracing::debug!("done browsing mDNS services");
                break;
            },
            evt = browse.recv_async() => {
                if let mdns_sd::ServiceEvent::ServiceResolved(svc) = evt? {
                    tracing::trace!(?svc, "found mDNS service");
                    svcs.insert(svc.get_hostname().to_string(), svc);
                }
            }
        }
    }

    Ok(svcs)
}
