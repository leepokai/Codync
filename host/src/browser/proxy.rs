//! Per-bot browser proxies.
//!
//! Chromium takes `--proxy-server` but cannot sign in to a proxy by itself, so every proxy is reached
//! through a short-lived HTTP proxy on loopback that adds the sign-in: HTTP upstreams get a
//! `Proxy-Authorization` header, SOCKS5 ones get the RFC 1929 handshake. The password never reaches
//! the browser's command line and is only kept in the vault.

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// A proxy as the user typed it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proxy {
    /// `socks5`, `http` or `https`.
    pub scheme: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Every form the editor accepts: `scheme://user:pass@host:port`, `scheme://host:port`,
/// `user:pass@host:port`, and `host:port:user:pass`.
pub fn parse(text: &str) -> Result<Proxy> {
    let text = text.trim();
    if text.is_empty() || text.len() > 512 {
        bail!("Enter a proxy address.");
    }
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("http".to_owned(), text),
    };
    if !matches!(scheme.as_str(), "socks5" | "http" | "https") {
        bail!("Use socks5, http or https.");
    }
    // `host:port:user:pass` has four colon-separated parts and no `@`.
    let (authority, username, password) = if let Some((auth, host)) = rest.rsplit_once('@') {
        let (user, pass) = auth.split_once(':').context("Write the sign-in as user:password.")?;
        (host, Some(percent_decode(user)), Some(percent_decode(pass)))
    } else if !rest.contains('@') && rest.matches(':').count() == 3 {
        let parts: Vec<&str> = rest.split(':').collect();
        (format!("{}:{}", parts[0], parts[1]).leak() as &str, Some(parts[2].to_owned()), Some(parts[3].to_owned()))
    } else {
        (rest, None, None)
    };
    let (host, port) = authority.rsplit_once(':').context("Give the proxy as host:port.")?;
    if host.is_empty() || host.len() > 255 || host.contains(char::is_whitespace) {
        bail!("The proxy host is invalid.");
    }
    let port: u16 = port.parse().context("The proxy port is invalid.")?;
    if port == 0 {
        bail!("The proxy port is invalid.");
    }
    if username.is_some() != password.is_some() {
        bail!("Give both the user and the password, or neither.");
    }
    Ok(Proxy { scheme, host: host.to_owned(), port, username, password })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Proxy {
    /// What the settings show: never the password.
    pub fn masked(&self) -> String {
        let sign = self.username.as_deref().map_or(String::new(), |u| format!("{u}@"));
        format!("{}://{sign}{}:{}", self.scheme, self.host, self.port)
    }
}

fn slot(bot: &str) -> String {
    format!("browser.proxy.{bot}")
}

pub fn load(store: &crate::store::Store, bot: &str) -> Result<Option<Proxy>> {
    crate::market::vault::read(store, &slot(bot))?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

pub fn save(store: &crate::store::Store, bot: &str, proxy: &Proxy) -> Result<()> {
    crate::market::vault::write(store, &slot(bot), &serde_json::to_string(proxy)?)
}

pub fn clear(store: &crate::store::Store, bot: &str) -> Result<()> {
    crate::market::vault::write(store, &slot(bot), "")
}

/// A running loopback relay; dropping the handle stops it.
pub struct Relay {
    pub port: u16,
    stop: tokio::sync::watch::Sender<bool>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// Starts the relay for `proxy` and returns the loopback port Chromium should use.
pub async fn start(proxy: Proxy) -> Result<Relay> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.context("Could not open the proxy relay")?;
    let port = listener.local_addr()?.port();
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let relay = Relay { port, stop };
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stopped.changed() => return,
                accepted = listener.accept() => {
                    if let Ok((stream, _)) = accepted {
                        tokio::spawn(serve(stream, proxy.clone()));
                    }
                }
            }
        }
    });
    Ok(relay)
}

async fn serve(mut client: TcpStream, proxy: Proxy) -> Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 4096];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 64 * 1024 {
        let read = client.read(&mut buf).await?;
        if read == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..read]);
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let first = text.lines().next().unwrap_or_default().to_owned();
    if let Some(rest) = first.strip_prefix("CONNECT ") {
        let target = rest.split_whitespace().next().context("CONNECT without a target")?;
        let mut upstream = connect(&proxy, target).await?;
        client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        return Ok(());
    }
    // Plain HTTP: the request line carries the whole address; the upstream wants origin-form.
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let url = parts.next().unwrap_or("/");
    let version = parts.next().unwrap_or("HTTP/1.1");
    let (target, origin) = match url.strip_prefix("http://") {
        Some(rest) => {
            let (host, path) = rest.split_once('/').map_or((rest, "/"), |(h, p)| (h, p));
            (host.to_owned(), format!("/{path}"))
        }
        None => (proxy.host.clone(), url.to_owned()),
    };
    let mut upstream = connect(&proxy, &target).await?;
    let mut forwarded = format!("{method} {origin} {version}\r\n");
    for line in text.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("proxy-connection:") || lower.starts_with("proxy-authorization:") {
            continue;
        }
        forwarded.push_str(line);
        forwarded.push_str("\r\n");
    }
    if let Some(auth) = basic_auth(&proxy) {
        forwarded.push_str("proxy-authorization: ");
        forwarded.push_str(&auth);
        forwarded.push_str("\r\n");
    }
    forwarded.push_str("\r\n");
    upstream.write_all(forwarded.as_bytes()).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

