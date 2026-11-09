//! Lets phones find a computer in no account on the local network. Such a computer has no relay to tell its phones a
//! new address (docs/plans/optional-sign-in.md), so it advertises its addresses over mDNS. A phone trusts nothing it
//! finds this way: it dials the address and the handshake checks the computer's pinned key.
//!
//! A computer in an account advertises nothing: the relay carries its address, and multicast needs a permission on
//! some systems that it shouldn't have to ask for.

use crate::analytics::{self, DiscoveryStage};
use crate::hub::Hub;
use crate::remote::owner;
use anyhow::{Context as _, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::sync::Arc;
use std::time::Duration;

/// The service phones browse for; `NSBonjourServices` on iOS and `LocalDiscovery.kt` name it too.
const SERVICE: &str = "_codync._tcp.local.";
/// How often addresses are read again: a network change shows up within this.
const EVERY: Duration = Duration::from_mins(1);
/// One TXT entry holds 255 bytes, its key included.
const MAX_URLS_LEN: usize = 240;

/// Everyone on the network reads this, and it's for finding a computer there: Tailscale names and addresses stay out.
fn on_this_network(url: &str) -> bool {
    let host = url.split("://").nth(1).unwrap_or(url);
    let tailscale_ip = host.split('.').take(2).map(str::parse::<u8>).collect::<Vec<_>>();
    !host.contains(".ts.net") && !matches!(tailscale_ip[..], [Ok(100), Ok(64..=127)])
}

/// As many of this network's addresses as fit in one TXT entry, comma-separated, best first.
fn urls_entry(urls: &[String]) -> String {
    let mut entry = String::new();
    for url in urls.iter().filter(|url| on_this_network(url)) {
        if entry.len() + url.len() + 1 > MAX_URLS_LEN {
            break;
        }
        if !entry.is_empty() {
            entry.push(',');
        }
        entry.push_str(url);
    }
    entry
}

struct Advertising {
    daemon: ServiceDaemon,
    fullname: String,
    urls: String,
}

impl Advertising {
    fn start(hub: &Hub, urls: String) -> Result<Self> {
        let daemon = ServiceDaemon::new().context("starting mDNS")?;
        let fullname = publish(&daemon, hub, &urls)?;
        Ok(Self { daemon, fullname, urls })
    }

    fn stop(self) {
        if let Err(e) = self.daemon.unregister(&self.fullname) {
            tracing::debug!(error = format!("{e:#}"), "couldn't withdraw the mDNS service");
        }
        if let Err(e) = self.daemon.shutdown() {
            tracing::debug!(error = format!("{e:#}"), "couldn't stop mDNS");
        }
    }
}

/// Publishes (or, for the same name, replaces) this computer's record. Returns its full name.
fn publish(daemon: &ServiceDaemon, hub: &Hub, urls: &str) -> Result<String> {
    let id = hub.identity.computer_id();
    let info = ServiceInfo::new(
        SERVICE,
        &id,
        &format!("codync-{id}.local."),
        "",
        hub.port,
        &[("id", id.as_str()), ("urls", urls)][..],
    )
    .context("describing the mDNS service")?
    .enable_addr_auto();
    let fullname = info.get_fullname().to_owned();
    daemon.register(info).context("publishing the mDNS service")?;
    Ok(fullname)
}

/// Keeps the record current while this computer is in no account.
pub async fn run(hub: Arc<Hub>) {
    let mut advertising: Option<Advertising> = None;
    let mut account = hub.cloud.account_changed.subscribe();
    // Tried again each round; analytics hears of it once.
    let mut reported = false;
    let mut failed = |hub: &Hub, stage, e: &anyhow::Error| {
        tracing::info!(error = format!("{e:#}"), "not advertising on the local network");
        if !std::mem::replace(&mut reported, true) {
            analytics::discovery_failed(hub, stage);
        }
    };
    loop {
        let port = hub.port;
        let urls =
            tokio::task::spawn_blocking(move || crate::service::interface_addresses(port)).await.unwrap_or_default();
        let urls = urls_entry(&urls);
        let wanted = owner::owner(&hub.store).is_none() && !urls.is_empty();
        advertising = match (advertising, wanted) {
            (Some(running), false) => {
                running.stop();
                None
            }
            (None, false) => None,
            (None, true) => Advertising::start(&hub, urls).inspect_err(|e| failed(&hub, DiscoveryStage::Start, e)).ok(),
            (Some(mut running), true) => {
                if running.urls != urls {
                    match publish(&running.daemon, &hub, &urls) {
                        Ok(_) => running.urls = urls,
                        Err(e) => failed(&hub, DiscoveryStage::Update, &e),
                    }
                }
                Some(running)
            }
        };
        let _ = tokio::time::timeout(EVERY, account.changed()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_networks_addresses_are_advertised() {
        let urls = [
            "http://kevins-mac.tail1234.ts.net:19222".to_owned(),
            "http://100.101.102.103:19222".to_owned(),
            "http://192.168.1.5:19222".to_owned(),
            "http://100.200.1.1:19222".to_owned(),
        ];
        assert_eq!(urls_entry(&urls), "http://192.168.1.5:19222,http://100.200.1.1:19222");
    }

    #[test]
    fn the_addresses_fit_one_txt_entry() {
        let one = "http://192.168.1.5:19222".to_owned();
        assert_eq!(urls_entry(std::slice::from_ref(&one)), one);
        assert_eq!(urls_entry(&[one.clone(), "http://10.0.0.2:19222".into()]), format!("{one},http://10.0.0.2:19222"));
        let many = vec![one; 40];
        let entry = urls_entry(&many);
        assert!(entry.len() <= MAX_URLS_LEN, "{}", entry.len());
        assert!(entry.split(',').all(|url| url.ends_with(":19222")), "cut between addresses, not inside one");
    }
}
