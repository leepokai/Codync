//! Bounded HTTPS imports. Pin validated DNS answers and validate every redirect.
use anyhow::{Context, Result, bail, ensure};
use futures::StreamExt;
use reqwest::{Client, Url};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

pub(super) async fn download(raw: &str) -> Result<Vec<u8>> {
    ensure!(raw.len() <= 2048, "Image URL is too long");
    tokio::time::timeout(Duration::from_secs(20), fetch(raw)).await.context("Image download timed out")?
}

async fn fetch(raw: &str) -> Result<Vec<u8>> {
    let mut url = Url::parse(raw).context("Invalid image URL")?;
    for _ in 0..4 {
        ensure!(
            url.scheme() == "https" && url.port_or_known_default() == Some(443),
            "Use an HTTPS image URL on port 443"
        );
        ensure!(url.username().is_empty() && url.password().is_none(), "Image URLs cannot contain credentials");
        let host = url.host_str().context("Image URL has no host")?;
        let addresses: Vec<SocketAddr> = match host.trim_matches(['[', ']']).parse::<IpAddr>() {
            Ok(ip) => vec![SocketAddr::new(ip, 443)],
            Err(_) => tokio::net::lookup_host((host, 443)).await.context("Cannot resolve image host")?.collect(),
        };
        ensure!(
            !addresses.is_empty() && addresses.iter().all(|a| public_ip(a.ip())),
            "Image URLs must use public internet addresses"
        );
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .resolve_to_addrs(host, &addresses)
            .build()?;
        let response = client.get(url.clone()).send().await.context("Cannot download image")?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("Image redirect has no destination")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        let response = response.error_for_status().context("Image server refused the download")?;
        ensure!(
            response.content_length().is_none_or(|n| n <= super::image::MAX_INPUT as u64),
            "Choose an image up to 5 MiB"
        );
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            ensure!(bytes.len() + chunk.len() <= super::image::MAX_INPUT, "Choose an image up to 5 MiB");
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    bail!("Too many image redirects")
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    // Only global unicast; reject transition/tunneling and documentation ranges too.
    s[0] & 0xe000 == 0x2000
        && s[0] != 0x2002
        && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
        && !(s[0] == 0x3fff && s[1] < 0x1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_and_translation_addresses_are_not_download_targets() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
            "2002:7f00::1",
            "2001:db8::1",
            "fc00::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700::1111".parse().unwrap()));
    }
}
