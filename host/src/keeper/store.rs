//! The keeper's records, `secrets.json` in its state folder (0600, written whole and renamed into place):
//! `{"v":1,"items":{"<slot>":{"kind":"visible"|"proxied","sealed":"…"}}}`. Each is sealed with the vault key
//! (`market::vault::seal`) and bound to its slot and kind, so a record moved to another slot or kind won't open.

use crate::LockExt;
use crate::market::vault;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroizing;

const FILE: &str = "secrets.json";

/// Whether a process may read the secret (`reveal`), or only use it through a grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Visible,
    Proxied,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Proxied => "proxied",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Item {
    kind: Kind,
    sealed: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    v: u32,
    items: BTreeMap<String, Item>,
}

pub struct Secrets {
    path: PathBuf,
    items: Mutex<BTreeMap<String, Item>>,
}

/// What a record is sealed to.
fn aad(slot: &str, kind: Kind) -> String {
    format!("codync/keeper/v1|{slot}|{}", kind.name())
}

impl Secrets {
    /// The records in `dir`; none yet when it has no file. A damaged file is an error, never replaced.
    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        let items = match std::fs::read(&path) {
            Ok(bytes) => {
                let file: File =
                    serde_json::from_slice(&bytes).with_context(|| format!("reading {}", path.display()))?;
                if file.v != 1 {
                    bail!("{} has unsupported version {}", path.display(), file.v);
                }
                file.items
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Ok(Self { path, items: Mutex::new(items) })
    }

    pub fn kind(&self, slot: &str) -> Option<Kind> {
        self.items.locked().get(slot).map(|i| i.kind)
    }

    /// Slots starting with `prefix`.
    pub fn names(&self, prefix: &str) -> Vec<String> {
        self.items.locked().keys().filter(|k| k.starts_with(prefix)).cloned().collect()
    }

    /// The record in `slot`, opened with `key`.
    pub fn get(&self, key: &[u8], slot: &str) -> Result<Option<(Kind, Zeroizing<Vec<u8>>)>> {
        let Some(item) = self.items.locked().get(slot).cloned() else { return Ok(None) };
        let plain = vault::open(key, aad(slot, item.kind).as_bytes(), &item.sealed)?;
        Ok(Some((item.kind, plain)))
    }

    pub fn put(&self, key: &[u8], slot: &str, kind: Kind, plain: &[u8]) -> Result<()> {
        let sealed = vault::seal(key, aad(slot, kind).as_bytes(), plain)?;
        let mut items = self.items.locked();
        let mut next = items.clone();
        next.insert(slot.to_owned(), Item { kind, sealed });
        self.save(&next)?;
        *items = next;
        Ok(())
    }

    /// `false` when there was nothing in `slot`.
    pub fn delete(&self, slot: &str) -> Result<bool> {
        let mut items = self.items.locked();
        if !items.contains_key(slot) {
            return Ok(false);
        }
        let mut next = items.clone();
        next.remove(slot);
        self.save(&next)?;
        *items = next;
        Ok(true)
    }

    /// Writes the whole file under the items lock, so saves can't interleave.
    fn save(&self, items: &BTreeMap<String, Item>) -> Result<()> {
        let bytes = serde_json::to_vec(&File { v: 1, items: items.clone() })?;
        let tmp = self.path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        crate::remote::identity::write_private(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.path).with_context(|| format!("writing {}", self.path.display()))
    }
}
