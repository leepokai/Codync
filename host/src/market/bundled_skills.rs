//! Atomic installation of skill folders bundled with Codync.

use super::{Skill, save, skills, skills_dir};
use crate::LockExt;
use anyhow::{Result, bail};
use std::sync::Mutex;

pub(super) static SKILLS_LOCK: Mutex<()> = Mutex::new(());

/// Installs a namespaced bundled skill while holding the store lock for reservation and publication.
/// A failed extraction is kept out of the skill list and its incomplete folder is removed.
pub(super) fn install(
    store: &crate::store::Store,
    id: &str,
    source: &str,
    name: &str,
    description: &str,
    extract: impl FnOnce(&std::path::Path) -> Result<()>,
) -> Result<Skill> {
    let _guard = SKILLS_LOCK.locked();
    let mut all = skills(store);
    if let Some(existing) = all.iter().find(|s| s.id == id) {
        if existing.source == source {
            return Ok(existing.clone());
        }
        bail!("A different skill already uses the id {id}");
    }
    let dest = skills_dir().join(id);
    if dest.exists() {
        bail!("A skill folder already uses the id {id}");
    }
    std::fs::create_dir_all(&dest)?;
    let s = Skill {
        id: id.to_owned(),
        name: name.to_owned(),
        description: description.to_owned(),
        source: source.to_owned(),
        written_by: None,
    };
    let result = extract(&dest).and_then(|()| {
        all.push(s.clone());
        save(store, "skills", &all)
    });
    if let Err(error) = result {
        if let Err(cleanup) = std::fs::remove_dir_all(&dest) {
            tracing::warn!(%cleanup, "could not remove an incomplete bundled skill");
        }
        return Err(error);
    }
    Ok(s)
}
