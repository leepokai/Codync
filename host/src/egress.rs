//! Egress policy: which hosts an outgoing request may reach.
//!
//! Requests whose URL comes from outside the host (a connector the user added, endpoints a
//! server's OAuth metadata names, download links in an API answer) get their client from
//! [`client`]. It resolves the name, checks every address against a [`Policy`], and pins those
//! addresses into the client, so a second lookup can't answer differently (DNS rebinding).
//! Cloud metadata and link-local addresses are never reachable; loopback and LAN only under
//! [`Policy::UserChosen`], and plain HTTP only to them. Redirects are followed within the same
//! origin only. Fixed first-party URLs (relay, updates, analytics) keep using `crate::http()`.
//!
//! Idea from rakazo's `packages/core/src/egress.ts` (Apache-2.0); no code copied.

use anyhow::{Context, Result, anyhow, bail};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::LockExt;

/// How long a pinned client (and so its DNS answer) is reused.
const KEEP: Duration = Duration::from_mins(5);
/// Redirects followed within one origin.
const MAX_REDIRECTS: usize = 5;
/// Alibaba Cloud's metadata service, inside the shared address space.
const ALIBABA_METADATA: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 200);
/// AWS's IPv6 metadata service, inside the unique local range.
const AWS_METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);

/// Which destinations a request may reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Policy {
    /// Public HTTPS hosts: URLs a remote server handed us.
    Public,
    /// Public HTTPS plus loopback and LAN, where plain HTTP is allowed too: a URL the user chose.
    UserChosen,
    /// Public HTTPS on exactly these hosts.
    Only(&'static [&'static str]),
}

/// What an address is, from most to least reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    Public,
    /// Loopback, private networks, CGNAT (Tailscale), unique local IPv6.
    Private,
    /// Link-local (cloud metadata), unspecified, multicast, broadcast, reserved.
    Forbidden,
}

fn reach(ip: IpAddr) -> Reach {
    match ip {
        IpAddr::V4(ip) => reach_v4(ip),
        IpAddr::V6(ip) => reach_v6(ip),
    }
}

fn reach_v4(ip: Ipv4Addr) -> Reach {
    let [a, b, c, _] = ip.octets();
    if ip.is_unspecified()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip == ALIBABA_METADATA
        || a == 0
        || a >= 240
    {
        Reach::Forbidden
    } else if ip.is_loopback()
        || ip.is_private()
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 192 && b == 0 && c == 0)
    {
        Reach::Private
    } else {
        Reach::Public
    }
}

fn reach_v6(ip: Ipv6Addr) -> Reach {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return reach_v4(v4);
    }
    let s = ip.segments();
    let v4 = |hi: u16, lo: u16| Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo));
    // Forms that reach an IPv4 address: IPv4-compatible (::a.b.c.d), NAT64 (64:ff9b::/96 and
    // the local-use 64:ff9b:1::/48) in the last 32 bits, 6to4 (2002::/16) in bits 16-48.
    let ipv4_compatible = s[..6] == [0; 6] && !ip.is_unspecified() && !ip.is_loopback();
    if ipv4_compatible || s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] || s[..3] == [0x64, 0xff9b, 1] {
        return reach_v4(v4(s[6], s[7]));
    }
    if s[0] == 0x2002 {
        return reach_v4(v4(s[1], s[2]));
    }
    if ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unicast_link_local()
        || ip == AWS_METADATA_V6
        || s[..2] == [0x2001, 0x0db8]
    {
        Reach::Forbidden
    } else if ip.is_loopback() || ip.is_unique_local() || s[0] & 0xffc0 == 0xfec0 {
        Reach::Private
    } else {
        Reach::Public
    }
}

/// `url`'s host without IPv6 brackets.
fn host(url: &reqwest::Url) -> Result<&str> {
    let host = url.host_str().filter(|h| !h.is_empty()).ok_or_else(|| anyhow!("the address has no host"))?;
    Ok(host.trim_start_matches('[').trim_end_matches(']'))
}

/// Every address `url`'s host resolves to (an IP literal is its own address).
async fn resolve(url: &reqwest::Url) -> Result<Vec<SocketAddr>> {
    let host = host(url)?;
    let port = url.port_or_known_default().ok_or_else(|| anyhow!("{host}: no port"))?;
    Ok(match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => {
            tokio::net::lookup_host((host, port)).await.with_context(|| format!("can't resolve {host}"))?.collect()
        }
    })
}

