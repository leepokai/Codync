//! The model lists this computer read, kept per agent and login profile (`kv` `models.<backend>/<profile>`): reading
//! one starts the agent and switches it to each model (seconds), so a model panel opens on the kept one at once.

use super::models::models;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{Store, now_ms};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A kept list younger than this answers without asking the agent again.
const FRESH_FOR_MS: i64 = 10 * 60 * 1000;

/// Which list an ask takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wanted {
    /// Any kept list, at once; one older than [`FRESH_FOR_MS`] is read again in the background (a client's panel).
    Kept,
    /// A kept list younger than [`FRESH_FOR_MS`], else a new read (checking a value against it).
    Fresh,
    /// A new read (a client's refresh).
    New,
}

/// Reads an agent login's list from the agent: [`models`], or a stand-in in tests.
type Read = fn(Arc<Hub>, String, Option<String>) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

fn read_agent(
    hub: Arc<Hub>,
    backend: String,
    login_profile: Option<String>,
) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> {
    Box::pin(async move { models(&hub.store, &backend, login_profile.as_deref()).await })
}

/// [`models`] for `backend` on `login_profile` as `wanted` takes it. No kept list waits for a read; a failed read
/// keeps the old list.
pub async fn shared_models(
    hub: &Arc<Hub>,
    backend: &str,
    login_profile: Option<&str>,
    wanted: Wanted,
) -> Result<Value> {
    answer(hub, backend, login_profile, wanted, read_agent).await
}

async fn answer(
    hub: &Arc<Hub>,
    backend: &str,
    login_profile: Option<&str>,
    wanted: Wanted,
    read: Read,
) -> Result<Value> {
    let login_profile = login_profile.filter(|s| !s.is_empty());
    let kept = (wanted != Wanted::New).then(|| Kept::read(&hub.store, backend, login_profile)).flatten();
    match kept {
        Some(kept) if kept.is_fresh() => Ok(kept.list),
        Some(kept) if wanted == Wanted::Kept => {
            let (hub, backend, login_profile) = (hub.clone(), backend.to_owned(), login_profile.map(str::to_owned));
            tokio::spawn(async move { fetch(&hub, &backend, login_profile.as_deref(), true, read).await });
            Ok(kept.list)
        }
        _ => fetch(hub, backend, login_profile, false, read).await,
    }
}

/// One read, shared with the asks for the same agent and login profile meanwhile (`InFlight`), kept when it worked.
/// `background`: a kept list answered the ask, or no one asked.
async fn fetch(
    hub: &Arc<Hub>,
    backend: &str,
    login_profile: Option<&str>,
    background: bool,
    read: Read,
) -> Result<Value> {
    let read_and_keep = async {
        let started = Instant::now();
        let answer = read(hub.clone(), backend.to_owned(), login_profile.map(str::to_owned)).await;
        keep(hub, backend, login_profile, &answer);
        let properties = json!({
            "agent": backend,
            "outcome": if answer.is_ok() { "loaded" } else { "failed" },
            "background": background,
            "seconds": started.elapsed().as_secs(),
        });
        analytics::capture(hub, Event::ModelListFetched, properties.as_object().cloned().unwrap_or_default());
        answer
    };
    Box::pin(hub.model_lists.get(backend, login_profile, Box::pin(read_and_keep))).await
}

/// Keeps a list just read, and sends it to open model panels when it differs from the kept one.
fn keep(hub: &Hub, backend: &str, login_profile: Option<&str>, answer: &Result<Value>) {
    let list = match answer {
        Ok(list) => list,
        Err(e) => return tracing::info!(backend, error = format!("{e:#}"), "couldn't read the agent's model list"),
    };
    if Kept::read(&hub.store, backend, login_profile).is_none_or(|old| old.list != *list) {
        // No rev, so a stream that missed it reads the list again on the next open.
        let _ = hub
            .events
            .send(json!({"type": "models", "backend": backend, "loginProfile": login_profile, "models": list}));
    }
    Kept { at: now_ms(), list: list.clone() }.save(&hub.store, backend, login_profile);
}

/// Reads the lists of the logins the bots run on, when they aren't kept or are old, one at a time a little after the host
/// starts: the first model panel on a computer opens with its list.
pub async fn warm(hub: Arc<Hub>) {
    if codync_agent::backends::agent_checks_off() {
        return;
    }
    tokio::time::sleep(Duration::from_secs(30)).await;
    let bots = match hub.store.bots() {
        Ok(bots) => bots,
        Err(e) => return tracing::warn!(error = format!("{e:#}"), "couldn't list the bots to read their model lists"),
    };
    let logins: BTreeSet<(String, Option<String>)> = bots
        .into_iter()
        .filter(|row| !row.deleted && !row.config.is_group() && row.config.backend != "custom")
        .map(|row| {
            let profile = crate::agent::login_profiles::of_bot(&hub.store, &row.config.backend, &row.config);
            (row.config.backend.clone(), profile.map(str::to_owned))
        })
        .collect();
    for (backend, profile) in logins {
        if !Kept::read(&hub.store, &backend, profile.as_deref()).is_some_and(|kept| kept.is_fresh()) {
            let _ = fetch(&hub, &backend, profile.as_deref(), true, read_agent).await;
        }
    }
}

/// A login was just signed in (maybe to another account or plan): its kept list answers once more and is read again.
pub fn signed_in(store: &Store, backend: &str, login_profile: Option<&str>) {
    if let Some(kept) = Kept::read(store, backend, login_profile) {
        Kept { at: 0, ..kept }.save(store, backend, login_profile);
    }
}

/// A login profile was removed: its list goes with it.
pub fn forget(store: &Store, backend: &str, login_profile: &str) {
    if let Err(e) = store.kv_delete(&key(backend, Some(login_profile))) {
        tracing::warn!(backend, error = format!("{e:#}"), "couldn't forget the model list");
    }
}

