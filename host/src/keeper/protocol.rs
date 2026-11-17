//! The keeper's socket protocol, which `socket` serves and `client` speaks: one JSON line each way, the answer
//! `{"ok":…}` or `{"error":"…"}`.

use super::record::{Keep, Record};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Request {
    /// The keeper's signing key (`crypto::b64`), which the Codync cloud trusts with the vault key.
    Pub,
    Status,
    Put {
        slot: String,
        value: String,
    },
    PutProxied {
        slot: String,
        record: Box<Record>,
        #[serde(default)]
        keep: Keep,
    },
    Delete {
        slot: String,
    },
    Has {
        slot: String,
    },
    /// Whether proxied `slot` still holds a sign-in: the keeper drops one it can't renew.
    SignedIn {
        slot: String,
    },
    /// The slots starting with `prefix`.
    Names {
        #[serde(default)]
        prefix: String,
    },
    /// A visible record's value; a proxied one is refused.
    Reveal {
        slot: String,
    },
    Grant {
        slot: String,
        ttl_secs: Option<u64>,
    },
    Revoke {
        token: String,
    },
    /// Installs release `version` over this binary and exits (systemd starts it again).
    Update {
        version: String,
    },
}

/// What the keeper says about itself.
#[derive(Serialize, Deserialize)]
pub struct Status {
    /// It has its vault key.
    pub unlocked: bool,
    pub version: String,
    /// Random per start of the keeper: a new one means every grant from before is gone.
    pub boot: String,
}

/// A grant on a proxied slot: its loopback URL, and the token in it, which revokes it.
#[derive(Serialize, Deserialize)]
pub struct Granted {
    pub url: String,
    pub token: String,
}
