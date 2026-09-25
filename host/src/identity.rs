//! This computer's long-term keys (spec §3.1): an Ed25519 signing key (its
//! identity; the computer id is derived from it) and an X25519 box key (mailbox).
//!
//! Kept in `identity.json` (0600) next to, never inside, the database: copying
//! the database must not copy the identity. Blocking `std::fs`: load once at start.

use crate::crypto::{self, b64};
use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use x25519_dalek::StaticSecret;

const FILE: &str = "identity.json";

pub struct Identity {
    pub sign: SigningKey,
    pub box_secret: StaticSecret,
}

#[derive(Serialize, Deserialize)]
struct File {
    v: u32,
    sign: String,
    #[serde(rename = "box")]
    box_key: String,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

impl Identity {
    /// Reads `identity.json`; `None` when this computer has none yet. A malformed file is an
    /// error, never silently replaced (that would make this a different computer).
    pub fn load(dir: &Path) -> Result<Option<Self>> {
        let p = path(dir);
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {}", p.display())),
        };
        let parse = || -> Result<Self> {
            let f: File = serde_json::from_str(&text)?;
            if f.v != 1 {
                bail!("unsupported version {}", f.v);
            }
            Ok(Self {
                sign: SigningKey::from_bytes(&crypto::unb64_n(&f.sign)?),
                box_secret: StaticSecret::from(crypto::unb64_n::<32>(&f.box_key)?),
            })
        };
        parse().map(Some).with_context(|| {
            format!(
                "{} is damaged; restore it, or delete it to become a new computer (every phone pairs again)",
                p.display()
            )
        })
    }

    pub fn load_or_create(dir: &Path) -> Result<Self> {
        if let Some(id) = Self::load(dir)? {
            return Ok(id);
        }
        let id = Self {
            sign: SigningKey::from_bytes(&crypto::random()),
            box_secret: StaticSecret::from(crypto::random::<32>()),
        };
        let file = File { v: 1, sign: b64(id.sign.as_bytes()), box_key: b64(id.box_secret.as_bytes()) };
        let p = path(dir);
        let tmp = dir.join(format!("{FILE}.tmp"));
        write_private(&tmp, serde_json::to_string(&file)?.as_bytes())?;
        std::fs::rename(&tmp, &p).with_context(|| format!("writing {}", p.display()))?;
        tracing::info!(computer_id = id.computer_id(), "created this computer's identity");
        Ok(id)
    }

    pub fn sign_pub(&self) -> [u8; 32] {
        self.sign.verifying_key().to_bytes()
    }

    pub fn box_pub(&self) -> [u8; 32] {
        crypto::x25519_pub(&self.box_secret)
    }

    pub fn sign_pub_b64(&self) -> String {
        b64(&self.sign_pub())
    }

    pub fn box_pub_b64(&self) -> String {
        b64(&self.box_pub())
    }

    pub fn cid_raw(&self) -> [u8; 16] {
        crypto::computer_id_raw(&self.sign_pub())
    }

    pub fn computer_id(&self) -> String {
        crypto::computer_id(&self.sign_pub())
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        crypto::sign(&self.sign, msg)
    }

    /// `Codync-Sig` header value for a request to `authority` (the lowercase Host header,
    /// with a non-default port). The spec's `sign_request(method, path_query, body)`
    /// plus the authority, which the signature covers.
    pub fn sign_request(&self, method: &str, authority: &str, path_query: &str, body: &[u8]) -> String {
        let ts = crate::store::now_ms();
        let nonce = b64(&crypto::random::<16>());
        let input = crypto::request_sig_input(method, authority, path_query, ts, &nonce, body);
        crypto::request_sig_header(&self.sign, ts, &nonce, &input)
    }
}

/// Creates `path` readable by this user only, before any secret is written to it.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(path).with_context(|| format!("creating {}", path.display()))?;
    #[cfg(unix)]
    {
        // `mode` only applies to new files: a leftover tmp keeps its old permissions.
        use std::os::unix::fs::PermissionsExt as _;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-id-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn created_once_private_and_stable() {
        let dir = temp_dir();
        assert!(Identity::load(&dir).unwrap().is_none());
        let a = Identity::load_or_create(&dir).unwrap();
        let b = Identity::load_or_create(&dir).unwrap();
        assert_eq!(a.computer_id(), b.computer_id());
        assert_eq!(a.box_pub(), b.box_pub());
        assert_eq!(a.computer_id().len(), 22);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(!dir.join(format!("{FILE}.tmp")).exists());
    }

    #[test]
    fn damaged_file_is_an_error_not_a_new_identity() {
        let dir = temp_dir();
        std::fs::write(dir.join(FILE), "{\"v\":1,\"sign\":\"nope\"}").unwrap();
        assert!(Identity::load_or_create(&dir).is_err());
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "{\"v\":1,\"sign\":\"nope\"}");
    }

    #[test]
    fn signed_requests_verify() {
        let id = Identity::load_or_create(&temp_dir()).unwrap();
        let header = id.sign_request("post", "Cloud.Example:8787", "/v1/host/register", b"{}");
        let field = |k: &str| header.split(',').find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap().to_owned();
        assert_eq!(field("kid"), id.sign_pub_b64());
        let input = crypto::request_sig_input(
            "POST",
            "cloud.example:8787",
            "/v1/host/register",
            field("ts").parse().unwrap(),
            &field("nonce"),
            b"{}",
        );
        crypto::verify(&id.sign_pub(), input.as_bytes(), &crypto::unb64_n(&field("sig")).unwrap()).unwrap();
    }
}
