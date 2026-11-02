//! The addresses phones dial this host at, sent in the pairing link and every `hello`.

use serde_json::Value;
use std::process::Command;
use std::sync::LazyLock;

/// The most addresses sent, best first: a phone keeps at most this many, and a longer list is cut to it.
const MAX_ADDRESSES: usize = 16;

/// Addresses the phone can try, best first: the ones set in `CODYNC_EXTERNAL_URL`, Tailscale, then LAN.
pub fn addresses(port: u16) -> Vec<String> {
    // Read once: every hello asks, and a bad entry warns once.
    static EXTERNAL: LazyLock<Vec<String>> =
        LazyLock::new(|| external_urls(&std::env::var("CODYNC_EXTERNAL_URL").unwrap_or_default()));
    let mut tailscale = vec![];
    let mut lan = vec![];
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        if iface.is_loopback() || is_container_bridge(&iface.name) {
            continue;
        }
        if let std::net::IpAddr::V4(ip) = iface.ip() {
            let o = ip.octets();
            // 100.64.0.0/10 is Tailscale's CGNAT range.
            if o[0] == 100 && (64..128).contains(&o[1]) {
                tailscale.push(format!("http://{ip}:{port}"));
            } else if ip.is_private() {
                lan.push(ip);
            }
        }
    }
    if let Ok(out) = Command::new("tailscale").args(["status", "--json"]).output()
        && let Ok(v) = serde_json::from_slice::<Value>(&out.stdout)
        && let Some(dns) = v["Self"]["DNSName"].as_str()
    {
        let dns = dns.trim_end_matches('.');
        if !dns.is_empty() {
            tailscale.insert(0, format!("http://{dns}:{port}"));
        }
    }
    tailscale.sort();
    tailscale.dedup();
    lan.sort_by_key(|ip| (lan_rank(*ip), *ip));
    lan.dedup();
    let lan = lan.into_iter().map(|ip| format!("http://{ip}:{port}"));
    EXTERNAL.iter().cloned().chain(tailscale).chain(lan).take(MAX_ADDRESSES).collect()
}

/// Home networks first, then office ones, so the cap drops the private ranges container and VM networks favor.
// ponytail: guesses by range; ask the OS for the default route's interface if a LAN address still gets cut.
fn lan_rank(ip: std::net::Ipv4Addr) -> u8 {
    match ip.octets() {
        [192, 168, ..] => 0,
        [10, ..] => 1,
        _ => 2,
    }
}

/// A bridge that containers or VMs on this computer sit behind (Docker, Podman, Kubernetes, libvirt, LXD).
/// No phone reaches the host through one, and a server with many Docker networks has dozens.
fn is_container_bridge(name: &str) -> bool {
    const PREFIXES: [&str; 12] =
        ["docker", "br-", "veth", "virbr", "lxcbr", "lxdbr", "cni", "flannel", "cali", "weave", "podman", "kube"];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// Where phones reach this host besides its own interfaces, such as an HTTPS reverse proxy that forwards
/// `/channel`: comma-separated `http(s)://` URLs at the root of a host (phones dial `<url>/channel`). The pairing
/// link and every `hello` carry them, so a phone keeps them when it refreshes its addresses.
fn external_urls(var: &str) -> Vec<String> {
    var.split(',')
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .filter_map(|u| match reqwest::Url::parse(u) {
            Ok(url)
                if matches!(url.scheme(), "http" | "https")
                    && url.host().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none() =>
            {
                Some(url.as_str().trim_end_matches('/').to_owned())
            }
            _ => {
                tracing::warn!(
                    url = u,
                    "CODYNC_EXTERNAL_URL entry isn't an http(s) URL at the root of a host (no user or password); ignored"
                );
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_urls_keep_http_addresses_only() {
        assert_eq!(
            external_urls(
                " https://codync.example.com/ , ftp://x, nonsense,http://10.0.0.2:8080,https://x.com/codync,https://x.com/?a,https://u:p@x.com"
            ),
            ["https://codync.example.com", "http://10.0.0.2:8080"]
        );
        assert_eq!(external_urls(""), [] as [String; 0]);
    }

    #[test]
    fn home_networks_rank_before_the_cap() {
        let mut ips: Vec<std::net::Ipv4Addr> =
            ["172.20.0.1", "10.0.0.10", "192.168.1.20", "10.0.0.9"].iter().map(|s| s.parse().unwrap()).collect();
        ips.sort_by_key(|ip| (lan_rank(*ip), *ip));
        let sorted: Vec<String> = ips.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, ["192.168.1.20", "10.0.0.9", "10.0.0.10", "172.20.0.1"]);
    }

    #[test]
    fn container_bridges_are_not_addresses() {
        for name in
            ["docker0", "docker_gwbridge", "br-3f2a9c1d7e4b", "veth12ab", "virbr0", "lxdbr0", "cni0", "flannel.1"]
        {
            assert!(is_container_bridge(name), "{name}");
        }
        for name in ["eth0", "enp0s3", "en0", "wlan0", "br0", "bridge0", "tailscale0", "ens3"] {
            assert!(!is_container_bridge(name), "{name}");
        }
    }
}
