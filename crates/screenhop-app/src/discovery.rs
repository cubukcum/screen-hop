//! Peer discovery (M3.2, plan §8.1): find other screen-hop peers on the LAN via **mDNS**, with a
//! first-class **manual host** path for networks where multicast is blocked or flaky.
//!
//! Both sources implement [`Discovery`] and are combined by [`merge`]. `ManualHosts` is pure and
//! fully unit-tested. [`MdnsDiscovery`] wraps `mdns-sd`; its multicast behaviour can only be
//! verified on a real LAN (see the M3 maintainer checklist), so the unit tests here cover the
//! manual path and the merge/dedup logic, which is where the routing decisions actually live.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};

/// The mDNS service type screen-hop peers advertise and browse for.
pub const SERVICE_TYPE: &str = "_screenhop._tcp.local.";
/// TXT-record key carrying a peer's stable id, so a browse result can be matched to a known peer.
pub const TXT_PEER_ID: &str = "peer_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerSource {
    Manual,
    Mdns,
}

/// A peer we might connect to, from either discovery source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// The peer's stable id, when known (mDNS TXT, or learned after handshake). Manual entries
    /// start as `None` — the id is confirmed by the handshake on connect.
    pub peer_id: Option<String>,
    pub addr: SocketAddr,
    pub source: PeerSource,
}

/// Anything that can enumerate currently-known peers.
pub trait Discovery {
    fn peers(&self) -> Vec<DiscoveredPeer>;
}

/// Manually-entered hosts — the always-available fallback when mDNS is blocked (§8.1).
#[derive(Debug, Clone, Default)]
pub struct ManualHosts {
    hosts: Vec<SocketAddr>,
}

impl ManualHosts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and add a `host:port` entry (an IP or a resolvable name). Returns `false` if it does
    /// not resolve to any socket address. De-duplicates.
    pub fn add(&mut self, host_port: &str) -> bool {
        match host_port.trim().to_socket_addrs() {
            Ok(addrs) => {
                let mut any = false;
                for addr in addrs {
                    any = true;
                    if !self.hosts.contains(&addr) {
                        self.hosts.push(addr);
                    }
                }
                any
            }
            Err(_) => false,
        }
    }

    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }
}

impl Discovery for ManualHosts {
    fn peers(&self) -> Vec<DiscoveredPeer> {
        self.hosts
            .iter()
            .map(|&addr| DiscoveredPeer {
                peer_id: None,
                addr,
                source: PeerSource::Manual,
            })
            .collect()
    }
}

/// Merge peers from several discovery sources, de-duplicated by socket address. A manually-entered
/// address wins its `source` (the user asserted it explicitly), and a known `peer_id` from any
/// source is preferred over `None`. Output is sorted by address for deterministic iteration.
pub fn merge(sources: &[&dyn Discovery]) -> Vec<DiscoveredPeer> {
    let mut by_addr: BTreeMap<SocketAddr, DiscoveredPeer> = BTreeMap::new();
    for src in sources {
        for peer in src.peers() {
            by_addr
                .entry(peer.addr)
                .and_modify(|existing| {
                    if existing.peer_id.is_none() && peer.peer_id.is_some() {
                        existing.peer_id = peer.peer_id.clone();
                    }
                    if peer.source == PeerSource::Manual {
                        existing.source = PeerSource::Manual;
                    }
                })
                .or_insert(peer);
        }
    }
    by_addr.into_values().collect()
}

// ---- mDNS ------------------------------------------------------------------

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

/// mDNS / DNS-SD discovery over `mdns-sd`: browses for `_screenhop._tcp` peers in a background
/// thread and (optionally) announces this peer so others can find it. Real multicast behaviour is
/// LAN-verified (M3 checklist); the routing logic it feeds is exercised by the manual-path tests.
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    found: Arc<Mutex<BTreeMap<String, DiscoveredPeer>>>,
    _browser: JoinHandle<()>,
}

impl MdnsDiscovery {
    /// Start browsing for peers. Resolved instances accumulate in the background until dropped.
    pub fn start() -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let receiver = daemon.browse(SERVICE_TYPE)?;
        let found: Arc<Mutex<BTreeMap<String, DiscoveredPeer>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let found_bg = Arc::clone(&found);

