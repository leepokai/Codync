//! The keeper (`codync-host keeper`): on a Codync Cloud computer, a process under its own system user (`codync`) that
//! holds the computer's secrets, so its agents, which run as the host's user, can use them but never read them
//! (docs/plans/codync-cloud.md, "Agents and the host's secrets").
//!
//! - Its vault key comes from the Codync cloud at each start and stays in memory (`unlock`); its records are sealed
//!   with it on disk (`store`).
//! - The host asks for records over a unix socket anyone on the machine can reach (`socket`, the host's side in
//!   `client`). Nothing on it reveals a proxied secret: a *visible* record can be read back, a *proxied* one
//!   ([`record::Record`]) only used through a grant.
//! - A grant is a loopback URL (`proxy`): requests through it go to the record's upstream with its headers added.
//! - It updates itself when the host asks (`update`), from a signed release.
//!
//! The host itself runs as its agents' user: a secret on its way to the keeper is only as safe as that process (the
//! plan's "Limit").
//!
//! A computer of the user's own never runs it: everything that talks to it is gated on `remote::managed::is_managed`.

pub mod client;
mod protocol;
pub mod record;

/// What a secret the keeper holds reads as everywhere else (an agent's variable, a connector's saved header or token):
/// the keeper's proxy puts the real one in its place.
pub const PLACEHOLDER: &str = "codync-keeper";

#[cfg(unix)]
mod proxy;
#[cfg(unix)]
mod socket;
#[cfg(unix)]
mod store;
#[cfg(all(unix, test))]
pub(crate) mod tests;
#[cfg(unix)]
mod unlock;
#[cfg(unix)]
mod update;

// The keeper and its socket exist on unix only; elsewhere the client refuses every call.
#[cfg(unix)]
pub use paths::*;

#[cfg(unix)]
mod paths {
    /// The keeper's socket (its unit's `RuntimeDirectory`). Fixed: the host's environment, which its agents can
    /// change, never decides where its secrets go.
    pub const SOCKET: &str = "/run/codync-keeper/keeper.sock";
    pub const DIR: &str = "/var/lib/codync-keeper";
    pub const PORT: u16 = 19230;
    /// What an op that needs the vault key answers until the cloud gave it.
    pub const LOCKED: &str = "locked";
    /// The longest a grant with a time limit lives (one without lives until revoked or the keeper stops).
    pub const MAX_GRANT_SECS: u64 = 86_400;
}

/// `codync-host keeper`: runs until stopped (or replaced by an update, which systemd restarts).
#[cfg(not(unix))]
#[allow(clippy::unused_async, reason = "the same call as the unix keeper's")]
pub async fn run() -> anyhow::Result<()> {
    anyhow::bail!("the keeper runs only on a Codync Cloud computer (Linux)")
}

#[cfg(unix)]
pub use daemon::run;

#[cfg(unix)]
mod daemon {
    use super::{LOCKED, MAX_GRANT_SECS, protocol::Granted, record::Record, store};
    use crate::LockExt;
    use crate::remote::identity::Identity;
    use anyhow::{Context, Result, anyhow, bail};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use zeroize::Zeroizing;

    /// The keeper's state: its identity, its records, the vault key once it has it, and the live grants.
    pub(crate) struct Keeper {
        pub identity: Identity,
        pub secrets: store::Secrets,
        key: Mutex<Option<Zeroizing<[u8; 32]>>>,
        grants: Mutex<HashMap<String, Grant>>,
        /// The proxy's port, for grant URLs.
        pub port: u16,
        /// Random per start: grants live in memory, so a new one tells the host that every grant it held is gone.
        pub boot: String,
        /// One OAuth refresh at a time: servers that rotate refresh tokens revoke the old one on use.
        pub refresh: tokio::sync::Mutex<()>,
    }

    struct Grant {
        slot: String,
        /// `None`: until revoked (an agent's, for as long as its process runs).
        until: Option<Instant>,
    }

    impl Grant {
        fn live(&self, now: Instant) -> bool {
            self.until.is_none_or(|until| until > now)
        }
    }

    impl Keeper {
        pub fn new(identity: Identity, secrets: store::Secrets, port: u16) -> Self {
            Self {
                identity,
                secrets,
                key: Mutex::new(None),
                grants: Mutex::default(),
                port,
                boot: crate::remote::crypto::b64(&crate::remote::crypto::random::<16>()),
                refresh: tokio::sync::Mutex::new(()),
            }
        }

        pub fn unlocked(&self) -> bool {
            self.key.locked().is_some()
        }

        pub fn set_key(&self, key: Zeroizing<[u8; 32]>) {
            *self.key.locked() = Some(key);
        }

