//! What listens on this computer, and the addresses clients show for it.

use anyhow::{Result, anyhow};
use serde::Serialize;
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};

/// A process listening on a TCP socket.
#[derive(Clone, Debug)]
pub struct Listening {
    pub pid: u32,
    pub name: String,
    pub socket: SocketAddr,
}

/// One port as clients show it: `local` works on this computer, `remote` from other devices. Empty `remote`: the
/// server listens only on this computer, as most dev servers do by default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_field_names, reason = "the wire's name")]
pub struct Port {
    pub port: u16,
    pub local: String,
    pub remote: Vec<String>,
}

/// Every TCP listener on the computer. Blocking.
pub fn listening() -> Result<Vec<Listening>> {
    let all = listeners::get_all().map_err(|error| anyhow!("{error}"))?;
    Ok(all
        .into_iter()
        .filter(|l| l.protocol == listeners::Protocol::TCP && l.state == listeners::SocketState::Listen)
        .map(|l| Listening { pid: l.process.pid, name: l.process.name, socket: l.socket })
        .collect())
}

/// The sockets of `pids`, in a stable order so a scan can tell whether they changed.
pub fn sockets_of(listening: &[Listening], pids: &[u32]) -> Vec<SocketAddr> {
    let mut sockets: Vec<SocketAddr> = listening.iter().filter(|l| pids.contains(&l.pid)).map(|l| l.socket).collect();
    sockets.sort();
    sockets.dedup();
    sockets
}

/// This computer's addresses, to notice when it moves to another network. Blocking, but starts no process.
pub fn interfaces() -> Vec<IpAddr> {
    let mut all: Vec<IpAddr> =
        if_addrs::get_if_addrs().unwrap_or_default().iter().map(if_addrs::Interface::ip).collect();
    all.sort();
    all
}

/// The addresses for `sockets`. Blocking: reading this computer's addresses may start a process.
pub fn urls(sockets: &[SocketAddr]) -> Vec<Port> {
    urls_with(sockets, |port| {
        let usual = crate::service::interface_addresses(port);
        // With only a public address, what listens on all interfaces is open at that one: say so.
        if usual.is_empty() { crate::service::public_addresses(port) } else { usual }
    })
}

/// `reachable`: this computer's addresses other devices can reach, for a port.
pub(super) fn urls_with(sockets: &[SocketAddr], reachable: impl Fn(u16) -> Vec<String>) -> Vec<Port> {
    let mut by_port: BTreeMap<u16, Vec<SocketAddr>> = BTreeMap::new();
    for socket in sockets {
        by_port.entry(socket.port()).or_default().push(*socket);
    }
    by_port
        .into_iter()
        .map(|(port, sockets)| {
            // ponytail: an `IPV6_V6ONLY` socket on `[::]` counts too, though it serves none of these IPv4 addresses;
            // the socket doesn't say. Add IPv6 addresses to `reachable` if a server like that turns up.
            let everywhere = sockets.iter().any(|s| s.ip().is_unspecified());
            let on_loopback = everywhere || sockets.iter().any(|s| s.ip().is_loopback());
            let specific: Vec<String> = sockets
                .iter()
                .filter(|s| !s.ip().is_unspecified() && !s.ip().is_loopback())
                .map(|s| match s {
                    SocketAddr::V4(s) => format!("http://{}:{port}", s.ip()),
                    SocketAddr::V6(s) => format!("http://[{}]:{port}", s.ip()),
                })
                .collect();
            let remote = if everywhere { reachable(port) } else { specific.clone() };
            let local = if on_loopback {
                format!("http://localhost:{port}")
            } else {
                specific.first().cloned().unwrap_or_else(|| format!("http://localhost:{port}"))
            };
            Port { port, local, remote }
        })
        .collect()
}
