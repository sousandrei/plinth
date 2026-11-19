//! LAN discovery for the shared peer transport.

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::transport::Result;

const SERVICE: &str = "_plinth-sync._tcp.local.";
const TIMEOUT: Duration = Duration::from_secs(5);

pub fn advertise(name: &str, addr: SocketAddr) -> Result<ServiceDaemon> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !addr.is_ipv4()
        || addr.ip().is_loopback()
    {
        return Err("advertising requires an alphanumeric name and a LAN IPv4 bind".into());
    }

    let host = format!("plinth-{name}.local.");
    let info = if addr.ip().is_unspecified() {
        ServiceInfo::new(SERVICE, name, &host, "", addr.port(), None)?.enable_addr_auto()
    } else {
        ServiceInfo::new(SERVICE, name, &host, addr.ip(), addr.port(), None)?
    };
    let daemon = ServiceDaemon::new()?;
    daemon.register(info)?;
    Ok(daemon)
}

pub fn discover(name: &str) -> Result<Vec<SocketAddr>> {
    let fullname = format!("{name}.{SERVICE}");
    let daemon = ServiceDaemon::new()?;
    let result = (|| -> Result<_> {
        let events = daemon.browse(SERVICE)?;
        let deadline = Instant::now() + TIMEOUT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(event) = events.recv_timeout(remaining) else {
                break;
            };
            if let ServiceEvent::ServiceResolved(info) = event
                && info.get_fullname().eq_ignore_ascii_case(&fullname)
            {
                let addrs: Vec<_> = info
                    .get_addresses_v4()
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip.into(), info.get_port()))
                    .collect();
                if !addrs.is_empty() {
                    return Ok(addrs);
                }
            }
        }
        Err(format!("no mDNS service named {name} found").into())
    })();
    stop(&daemon);
    result
}

pub fn stop(daemon: &ServiceDaemon) {
    if let Ok(status) = daemon.shutdown() {
        let _ = status.recv_timeout(TIMEOUT);
    }
}