        /// The vault key, or [`LOCKED`].
        pub fn key(&self) -> Result<Zeroizing<[u8; 32]>> {
            self.key.locked().clone().ok_or_else(|| anyhow!(LOCKED))
        }

        /// The proxied record in `slot`; `None` when there is none (or the slot holds a visible one).
        pub fn record(&self, slot: &str) -> Result<Option<Record>> {
            match self.secrets.get(&*self.key()?, slot)? {
                Some((store::Kind::Proxied, plain)) => {
                    Ok(Some(serde_json::from_slice(&plain).map_err(|_| anyhow!("a damaged record"))?))
                }
                _ => Ok(None),
            }
        }

        pub fn put_record(&self, slot: &str, record: &Record) -> Result<()> {
            let plain = Zeroizing::new(serde_json::to_vec(record)?);
            self.secrets.put(&*self.key()?, slot, store::Kind::Proxied, &plain)
        }

        /// A new grant on proxied `slot` for `ttl` seconds (at most a day; none or 0: until revoked).
        pub fn grant(&self, slot: &str, ttl: Option<u64>) -> Result<Granted> {
            if self.secrets.kind(slot) != Some(store::Kind::Proxied) {
                bail!("no proxied secret in {slot}");
            }
            let token = crate::remote::crypto::b64(&crate::remote::crypto::random::<24>());
            let mut grants = self.grants.locked();
            let now = Instant::now();
            grants.retain(|_, g| g.live(now));
            let until = ttl.filter(|&t| t > 0).map(|t| now + Duration::from_secs(t.min(MAX_GRANT_SECS)));
            grants.insert(token.clone(), Grant { slot: slot.to_owned(), until });
            Ok(Granted { url: format!("http://127.0.0.1:{}/t/{token}", self.port), token })
        }

        /// The slot a live grant is for.
        pub fn granted(&self, token: &str) -> Option<String> {
            let grants = self.grants.locked();
            grants.get(token).filter(|g| g.live(Instant::now())).map(|g| g.slot.clone())
        }

        pub fn revoke(&self, token: &str) {
            self.grants.locked().remove(token);
        }

        /// Ends every grant on `slot` (it was deleted).
        pub fn revoke_slot(&self, slot: &str) {
            self.grants.locked().retain(|_, g| g.slot != slot);
        }
    }

    /// Where the keeper keeps its state and how it is reached: from its unit's environment, with defaults.
    struct Config {
        dir: PathBuf,
        socket: PathBuf,
        port: u16,
        cloud: String,
    }

    impl Config {
        fn from_env() -> Result<Self> {
            let port = match std::env::var("CODYNC_KEEPER_PORT") {
                Ok(p) => p.parse().context("CODYNC_KEEPER_PORT must be a port number")?,
                Err(_) => super::PORT,
            };
            Ok(Self {
                dir: std::env::var_os("CODYNC_KEEPER_DIR").map_or_else(|| PathBuf::from(super::DIR), PathBuf::from),
                socket: PathBuf::from(super::SOCKET),
                port,
                cloud: crate::remote::cloud::keeper_cloud_url(),
            })
        }
    }

    pub async fn run() -> Result<()> {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "codync_host=info".into()),
            )
            .log_internal_errors(false)
            .init();
        let config = Config::from_env()?;
        let (keeper, serve) = start(&config.dir, &config.socket, config.port).await?;
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            port = keeper.port,
            socket = %config.socket.display(),
            "codync keeper listening"
        );
        tokio::spawn(super::unlock::run(keeper, config.cloud));
        serve.await
    }

    /// Opens the keeper in `dir` and binds its proxy (`port`; 0: any) and its socket: it, and what serves both.
    pub(crate) async fn start(
        dir: &Path,
        socket: &Path,
        port: u16,
    ) -> Result<(Arc<Keeper>, impl Future<Output = Result<()>> + use<>)> {
        let (identity, secrets) = {
            let dir = dir.to_owned();
            tokio::task::spawn_blocking(move || {
                std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
                anyhow::Ok((Identity::load_or_create(&dir)?, store::Secrets::open(&dir)?))
            })
            .await??
        };
        // The proxy first: a second keeper fails here, before it could take the first one's socket.
        let proxy = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .with_context(|| format!("binding 127.0.0.1:{port}"))?;
        let keeper = Arc::new(Keeper::new(identity, secrets, proxy.local_addr()?.port()));
        let listener = super::socket::bind(socket)?;
        let serve = {
            let keeper = keeper.clone();
            async move {
                tokio::select! {
                    r = super::proxy::serve(keeper.clone(), proxy) => r,
                    r = super::socket::serve(keeper, listener) => r,
                }
            }
        };
        Ok((keeper, serve))
    }
}