fn key(backend: &str, login_profile: Option<&str>) -> String {
    format!("models.{backend}/{}", login_profile.unwrap_or_default())
}

/// A kept list and when it was read.
#[derive(Debug, PartialEq)]
struct Kept {
    at: i64,
    list: Value,
}

impl Kept {
    /// None when there isn't one or the row doesn't read as one.
    fn read(store: &Store, backend: &str, login_profile: Option<&str>) -> Option<Self> {
        let row: Value = serde_json::from_str(&store.kv_get(&key(backend, login_profile))?).ok()?;
        let at = row["at"].as_i64()?;
        let list = row.get("list").filter(|l| l["models"].is_array())?.clone();
        Some(Self { at, list })
    }

    fn save(&self, store: &Store, backend: &str, login_profile: Option<&str>) {
        let row = json!({"at": self.at, "list": self.list}).to_string();
        if let Err(e) = store.kv_set(&key(backend, login_profile), &row) {
            tracing::warn!(backend, error = format!("{e:#}"), "couldn't keep the model list");
        }
    }

    fn is_fresh(&self) -> bool {
        now_ms() - self.at <= FRESH_FOR_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    /// Answers `test.answer` (none: signed out) and counts its reads in `test.reads`.
    fn stand_in(hub: Arc<Hub>, _: String, _: Option<String>) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            hub.store.kv_set("test.reads", &(reads(&hub) + 1).to_string())?;
            match hub.store.kv_get("test.answer") {
                Some(answer) => Ok(serde_json::from_str(&answer)?),
                None => anyhow::bail!("signed out"),
            }
        })
    }

    fn reads(hub: &Hub) -> i64 {
        hub.store.kv_get("test.reads").and_then(|n| n.parse().ok()).unwrap_or(0)
    }

    fn agent_offers(hub: &Hub, id: &str) -> Value {
        let list = json!({"models": [{"id": id}]});
        hub.store.kv_set("test.answer", &list.to_string()).expect("answer");
        list
    }

    fn hub() -> (Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-model-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        (Hub::new(store, "model-cache-unit".into(), identity, "test".into(), 0), dir)
    }

    async fn ask(hub: &Arc<Hub>, wanted: Wanted) -> Result<Value> {
        answer(hub, "claude", None, wanted, stand_in).await
    }

    #[test]
    fn a_kept_list_reads_back_and_junk_does_not() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("memory store");
        assert!(Kept::read(&store, "claude", None).is_none());
        store.kv_set(&key("claude", None), r#"{"at": 5, "list": {"models": [{"id": "opus"}]}}"#).expect("set");
        assert_eq!(Kept::read(&store, "claude", None), Some(Kept { at: 5, list: json!({"models": [{"id": "opus"}]}) }));
        for junk in ["not json", r#"{"list": {"models": []}}"#, r#"{"at": 5, "list": {"models": "x"}}"#] {
            store.kv_set(&key("claude", None), junk).expect("set");
            assert!(Kept::read(&store, "claude", None).is_none(), "{junk}");
        }
        assert_ne!(key("gemini", Some("pro-7")), key("gemini", None));
    }

    #[tokio::test]
    async fn a_kept_list_answers_and_a_new_one_goes_out_only_when_it_differs() {
        let (hub, dir) = hub();
        let mut events = hub.events.subscribe();
        let opus = agent_offers(&hub, "opus");

        assert_eq!(ask(&hub, Wanted::Kept).await.expect("first read"), opus, "no kept list waits for a read");
        assert_eq!(events.try_recv().expect("event")["models"], opus);
        assert_eq!(ask(&hub, Wanted::Kept).await.expect("kept"), opus);
        assert_eq!(ask(&hub, Wanted::Fresh).await.expect("kept"), opus);
        assert_eq!(reads(&hub), 1, "a fresh kept list answers without the agent");

        assert_eq!(ask(&hub, Wanted::New).await.expect("refresh"), opus);
        assert_eq!(reads(&hub), 2, "a refresh reads");
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)), "the same list goes out once");

        let sonnet = agent_offers(&hub, "sonnet");
        assert_eq!(ask(&hub, Wanted::New).await.expect("refresh"), sonnet);
        assert_eq!(events.try_recv().expect("event")["models"], sonnet);

        hub.store.kv_delete("test.answer").expect("sign out");
        assert!(ask(&hub, Wanted::New).await.is_err());
        assert_eq!(ask(&hub, Wanted::Kept).await.expect("kept"), sonnet, "a failed read keeps the old list");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_old_list_answers_at_once_and_is_read_again_behind_it() {
        let (hub, dir) = hub();
        let mut events = hub.events.subscribe();
        let opus = json!({"models": [{"id": "opus"}]});
        Kept { at: 0, list: opus.clone() }.save(&hub.store, "claude", None);
        let sonnet = agent_offers(&hub, "sonnet");

        assert_eq!(ask(&hub, Wanted::Kept).await.expect("kept"), opus, "a panel doesn't wait");
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.expect("in time").expect("event");
        assert_eq!(event["models"], sonnet);
        assert_eq!(Kept::read(&hub.store, "claude", None).map(|k| k.list), Some(sonnet.clone()));

        signed_in(&hub.store, "claude", None);
        assert!(!Kept::read(&hub.store, "claude", None).expect("kept").is_fresh(), "signing in marks it old");
        let gpt = agent_offers(&hub, "gpt");
        assert_eq!(ask(&hub, Wanted::Fresh).await.expect("read"), gpt, "checking a value waits for a new list");

        forget(&hub.store, "claude", "");
        assert!(Kept::read(&hub.store, "claude", None).is_none(), "a removed login's list goes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
