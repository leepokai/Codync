//! Which agents' usage this computer reads and shows.
//!
//! An agent shows when a bot here runs it, as its own agent or a fallback, unless the user said
//! otherwise in the Usage screen. Reading follows the same list: an agent nobody runs or asked
//! for is never started and its saved login never read. One hidden while a bot still runs it keeps
//! being read, because background work and fallbacks go by its limits.

use super::{
    Window, amp, antigravity, auggie, claude, codex, copilot, cursor, grok, hermes, junie, kilo, kimi, kiro, omp,
    opencode, vibe,
};
use crate::LockExt;
use crate::agent::backends;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result};
use futures::future::BoxFuture;
use serde::Serialize;
use std::collections::HashSet;

/// An agent the apps can list in the Usage screen's chooser.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Service {
    pub id: &'static str,
    pub name: &'static str,
    /// The agent's ACP registry id, which its logo goes by. Without one, its `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<&'static str>,
    pub shown: bool,
}

pub(super) struct Reader {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) source: &'static str,
    /// Something to read without the agent's CLI: an app's saved login or quota file.
    pub(super) found_without_cli: Option<fn() -> bool>,
    pub(super) read: for<'a> fn(&'a Hub) -> BoxFuture<'a, Option<Vec<Window>>>,
}

fn blocking(read: fn() -> Option<Vec<Window>>) -> BoxFuture<'static, Option<Vec<Window>>> {
    Box::pin(async move { tokio::task::spawn_blocking(read).await.ok().flatten() })
}

/// Every agent the host can read, in the order the chooser lists them.
pub(super) const READERS: &[Reader] = &[
    Reader {
        id: "claude",
        name: "Claude",
        source: "claude",
        found_without_cli: None,
        read: |_| Box::pin(claude::collect()),
    },
    Reader {
        id: "codex",
        name: "Codex",
        source: "Codex CLI",
        found_without_cli: None,
        read: |_| Box::pin(codex::collect()),
    },
    Reader {
        id: "antigravity",
        name: "Google Antigravity",
        source: "antigravity",
        found_without_cli: None,
        read: |hub| Box::pin(antigravity::collect(hub)),
    },
    Reader {
        id: "cursor",
        name: "Cursor",
        source: "Cursor",
        found_without_cli: Some(cursor::login_saved),
        read: |_| Box::pin(cursor::collect()),
    },
    Reader {
        id: "copilot",
        name: "GitHub Copilot",
        source: "Copilot CLI",
        found_without_cli: None,
        read: |hub| Box::pin(copilot::collect(hub)),
    },
    Reader {
        id: "grok",
        name: "Grok Build",
        source: "Grok CLI",
        found_without_cli: None,
        read: |hub| Box::pin(grok::collect(hub)),
    },
    Reader { id: "amp", name: "Amp", source: "amp usage", found_without_cli: None, read: |_| Box::pin(amp::collect()) },
    Reader {
        id: "auggie",
        name: "Auggie",
        source: "auggie account status",
        found_without_cli: None,
        read: |hub| Box::pin(auggie::collect(hub)),
    },
    Reader {
        id: "hermes",
        name: "Hermes Agent",
        source: "hermes usage",
        found_without_cli: None,
        read: |hub| Box::pin(hermes::collect(hub)),
    },
    Reader {
        id: "junie",
        name: "Junie",
        source: "JetBrains IDE",
        found_without_cli: Some(junie::quota_saved),
        read: |_| blocking(junie::collect),
    },
    Reader {
        id: "kilo",
        name: "Kilo",
        source: "kilo profile",
        found_without_cli: None,
        read: |hub| Box::pin(kilo::collect(hub)),
    },
    Reader {
        id: "kimi",
        name: "Kimi Code",
        source: "kimi",
        found_without_cli: None,
        read: |_| blocking(kimi::collect),
    },
    Reader {
        id: "kiro",
        name: "Kiro CLI",
        source: "kiro-cli",
        found_without_cli: None,
        read: |_| Box::pin(kiro::collect()),
    },
    Reader {
        id: "vibe",
        name: "Mistral Vibe",
        source: "vibe logs",
        found_without_cli: None,
        read: |_| blocking(vibe::collect),
    },
    Reader {
        id: "omp",
        name: "oh-my-pi",
        source: "omp usage",
        found_without_cli: None,
        read: |_| Box::pin(omp::collect()),
    },
    Reader {
        id: "opencode",
        name: "OpenCode",
        source: "opencode database",
        found_without_cli: None,
        read: |_| blocking(opencode::collect),
    },
];

/// The order the user arranged the agents in, as ids.
const ORDER_KEY: &str = "usage.order";

fn arranged(store: &Store) -> Vec<String> {
    let Some(saved) = store.kv_get(ORDER_KEY) else { return Vec::new() };
    serde_json::from_str(&saved).unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't read the usage order");
        Vec::new()
    })
}

fn reader(id: &str) -> Result<&'static Reader> {
    READERS.iter().find(|reader| reader.id == id).context("unknown usage service")
}

