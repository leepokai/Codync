//! Where this computer is: its key, its iroh endpoint, and the location it signs into the roster,
//! again whenever its addresses change.

use super::{Inner, locked};
use crate::roster::{Change, bounded_addr};
use crate::types::Connector;
use crate::wire;
use anyhow::{Context, Result, anyhow};
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey, Watcher};
use std::path::Path;
use std::sync::Weak;
use std::time::{Duration, Instant};

const KEY_FILE: &str = "secret.key";
/// How long addresses get to settle after a change before this computer signs where it is.
const ADDR_SETTLE: Duration = Duration::from_secs(2);

impl Inner {
    /// Writes where this computer is into the roster when it moved. Returns whether it did.
    pub(super) fn locate(&self, addr: EndpointAddr) -> Result<bool> {
        // As signed: a computer with more addresses than a location holds hasn't moved each time.
        let addr = bounded_addr(addr);
        let mut guard = locked(&self.roster);
        let Some(roster) = guard.as_mut() else { return Ok(false) };
        if roster.members().get(&self.me()).is_none_or(|m| m.addr == addr) {
            return Ok(false);
        }
        roster.sign(self.endpoint.secret_key(), wire::now_ms(), Change::Locate { addr })?;
        self.save(roster)?;
        Ok(true)
    }
}

/// Signs where this computer is again whenever its addresses change (a laptop that moved to
/// another network) and tells the others, so they find it without a restart: with the `private`
/// connector, nothing else would. The roster keeps only each computer's newest location.
pub(super) async fn follow_addr(inner: Weak<Inner>, mut addr: impl Watcher<Value = EndpointAddr>) {
    while addr.updated().await.is_ok() {
        // Addresses change in bursts (an interface comes up, then the relay answers): sign once.
        tokio::time::sleep(ADDR_SETTLE).await;
        let Some(inner) = inner.upgrade() else { return };
        match inner.locate(addr.get()) {
            Ok(true) => {
                tracing::info!("this computer moved; telling the others");
                inner.hello_all();
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(error = format!("{e:#}"), "couldn't sign where this computer is"),
        }
    }
}

pub(super) async fn bind(key: SecretKey, connector: &Connector) -> Result<Endpoint> {
    let builder = match connector {
        Connector::Public => Endpoint::builder(presets::N0),
        Connector::Relay(url) => {
            let url: RelayUrl = url.parse().context("not a relay URL")?;
            Endpoint::builder(presets::Minimal).relay_mode(RelayMode::custom([url]))
        }
        Connector::Private => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled),
    };
    Ok(builder.secret_key(key).bind().await?)
}

/// Waits briefly until the endpoint knows its own addresses, so invites and the roster carry them.
pub(super) async fn settle(endpoint: &Endpoint, connector: &Connector) {
    if *connector != Connector::Private {
        let _ = tokio::time::timeout(Duration::from_secs(5), endpoint.online()).await;
    }
    let until = Instant::now() + Duration::from_secs(3);
    while endpoint.addr().ip_addrs().next().is_none() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(super) fn load_key(dir: &Path) -> Result<SecretKey> {
    let path = dir.join(KEY_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| anyhow!("{} is damaged", path.display()))?;
            Ok(SecretKey::from_bytes(&bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            write_private(&path, &key.to_bytes())?;
            Ok(key)
        }
        Err(e) => Err(e).context("couldn't read this computer's key"),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        file.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Node;
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-network-unit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn a_computer_that_moves_tells_the_others_without_a_restart() {
        let (desk_dir, laptop_dir) = (temp_dir("desk"), temp_dir("laptop"));
        let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.expect("desk starts");
        desk.create("desk").expect("a network");
        let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.expect("laptop starts");
        laptop.join(&desk.invite().expect("an invite"), "laptop").await.expect("joined");

        // The laptop moves: an address it didn't have before.
        let moved = laptop.inner.endpoint.addr().with_ip_addr("192.0.2.7:4242".parse().expect("an address"));
        let watched = n0_watcher::Watchable::new(laptop.inner.endpoint.addr());
        tokio::spawn(follow_addr(Arc::downgrade(&laptop.inner), watched.watch()));
        watched.set(moved.clone()).expect("watched");
        let desk_sees =
            || locked(&desk.inner.roster).as_ref().and_then(|r| r.members().get(&laptop.id()).map(|m| m.addr.clone()));
        tokio::time::timeout(Duration::from_secs(10), async {
            while desk_sees() != Some(moved.clone()) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the desk hears where the laptop is now");
        for dir in [desk_dir, laptop_dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