fn basic_auth(proxy: &Proxy) -> Option<String> {
    let user = proxy.username.as_deref()?;
    let password = proxy.password.as_deref().unwrap_or_default();
    Some(format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))))
}

/// Opens a connection to `target` (host:port) through the proxy.
async fn connect(proxy: &Proxy, target: &str) -> Result<TcpStream> {
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await.context("The proxy did not answer")?;
    if proxy.scheme == "socks5" {
        socks5(&mut stream, proxy, target).await?;
    } else {
        let mut head = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
        if let Some(auth) = basic_auth(proxy) {
            head.push_str("Proxy-Authorization: ");
            head.push_str(&auth);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).await?;
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        while !answer.windows(4).any(|w| w == b"\r\n\r\n") && answer.len() < 8 * 1024 {
            if stream.read(&mut byte).await? == 0 {
                bail!("The proxy closed the connection");
            }
            answer.push(byte[0]);
        }
        let status = String::from_utf8_lossy(&answer);
        if !status.starts_with("HTTP/1.") || !status.contains(" 200") {
            if status.contains(" 407") {
                bail!("The proxy refused the sign-in");
            }
            bail!("The proxy refused the connection");
        }
    }
    Ok(stream)
}

/// RFC 1928/1929: greeting with the sign-in method, the sign-in, then the target.
async fn socks5(stream: &mut TcpStream, proxy: &Proxy, target: &str) -> Result<()> {
    let auth = proxy.username.is_some();
    stream.write_all(&[5, if auth { 2 } else { 1 }, 0, if auth { 2 } else { 0 }][..if auth { 4 } else { 3 }]).await?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await?;
    if choice[0] != 5 || choice[1] == 0xff {
        bail!("The proxy accepted no sign-in method");
    }
    if choice[1] == 2 {
        let user = proxy.username.as_deref().unwrap_or_default().as_bytes();
        let password = proxy.password.as_deref().unwrap_or_default().as_bytes();
        let mut message = vec![1, u8::try_from(user.len()).unwrap_or(0)];
        message.extend_from_slice(user);
        message.push(u8::try_from(password.len()).unwrap_or(0));
        message.extend_from_slice(password);
        stream.write_all(&message).await?;
        let mut answer = [0u8; 2];
        stream.read_exact(&mut answer).await?;
        if answer[1] != 0 {
            bail!("The proxy refused the sign-in");
        }
    }
    let (host, port) = target.rsplit_once(':').context("The proxy target has no port")?;
    let port: u16 = port.parse().context("The proxy target port is invalid")?;
    let mut request = vec![5, 1, 0];
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        request.push(1);
        request.extend_from_slice(&ip.octets());
    } else {
        request.push(3);
        request.push(u8::try_from(host.len()).unwrap_or(0));
        request.extend_from_slice(host.as_bytes());
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;
    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await?;
    if reply[1] != 0 {
        bail!("The proxy refused the connection");
    }
    let skip = match reply[3] {
        1 => 4,
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            usize::from(len[0])
        }
        _ => 16,
    };
    let mut rest = vec![0u8; skip + 2];
    stream.read_exact(&mut rest).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_address_form_is_read() {
        let scheme = parse("socks5://ana:se%40cret@host:1080").unwrap();
        assert_eq!(
            (scheme.scheme.as_str(), scheme.username.as_deref(), scheme.password.as_deref()),
            ("socks5", Some("ana"), Some("se@cret"))
        );
        let http = parse("http://host:3128").unwrap();
        assert_eq!((http.scheme.as_str(), http.port, http.username.as_deref()), ("http", 3128, None));
        let pair = parse("ana:clave@host:1080").unwrap();
        assert_eq!((pair.username.as_deref(), pair.password.as_deref()), (Some("ana"), Some("clave")));
        let parts = parse("host:1080:ana:clave").unwrap();
        assert_eq!((parts.host.as_str(), parts.username.as_deref()), ("host", Some("ana")));
        assert_eq!(parts.masked(), "http://ana@host:1080");
    }

    #[test]
    fn bad_addresses_are_refused() {
        for bad in ["", "socks4://h:1", "host", "host:", "host:0", "http://:80", "u@host:1", "host:1:u"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