fn choice_key(id: &str) -> String {
    format!("usage.shown.{id}")
}

/// What the user chose for `id`, when they did.
fn chosen(store: &Store, id: &str) -> Option<bool> {
    store.kv_get(&choice_key(id)).map(|value| value == "1")
}

/// `id`'s ACP registry entry, when it has one.
pub(super) fn registry(id: &str) -> Option<&'static str> {
    backends::harness(id).and_then(|harness| harness.registry)
}

/// A bot here runs this agent, as its own or a fallback.
pub(super) fn runs(store: &Store, backend: &str) -> bool {
    backends::harness_of(backend).is_some_and(|harness| used_agents(store).contains(harness.id))
}

/// The agents this computer's bots run, by their readers' ids.
fn used_agents(store: &Store) -> HashSet<&'static str> {
    let bots = store.bots().unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't list the bots for their usage");
        Vec::new()
    });
    bots.iter()
        .filter(|bot| !bot.deleted)
        .flat_map(|bot| {
            let fallbacks = bot.config.fallbacks.iter().map(|fallback| fallback.backend.as_str());
            std::iter::once(bot.config.backend.as_str()).chain(fallbacks)
        })
        .filter_map(|backend| backends::harness_of(backend).map(|harness| harness.id))
        .collect()
}

/// Whether the agent gets read: a bot runs it, or the user asked for it.
fn wanted(id: &str, used: &HashSet<&str>, store: &Store) -> bool {
    used.contains(id) || chosen(store, id) == Some(true)
}

/// Whether the chooser lists it: there is something on this computer to read it from.
fn listed(reader: &Reader, used: &HashSet<&str>, store: &Store, held: &HashSet<&str>) -> bool {
    wanted(reader.id, used, store)
        || held.contains(reader.id)
        || backends::installed(reader.id)
        || reader.found_without_cli.is_some_and(|found| found())
}

/// Every reader this computer should run.
pub(super) fn wanted_readers(store: &Store) -> Vec<&'static Reader> {
    let used = used_agents(store);
    READERS.iter().filter(|reader| wanted(reader.id, &used, store)).collect()
}

/// The wanted readers with no reading yet.
pub(super) fn unread_readers(hub: &Hub) -> Vec<&'static Reader> {
    let held = held_readings(hub);
    wanted_readers(&hub.store).into_iter().filter(|reader| !held.contains(reader.id)).collect()
}

fn held_readings(hub: &Hub) -> HashSet<&'static str> {
    hub.usage.locked().providers.iter().map(|provider| provider.id).collect()
}

/// Brings the chooser's list up to date and drops readings nobody wants any more. One at a time: a sync that read the
/// settings before a change can't land after the sync that change made.
pub(super) fn sync(hub: &Hub) {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = ONE_AT_A_TIME.locked();
    let store = &hub.store;
    let used = used_agents(store);
    // The database and the disk stay outside the usage lock: a reading landing meanwhile is kept.
    let held = held_readings(hub);
    let mut services: Vec<Service> = READERS
        .iter()
        .filter(|reader| listed(reader, &used, store, &held))
        .map(|reader| Service {
            id: reader.id,
            name: reader.name,
            registry: registry(reader.id),
            shown: chosen(store, reader.id).unwrap_or_else(|| used.contains(reader.id)),
        })
        .collect();
    // Stable: an agent the order doesn't name follows the others, in the readers' order.
    let order = arranged(store);
    services.sort_by_key(|service| order.iter().position(|id| id == service.id).unwrap_or(usize::MAX));
    let keep: HashSet<&str> = READERS.iter().map(|reader| reader.id).filter(|id| wanted(id, &used, store)).collect();
    hub.update_usage(|usage| {
        usage.services = services;
        usage.providers.retain(|provider| keep.contains(provider.id));
    });
}

fn changed(hub: &Hub) {
    sync(hub);
    if !unread_readers(hub).is_empty() {
        hub.usage_wake.notify_one();
    }
}

/// A bot was added, removed or moved to other agents.
pub fn bots_changed(hub: &Hub) {
    changed(hub);
}

/// The user's choice for one agent.
pub fn set_shown(hub: &Hub, id: &str, shown: bool) -> Result<()> {
    hub.store.kv_set(&choice_key(reader(id)?.id), if shown { "1" } else { "0" })?;
    changed(hub);
    Ok(())
}