/// The addresses `url` may be reached at under `policy`; an error says why not.
async fn allowed(policy: Policy, url: &reqwest::Url) -> Result<Vec<SocketAddr>> {
    let host = host(url)?;
    if !matches!(url.scheme(), "https" | "http") {
        bail!("{host}: only https addresses can be reached");
    }
    if let Policy::Only(hosts) = policy
        && !hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
    {
        bail!("{host} isn't one of the hosts this request may reach");
    }
    let addrs = resolve(url).await?;
    let reaches: Vec<Reach> = addrs.iter().map(|a| reach(a.ip())).collect();
    let worst = reaches.iter().max().copied().ok_or_else(|| anyhow!("{host} has no address"))?;
    match (worst, policy) {
        (Reach::Forbidden, _) => bail!("{host} points at a link-local or reserved address, which is never reachable"),
        (Reach::Private, Policy::Public | Policy::Only(_)) => {
            bail!("{host} points at this computer or its local network, which this request may not reach")
        }
        _ => {}
    }
    let all_private = reaches.iter().all(|r| *r == Reach::Private);
    if url.scheme() == "http" && !(policy == Policy::UserChosen && all_private) {
        bail!("{host}: only https addresses can be reached");
    }
    Ok(addrs)
}

/// Refuses `url` when `policy` doesn't let it be reached (for checks before saving a URL).
pub async fn check(policy: Policy, url: &str) -> Result<()> {
    let url = reqwest::Url::parse(url).context("not a valid address")?;
    allowed(policy, &url).await.map(drop)
}

/// The policy for URLs that the server at `url`, reached under `policy`, hands us (OAuth
/// endpoints, an SSE endpoint): the local network only when the user chose that server and every
/// address of it is on that network, so a public server can't point us inward. Anything
/// uncertain (a bad URL, a failed lookup) is `Public`.
pub async fn derived(policy: Policy, url: &str) -> Policy {
    if policy != Policy::UserChosen {
        return Policy::Public;
    }
    let Ok(url) = reqwest::Url::parse(url) else { return Policy::Public };
    match resolve(&url).await {
        Ok(addrs) if !addrs.is_empty() && addrs.iter().all(|a| reach(a.ip()) == Reach::Private) => Policy::UserChosen,
        _ => Policy::Public,
    }
}

/// Reads a JSON response body of at most `max` bytes.
pub async fn json(res: reqwest::Response, max: usize) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&body(res, max).await?)?)
}

/// Policy, scheme, host, port.
type Key = (Policy, String, String, u16);
static CLIENTS: Mutex<Option<HashMap<Key, (Instant, reqwest::Client)>>> = Mutex::new(None);

/// A client for requests to `url`'s origin under `policy`, with its checked addresses pinned.
pub async fn client(policy: Policy, url: &str) -> Result<reqwest::Client> {
    let parsed = reqwest::Url::parse(url).context("not a valid address")?;
    let host = host(&parsed)?.to_ascii_lowercase();
    let key: Key =
        (policy, parsed.scheme().to_owned(), host.clone(), parsed.port_or_known_default().unwrap_or_default());
    if let Some((at, c)) = CLIENTS.locked().as_ref().and_then(|m| m.get(&key))
        && at.elapsed() < KEEP
    {
        return Ok(c.clone());
    }
    let addrs = match allowed(policy, &parsed).await {
        Ok(addrs) => addrs,
        Err(error) => {
            tracing::warn!(host = %host, policy = ?policy, error = format!("{error:#}"), "egress refused");
            return Err(error);
        }
    };
    let mut builder = reqwest::Client::builder().redirect(same_origin());
    if host.parse::<IpAddr>().is_err() {
        builder = builder.resolve_to_addrs(&host, &addrs);
    }
    let client = builder.build().context("TLS setup failed")?;
    let mut clients = CLIENTS.locked();
    let map = clients.get_or_insert_with(HashMap::new);
    map.retain(|_, (at, _)| at.elapsed() < KEEP);
    map.insert(key, (Instant::now(), client.clone()));
    Ok(client)
}

/// Follows a redirect only within the first request's origin (its addresses are the pinned ones).
fn same_origin() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let same = attempt.previous().first().is_some_and(|first| first.origin() == attempt.url().origin());
        if same && attempt.previous().len() <= MAX_REDIRECTS { attempt.follow() } else { attempt.stop() }
    })
}

/// GETs `url`, following redirects to other hosts too, each hop checked against `policy` and
/// pinned like the first (for downloads served from a CDN, like GitHub release assets).
pub async fn get_following(policy: Policy, url: &str, timeout: Duration) -> Result<reqwest::Response> {
    let mut url = reqwest::Url::parse(url).context("not a valid address")?;
    for _ in 0..=MAX_REDIRECTS {
        let res = client(policy, url.as_str()).await?.get(url.clone()).timeout(timeout).send().await?;
        if !res.status().is_redirection() {
            return Ok(res);
        }
        let location = res
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|l| l.to_str().ok())
            .ok_or_else(|| anyhow!("{} redirected without a location", host(&url).unwrap_or_default()))?;
        url = url.join(location).context("a redirect named a bad address")?;
    }
    bail!("too many redirects")
}

