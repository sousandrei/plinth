//! LAN discovery for the shared peer transport.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::transport::Result;

const SERVICE: &str = "_plinth-sync._tcp.local.";
const TIMEOUT: Duration = Duration::from_secs(5);
const SCAN_TIMEOUT: Duration = Duration::from_secs(3);

/// A connection candidate announced on the LAN. Pairing verifies its identity.
#[derive(Debug)]
pub struct DiscoveredPeer {
    pub name: String,
    pub addresses: Vec<SocketAddr>,
}

pub struct Advertisement(ServiceDaemon);

impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.0.shutdown();
    }
}

pub fn advertise_managed(name: &str, addr: SocketAddr) -> Result<Advertisement> {
    advertise(name, addr).map(Advertisement)
}

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
    browse(TIMEOUT, |event| {
        let ServiceEvent::ServiceResolved(info) = event else {
            return None;
        };
        if !info.get_fullname().eq_ignore_ascii_case(&fullname) {
            return None;
        }
        let addresses = resolved_addresses(&info);
        (!addresses.is_empty()).then_some(addresses)
    })?
    .ok_or_else(|| format!("no mDNS service named {name} found").into())
}

/// List all resolved Plinth peers instead of searching for a known name.
pub fn discover_peers() -> Result<Vec<DiscoveredPeer>> {
    let mut peers = BTreeMap::new();
    browse::<()>(SCAN_TIMEOUT, |event| {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                if let Some(name) = instance_name(info.get_fullname()) {
                    let addresses = resolved_addresses(&info);
                    let key = name.to_ascii_lowercase();
                    if addresses.is_empty() {
                        peers.remove(&key);
                    } else {
                        peers.insert(
                            key,
                            DiscoveredPeer {
                                name: name.to_owned(),
                                addresses,
                            },
                        );
                    }
                }
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                if let Some(name) = instance_name(&fullname) {
                    peers.remove(&name.to_ascii_lowercase());
                }
            }
            _ => {}
        }
        None
    })?;
    Ok(peers.into_values().collect())
}

fn instance_name(fullname: &str) -> Option<&str> {
    let suffix_length = SERVICE.len() + 1;
    let (name, suffix) = fullname.split_at_checked(fullname.len().checked_sub(suffix_length)?)?;
    let service = suffix.strip_prefix('.')?;
    (!name.is_empty() && service.eq_ignore_ascii_case(SERVICE)).then_some(name)
}

fn resolved_addresses(info: &ResolvedService) -> Vec<SocketAddr> {
    let port = info.get_port();
    if port == 0 {
        return Vec::new();
    }
    let mut addresses: Vec<_> = info
        .get_addresses_v4()
        .into_iter()
        .filter(|ip| {
            !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast() && !ip.is_broadcast()
        })
        .map(|ip| SocketAddr::new(ip.into(), port))
        .collect();
    addresses.sort_unstable();
    addresses
}

fn browse<T>(
    timeout: Duration,
    mut select: impl FnMut(ServiceEvent) -> Option<T>,
) -> Result<Option<T>> {
    let daemon = ServiceDaemon::new()?;
    let result = (|| {
        let events = daemon.browse(SERVICE)?;
        let deadline = Instant::now() + timeout;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(event) = events.recv_timeout(remaining) else {
                break;
            };
            if let Some(found) = select(event) {
                return Ok(Some(found));
            }
        }
        Ok(None)
    })();
    stop(&daemon);
    result
}

pub fn stop(daemon: &ServiceDaemon) {
    if let Ok(status) = daemon.shutdown() {
        let _ = status.recv_timeout(TIMEOUT);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        net::{IpAddr, TcpListener},
        process,
    };

    use super::*;

    #[test]
    #[ignore = "requires PLINTH_TEST_LAN_IP and access to LAN multicast"]
    fn discovers_multiple_advertised_peers_without_knowing_the_names() {
        let ip: IpAddr = env::var("PLINTH_TEST_LAN_IP").unwrap().parse().unwrap();
        let listeners = [
            TcpListener::bind((ip, 0)).unwrap(),
            TcpListener::bind((ip, 0)).unwrap(),
        ];
        let addresses = listeners
            .each_ref()
            .map(|listener| listener.local_addr().unwrap());
        let names = ['a', 'b'].map(|suffix| format!("plinth-discovery-{}-{suffix}", process::id()));
        let advertisers = [
            advertise(&names[0], addresses[0]).unwrap(),
            advertise(&names[1], addresses[1]).unwrap(),
        ];
        let peers = discover_peers();
        let named = discover(&names[0]);
        for daemon in &advertisers {
            stop(daemon);
        }

        let peers = peers.unwrap();
        for (name, address) in names.iter().zip(addresses) {
            let matches: Vec<_> = peers.iter().filter(|peer| &peer.name == name).collect();
            assert_eq!(
                matches.len(),
                1,
                "expected one announcement for {name}: {peers:?}"
            );
            assert!(matches[0].addresses.contains(&address));
        }
        assert!(named.unwrap().contains(&addresses[0]));
    }
}
