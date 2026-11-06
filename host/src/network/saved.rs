//! The networks this computer is in, as kept between runs: kv `networks`, each with its node's
//! folder in the data directory.

use crate::store::Store;
use anyhow::{Result, bail};
use codync_network::{Connector, Rule};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KV: &str = "networks";
/// The folder of the network a computer joined before it could be in several, and its connector.
const FIRST: &str = "network";
const FIRST_CONNECTOR: &str = "network_connector";
/// What a network is called when nobody named it.
pub const DEFAULT_NAME: &str = "home";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// Its node's folder, and its id in the API.
    pub id: String,
    pub name: String,
    pub connector: Connector,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

pub fn dir(id: &str) -> PathBuf {
    crate::service::data_dir().join(id)
}

/// The networks this computer is in.
pub fn load(store: &Store) -> Vec<Saved> {
    let Some(text) = store.kv_get(KV) else { return Vec::new() };
    serde_json::from_str(&text).unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't read the saved networks");
        Vec::new()
    })
}

pub fn save(store: &Store, all: &[Saved]) -> Result<()> {
    store.kv_set(KV, &serde_json::to_string(all)?)
}

/// Keeps the one network a computer was in before it could be in several (its folder in `data`,
/// and kv `network_connector`) as `home`. Once, before the first `load`.
pub fn migrate(store: &Store, data: &Path) -> Result<()> {
    if store.kv_get(KV).is_some() || !data.join(FIRST).join("network.json").exists() {
        return Ok(());
    }
    let connector = store.kv_get(FIRST_CONNECTOR).map_or(Ok(Connector::Public), |c| c.parse())?;
    save(store, &[Saved { id: FIRST.into(), name: DEFAULT_NAME.into(), connector, rules: Vec::new() }])?;
    store.kv_delete(FIRST_CONNECTOR)
}

/// A folder no network uses yet, and no network's name (`Network::slot` takes either):
/// `network`, then `network-<random>`.
pub fn new_id(all: &[Saved]) -> String {
    let free = |id: &str| all.iter().all(|s| s.id != id && !s.name.eq_ignore_ascii_case(id)) && !dir(id).exists();
    if free(FIRST) {
        return FIRST.into();
    }
    loop {
        let id = format!("{FIRST}-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        if free(&id) {
            return id;
        }
    }
}

/// `name` as a network name (like a computer's: no spaces, `@` or `.`, which addresses use),
/// numbered when another network here has it, or has it as its id (`home-2`).
pub fn free_name(all: &[Saved], name: &str) -> Result<String> {
    let name = super::handle(name.trim());
    if name.len() > 40 {
        bail!("a network's name is at most 40 characters");
    }
    let taken = |n: &str| all.iter().any(|s| s.name.eq_ignore_ascii_case(n) || s.id == n);
    // Each network takes two: its name and its id.
    Ok(super::handles::numbered(&name, 2 * all.len(), |n| !taken(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(id: &str, name: &str) -> Saved {
        Saved { id: id.into(), name: name.into(), connector: Connector::Public, rules: Vec::new() }
    }

    #[test]
    fn a_taken_network_name_gets_a_number() {
        let all = [saved("network", "home"), saved("network-1", "home-2")];
        assert_eq!(free_name(&all, "Work").unwrap(), "work");
        assert_eq!(free_name(&all, "home").unwrap(), "home-3");
        assert_eq!(free_name(&all, "my.lab").unwrap(), "my-lab", "no dots: addresses use them");
        assert_eq!(free_name(&all, "network").unwrap(), "network-2", "another network's id");
    }

    #[test]
    fn the_network_a_computer_was_in_before_becomes_home() {
        let data = std::env::temp_dir().join(format!("codync-saved-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(data.join(FIRST)).unwrap();
        let store = Store::open(&data.join("test.db")).unwrap();
        migrate(&store, &data).unwrap();
        assert_eq!(load(&store), [], "no folder with a network in it yet");

        std::fs::write(data.join(FIRST).join("network.json"), "{}").unwrap();
        store.kv_set(FIRST_CONNECTOR, "private").unwrap();
        migrate(&store, &data).unwrap();
        let home = Saved { id: FIRST.into(), name: "home".into(), connector: Connector::Private, rules: Vec::new() };
        assert_eq!(load(&store), std::slice::from_ref(&home));
        assert_eq!(store.kv_get(FIRST_CONNECTOR), None, "kept with the network now");
        // Once: what the user changes later stays.
        save(&store, &[]).unwrap();
        migrate(&store, &data).unwrap();
        assert_eq!(load(&store), []);
        assert!(new_id(&[home]).starts_with("network-"), "the folder is taken");
        let _ = std::fs::remove_dir_all(data);
    }
}