/// A response body of at most `max` bytes; a longer one is refused, not cut.
pub async fn body(mut res: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    let too_big = || anyhow!("the answer is larger than {} KB", max / 1024);
    if res.content_length().is_some_and(|n| usize::try_from(n).map_or(true, |n| n > max)) {
        return Err(too_big());
    }
    let mut out = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if out.len() + chunk.len() > max {
            return Err(too_big());
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> Reach {
        reach(s.parse().unwrap())
    }

    #[test]
    fn classifies_addresses() {
        for s in ["169.254.169.254", "100.100.100.200", "0.0.0.0", "224.0.0.1", "255.255.255.255", "fe80::1", "::"] {
            assert_eq!(ip(s), Reach::Forbidden, "{s}");
        }
        for s in ["fd00:ec2::254", "::ffff:169.254.169.254", "64:ff9b::a9fe:a9fe", "::a9fe:a9fe", "2002:a9fe:a9fe::1"] {
            assert_eq!(ip(s), Reach::Forbidden, "{s}");
        }
        for s in [
            "127.0.0.1",
            "10.0.0.8",
            "172.16.3.4",
            "192.168.1.1",
            "100.101.102.103",
            "::1",
            "fd12::1",
            "::7f00:1",
            "fec0::1",
        ] {
            assert_eq!(ip(s), Reach::Private, "{s}");
        }
        for s in ["1.1.1.1", "140.82.112.3", "2606:4700::1111", "::ffff:8.8.8.8"] {
            assert_eq!(ip(s), Reach::Public, "{s}");
        }
    }

    #[tokio::test]
    async fn metadata_is_never_reachable() {
        for policy in [Policy::Public, Policy::UserChosen] {
            assert!(check(policy, "http://169.254.169.254/latest/meta-data/").await.is_err());
            assert!(check(policy, "https://[fd00:ec2::254]/").await.is_err());
        }
    }

    #[tokio::test]
    async fn the_local_network_needs_a_user_chosen_url() {
        assert!(check(Policy::Public, "https://127.0.0.1/mcp").await.is_err());
        assert!(check(Policy::Public, "https://localhost/mcp").await.is_err());
        assert!(check(Policy::UserChosen, "http://127.0.0.1:3000/mcp").await.is_ok());
        assert!(check(Policy::UserChosen, "http://localhost:3000/mcp").await.is_ok());
        assert_eq!(derived(Policy::UserChosen, "http://127.0.0.1:3000/mcp").await, Policy::UserChosen);
        assert_eq!(derived(Policy::UserChosen, "https://1.1.1.1/mcp").await, Policy::Public);
        assert_eq!(derived(Policy::UserChosen, "https://no-such-host.invalid/mcp").await, Policy::Public);
    }

    #[tokio::test]
    async fn plain_http_is_only_for_the_local_network() {
        assert!(check(Policy::UserChosen, "http://1.1.1.1/").await.is_err());
        assert!(check(Policy::UserChosen, "https://1.1.1.1/").await.is_ok());
        assert!(check(Policy::UserChosen, "ftp://127.0.0.1/").await.is_err());
    }

    #[tokio::test]
    async fn an_allowlist_lets_only_its_hosts_through() {
        const ONE_HOST: Policy = Policy::Only(&["1.1.1.1"]);
        assert!(check(ONE_HOST, "https://1.1.1.1/x").await.is_ok());
        assert!(check(ONE_HOST, "https://8.8.8.8/x").await.is_err());
    }

    /// A loopback server: `/away` redirects to another origin, `/hop` to the same server under
    /// another name, `/metadata` to the cloud metadata address, `/here` within its own origin,
    /// `/big` is 2 KB.
    async fn server() -> String {
        use axum::{Router, response::Redirect, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hop = format!("http://localhost:{port}/ok");
        let router = Router::new()
            .route("/away", get(|| async { Redirect::temporary("http://localhost:9/") }))
            .route("/hop", get(move || std::future::ready(Redirect::temporary(&hop))))
            .route("/metadata", get(|| async { Redirect::temporary("http://169.254.169.254/latest/") }))
            .route("/here", get(|| async { Redirect::temporary("/ok") }))
            .route("/ok", get(|| async { "ok" }))
            .route("/big", get(|| async { "x".repeat(2048) }));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn downloads_follow_redirects_that_pass_the_policy() {
        let base = server().await;
        let wait = Duration::from_secs(5);
        let hop = get_following(Policy::UserChosen, &format!("{base}/hop"), wait).await.unwrap();
        assert_eq!(hop.text().await.unwrap(), "ok");
        assert!(get_following(Policy::UserChosen, &format!("{base}/metadata"), wait).await.is_err());
    }

    #[tokio::test]
    async fn redirects_stay_within_the_origin() {
        let base = server().await;
        let c = client(Policy::UserChosen, &base).await.unwrap();
        let away = c.get(format!("{base}/away")).send().await.unwrap();
        assert!(away.status().is_redirection());
        let here = c.get(format!("{base}/here")).send().await.unwrap();
        assert_eq!(here.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn bodies_are_capped() {
        let base = server().await;
        let c = client(Policy::UserChosen, &base).await.unwrap();
        let big = c.get(format!("{base}/big")).send().await.unwrap();
        assert!(body(big, 1024).await.is_err());
        let big = c.get(format!("{base}/big")).send().await.unwrap();
        assert_eq!(body(big, 4096).await.unwrap().len(), 2048);
    }
}
