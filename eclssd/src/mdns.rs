use super::Args;
use anyhow::Context;

pub fn advertise(args: &Args) -> anyhow::Result<()> {
    let hostname = hostname::get().context("failed to determine hostname")?;
    let hostname = hostname
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("host_name is not valid UTF-8"))?;
    let host_name = format!("{hostname}.local.");
    let ip = local_ip_address::local_ip().context("failed to determine local IP address")?;
    let port = args.listen_addr.port();
    tracing::info!(
        ?hostname,
        ?ip,
        port,
        location = ?args.location,
        "starting mDNS advertisement",
    );
    let mdns = mdns_sd::ServiceDaemon::new().context("failed to start mDNS-SD daemon")?;
    let mut props = vec![("eclss_version", env!("CARGO_PKG_VERSION"))];
    let my_name = if let Some(location) = &args.location {
        props.push(("eclss_location", location.as_ref()));
        format!("ECLSS @ {location}")
    } else {
        format!("ECLSS @ {hostname}")
    };

    let ty_domains = [
        "_eclss._tcp.local.",
        "_http._tcp.local.",
        "_prometheus-http._tcp.local.",
    ];
    for ty_domain in ty_domains {
        let service_info =
            mdns_sd::ServiceInfo::new(ty_domain, &my_name, &host_name, ip, port, &props[..])
                .with_context(|| "failed to construct mDNS service info for '{ty_domain}'")?;
        mdns.register(service_info)
            .context("failed to register mDNS service")?;
        tracing::debug!("registered mDNS advertisement for {ty_domain}");
    }

    Ok(())
}