        let browser = thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let peer_id = info.get_property_val_str(TXT_PEER_ID).map(str::to_owned);
                    let port = info.get_port();
                    let mut map = found_bg.lock().unwrap_or_else(|e| e.into_inner());
                    for ip in info.get_addresses_v4() {
                        map.insert(
                            info.get_fullname().to_owned(),
                            DiscoveredPeer {
                                peer_id: peer_id.clone(),
                                addr: SocketAddr::new(ip.into(), port),
                                source: PeerSource::Mdns,
                            },
                        );
                    }
                }
            }
        });

        Ok(Self {
            daemon,
            found,
            _browser: browser,
        })
    }

    /// Announce THIS peer so others discover it. `peer_id` travels in a TXT record; `port` is this
    /// node's mesh listen port. We pass the detected primary IPv4 when available (empty otherwise)
    /// and also `enable_addr_auto` so the advertised set stays current as interfaces change.
    pub fn announce(&self, peer_id: &str, port: u16) -> Result<(), mdns_sd::Error> {
        let host = format!("{peer_id}.local.");
        let ip = primary_local_ipv4()
            .map(|v4| v4.to_string())
            .unwrap_or_default();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            peer_id, // instance name (unique per peer)
            &host,
            ip.as_str(),
            port,
            &[(TXT_PEER_ID, peer_id)][..],
        )?
        .enable_addr_auto();
        self.daemon.register(info)
    }
}

/// Best-effort primary IPv4 of this host: open a UDP socket and ask the OS which local address it
/// would use to reach an off-link target. No packets are sent (UDP `connect` only selects a route),
/// so this is fast and works offline as long as an interface/route exists. `None` if it can't tell.
fn primary_local_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?; // route selection only — nothing is transmitted
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if !v4.is_unspecified() => Some(v4),
        _ => None,
    }
}

impl Discovery for MdnsDiscovery {
    fn peers(&self) -> Vec<DiscoveredPeer> {
        self.found
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

impl Drop for MdnsDiscovery {
    fn drop(&mut self) {
        // Stop the daemon; the browse channel closes and the browser thread exits.
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_hosts_parse_dedup_and_report_as_manual() {
        let mut m = ManualHosts::new();
        assert!(m.add("127.0.0.1:7777"));
        assert!(m.add("127.0.0.1:7777")); // duplicate
        assert!(m.add("10.0.0.5:9000"));
        assert_eq!(m.len(), 2);
        let peers = m.peers();
        assert!(peers
            .iter()
            .all(|p| p.source == PeerSource::Manual && p.peer_id.is_none()));
    }

    #[test]
    fn manual_add_rejects_garbage() {
        let mut m = ManualHosts::new();
        assert!(!m.add("not-a-socket-addr"));
        assert!(!m.add(""));
        assert!(m.is_empty());
    }

    #[test]
    fn merge_dedups_by_addr_and_prefers_manual_and_known_id() {
        // An mDNS source that knows the peer id, and a manual source for the same address.
        struct FixedMdns(Vec<DiscoveredPeer>);
        impl Discovery for FixedMdns {
            fn peers(&self) -> Vec<DiscoveredPeer> {
                self.0.clone()
            }
        }
        let addr: SocketAddr = "10.0.0.5:7777".parse().unwrap();
        let mdns = FixedMdns(vec![DiscoveredPeer {
            peer_id: Some("peerB".into()),
            addr,
            source: PeerSource::Mdns,
        }]);
        let mut manual = ManualHosts::new();
        assert!(manual.add("10.0.0.5:7777"));

        let merged = merge(&[&mdns, &manual]);
        assert_eq!(merged.len(), 1, "same address must collapse to one entry");
        let p = &merged[0];
        assert_eq!(p.addr, addr);
        assert_eq!(p.peer_id.as_deref(), Some("peerB"), "known id is kept");
        assert_eq!(
            p.source,
            PeerSource::Manual,
            "manual assertion wins the source"
        );
    }
}