/// The user's order for the agents, which every Usage screen follows.
pub fn set_order(hub: &Hub, ids: &[String]) -> Result<()> {
    for id in ids {
        reader(id)?;
    }
    hub.store.kv_set(ORDER_KEY, &serde_json::to_string(ids)?)?;
    sync(hub);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Provider, near_limit};
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Fallback};

    fn hub() -> std::sync::Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let identity = Identity::load_or_create(&dir).unwrap();
        Hub::new(store, "h".into(), identity, "t".into(), 0)
    }

    fn bot(id: &str, backend: &str, fallback: Option<&str>) -> BotConfig {
        let fallbacks = fallback.map(|backend| Fallback { backend: backend.into(), ..Fallback::default() });
        BotConfig {
            id: id.into(),
            backend: backend.into(),
            fallbacks: fallbacks.into_iter().collect(),
            ..serde_json::from_value(serde_json::json!({"id": id, "name": id})).unwrap()
        }
    }

    fn reading(id: &'static str, percent: f64) -> Provider {
        Provider::new(id, id, "test", vec![Window::new("weekly", "Weekly", percent, None)])
    }

    fn shown(hub: &Hub, id: &str) -> Option<bool> {
        hub.usage.locked().services.iter().find(|service| service.id == id).map(|service| service.shown)
    }

    #[test]
    fn the_users_order_leads_the_list_and_the_readings() {
        let hub = hub();
        hub.store.save_bot(&bot("a", "claude", Some("codex"))).unwrap();
        hub.update_usage(|usage| usage.providers = vec![reading("claude", 1.0), reading("codex", 2.0)]);
        sync(&hub);
        let ids = |usage: &super::super::Usage| usage.services.iter().map(|s| s.id).take(2).collect::<Vec<_>>();
        let providers = |usage: &super::super::Usage| usage.providers.iter().map(|p| p.id).collect::<Vec<_>>();
        assert_eq!(ids(&hub.usage.locked().visible()), ["claude", "codex"]);

        set_order(&hub, &["codex".to_owned()]).unwrap();
        let visible = hub.usage.locked().visible();
        assert_eq!(ids(&visible), ["codex", "claude"], "one it doesn't name follows");
        assert_eq!(providers(&visible), ["codex", "claude"]);

        assert!(set_order(&hub, &["not-an-agent".to_owned()]).is_err());
        assert_eq!(ids(&hub.usage.locked().visible()), ["codex", "claude"], "a refused order changes nothing");
    }

    #[test]
    fn only_the_agents_bots_run_are_shown_and_read_by_default() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "antigravity", Some("codex"))).unwrap();
        // A registry id is the same agent.
        hub.store.save_bot(&bot("b2", "claude-acp", None)).unwrap();
        hub.store.save_bot(&bot("gone", "cursor", None)).unwrap();
        hub.store.delete_bot("gone").unwrap();
        sync(&hub);
        assert_eq!(shown(&hub, "antigravity"), Some(true));
        assert_eq!(shown(&hub, "codex"), Some(true), "a fallback counts");
        assert_eq!(shown(&hub, "claude"), Some(true));
        assert_ne!(shown(&hub, "cursor"), Some(true), "no bot runs Cursor");
        let read: Vec<_> = wanted_readers(&hub.store).iter().map(|reader| reader.id).collect();
        assert_eq!(read, ["claude", "codex", "antigravity"], "Cursor's saved login is left alone");
    }

    #[test]
    fn hiding_an_agent_a_bot_runs_keeps_reading_it_for_its_limits() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "codex", None)).unwrap();
        hub.update_usage(|usage| usage.providers = vec![reading("codex", 90.0)]);
        set_shown(&hub, "codex", false).unwrap();
        assert_eq!(shown(&hub, "codex"), Some(false));
        assert_eq!(hub.usage.locked().visible().providers, []);
        assert!(near_limit(&hub, "codex"), "background work still goes by its limits");
        assert_eq!(wanted_readers(&hub.store).len(), 1);
    }

    #[test]
    fn turning_an_agent_on_reads_it_without_a_bot_and_off_drops_its_reading() {
        let hub = hub();
        set_shown(&hub, "cursor", true).unwrap();
        assert_eq!(shown(&hub, "cursor"), Some(true));
        assert_eq!(unread_readers(&hub).len(), 1, "it gets read right away");
        hub.update_usage(|usage| usage.providers.push(reading("cursor", 10.0)));
        assert_eq!(hub.usage.locked().visible().providers.len(), 1);
        set_shown(&hub, "cursor", false).unwrap();
        assert_eq!(hub.usage.locked().providers, [], "no stale reading stays behind");
        assert!(wanted_readers(&hub.store).is_empty());
    }

    #[test]
    fn a_bot_runs_an_agent_by_either_of_its_ids() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "grok-build", Some("antigravity"))).unwrap();
        assert!(runs(&hub.store, "grok"));
        assert!(runs(&hub.store, "antigravity-acp"), "a fallback counts");
        assert!(!runs(&hub.store, "cursor"));
    }

    /// A reader whose id is no harness's is unreachable: no bot's `backend` finds it, and its CLI is never seen.
    #[test]
    fn every_reader_is_a_harness() {
        assert_eq!(READERS.iter().map(|reader| reader.id).find(|id| backends::harness(id).is_none()), None);
    }

    #[test]
    fn only_a_known_agent_can_be_chosen() {
        let hub = hub();
        assert!(set_shown(&hub, "not-an-agent", true).is_err());
        assert_eq!(hub.store.kv_get(&choice_key("not-an-agent")), None);
    }
}
