//! The models an agent offers, read from a session it opens for that alone.

use super::{Started, probe_dir, start_with};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use codync_agent::acp::Incoming;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OnceCell;

type InFlightKey = (String, Option<String>);

/// Model lists being fetched, by agent and login profile: a settings screen asks for one from several places at once
/// (the model picker, Approvals), and each would otherwise start the agent. An ask after one came fetches afresh.
#[derive(Default)]
pub struct InFlight(Mutex<HashMap<InFlightKey, Slot>>);

/// One fetch's answer, for every ask that came while it ran (a failure as its text).
type Slot = Arc<OnceCell<Result<Value, String>>>;

impl InFlight {
    /// Forgets cached catalogs for `backend` after its credentials change.
    pub(crate) fn clear(&self, backend: &str) {
        self.0.locked().retain(|(known, _), _| known != backend);
    }

    pub(crate) async fn get(
        &self,
        backend: &str,
        login_profile: Option<&str>,
        fetch: impl Future<Output = Result<Value>>,
    ) -> Result<Value> {
        let key = (backend.to_owned(), login_profile.map(str::to_owned));
        let slot = self.0.locked().entry(key.clone()).or_default().clone();
        let mine = Arc::clone(&slot);
        let answer = slot
            .get_or_init(|| async move {
                let answer = fetch.await.map_err(|e| format!("{e:#}"));
                // A newer lookup for this key may already have replaced us (`clear` then `get`).
                // Removing by key would drop that one and make the next ask start again.
                self.0.locked().retain(|known, held| known != &key || !Arc::ptr_eq(held, &mine));
                answer
            })
            .await;
        answer.clone().map_err(anyhow::Error::msg)
    }
}

/// [`models`], shared with the asks for the same agent and login profile meanwhile.
pub async fn shared_models(hub: &Arc<Hub>, backend: &str, login_profile: Option<&str>) -> Result<Value> {
    if backend == "imate" {
        return super::imate::shared_models(hub, login_profile).await;
    }
    let login_profile = login_profile.filter(|s| !s.is_empty());
    Box::pin(hub.model_lists.get(backend, login_profile, Box::pin(models(&hub.store, backend, login_profile)))).await
}

/// Discover the agent's advertised models in an isolated, prompt-free session.
/// This uses the same credentials as actual bot sessions and never reads project files.
pub async fn models(store: &Store, backend: &str, login_profile: Option<&str>) -> Result<Value> {
    let login_profile = login_profile.filter(|s| !s.is_empty());
    let running = crate::agent::login_profiles::hold(store, backend, login_profile)?;
    let Started { acp, mut inbox, .. } = start_with(store, backend, login_profile, &[]).await?;
    let result = tokio::time::timeout(Duration::from_mins(1), async {
        let request = acp.request("session/new", json!({"cwd": probe_dir(), "mcpServers": []}));
        tokio::pin!(request);
        let session = loop {
            tokio::select! {
                result = &mut request => break result?,
                incoming = inbox.recv() => match incoming {
                    Some(Incoming::Request(request)) => {
                        request.refuse(codync_agent::acp::METHOD_NOT_FOUND, "not supported during model discovery").await?;
                    }
                    Some(Incoming::Notification { .. } | Incoming::Update(_)) => {}
                    Some(Incoming::Closed { .. }) | None => bail!("agent closed during model discovery"),
                }
            }
        };
        codync_agent::acp::models::catalog(&acp, &session, backend).await
    })
    .await;
    acp.kill().await;
    drop(running);
    result.context("the agent's model list timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn asks_at_once_start_the_agent_once_and_later_ones_again() {
        let lists = InFlight::default();
        let started = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |ok: bool| {
            let started = &started;
            async move {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                if ok { Ok(json!({"models": []})) } else { Err(anyhow::anyhow!("signed out")) }
            }
        };
        let (a, b) = tokio::join!(lists.get("claude", None, fetch(true)), lists.get("claude", None, fetch(true)));
        assert_eq!((a.unwrap(), b.unwrap()), (json!({"models": []}), json!({"models": []})));
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            lists.get("claude", None, fetch(false)).await.unwrap_err().to_string(),
            "signed out",
            "a refresh fetches"
        );
        assert!(lists.get("claude", None, fetch(true)).await.is_ok());
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn login_profiles_do_not_share_in_flight_slots() {
        let lists = InFlight::default();
        let started = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |label: &'static str| {
            let started = &started;
            async move {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(25)).await;
                Ok(json!({"models": [{"id": label}]}))
            }
        };
        let (d1, d2, p1_a, p1_b, p2) = tokio::join!(
            lists.get("gemini", None, fetch("default")),
            lists.get("gemini", None, fetch("default-dup")),
            lists.get("gemini", Some("pro-7"), fetch("pro")),
            lists.get("gemini", Some("pro-7"), fetch("pro-dup")),
            lists.get("gemini", Some("ultra-8"), fetch("ultra")),
        );
        assert_eq!(d1.unwrap()["models"][0]["id"], "default");
        assert_eq!(d2.unwrap()["models"][0]["id"], "default");
        assert_eq!(p1_a.unwrap()["models"][0]["id"], "pro");
        assert_eq!(p1_b.unwrap()["models"][0]["id"], "pro");
        assert_eq!(p2.unwrap()["models"][0]["id"], "ultra");
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_finished_lookup_does_not_drop_the_one_that_replaced_it() {
        let lists = Arc::new(InFlight::default());
        let (release, wait) = tokio::sync::oneshot::channel();
        let old_started = Arc::new(tokio::sync::Notify::new());
        let fresh_entered = Arc::new(tokio::sync::Notify::new());
        let running = Arc::clone(&lists);
        let ready = Arc::clone(&old_started);
        let old = tokio::spawn(async move {
            running
                .get("imate", None, async move {
                    ready.notify_one();
                    let _ = wait.await;
                    Ok(json!({"id": "old"}))
                })
                .await
        });
        old_started.notified().await;
        lists.clear("imate");
        let fetching = Arc::clone(&lists);
        let entered = Arc::clone(&fresh_entered);
        let fresh = tokio::spawn(async move {
            fetching
                .get("imate", None, async move {
                    entered.notify_one();
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(json!({"id": "fresh"}))
                })
                .await
        });
        fresh_entered.notified().await;
        release.send(()).expect("release");
        assert_eq!(old.await.expect("old task").expect("old list")["id"], "old");
        let started = std::sync::atomic::AtomicUsize::new(0);
        let again = lists
            .get("imate", None, async {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(json!({"id": "again"}))
            })
            .await
            .expect("joined");
        assert_eq!(again["id"], "fresh");
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 0, "the replacement lookup was still running");
        assert_eq!(fresh.await.expect("fresh task").expect("fresh list")["id"], "fresh");
    }

    #[tokio::test]
    async fn models_holds_and_releases_login_profile_across_errors_and_concurrent_guards() {
        use crate::agent::login_profiles::{self, Kind, Refused};

        let dir = std::env::temp_dir().join(format!("codync-models-hold-unit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let own = login_profiles::add(&store, "not-installed-test-agent", Kind::ApiKey).await.expect("own profile");
        let own_id = own["id"].as_str().expect("own id").to_owned();
        let other = login_profiles::add(&store, "claude", Kind::ApiKey).await.expect("other profile");
        let other_id = other["id"].as_str().expect("other id").to_owned();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        let hub = Hub::new(store, "models-hold-unit".into(), identity, "test".into(), 0);

        // Missing profile fails cleanly with the same message as `start_with`.
        let missing_err = models(&hub.store, "not-installed-test-agent", Some("missing-profile"))
            .await
            .expect_err("missing profile should fail");
        assert!(missing_err.to_string().contains("removed from this computer"), "{missing_err:#}");

        // Default (`None`), empty (`""` filtered by `models`), and another agent's profile hold no guard.
        assert!(login_profiles::hold(&hub.store, "not-installed-test-agent", None).expect("none").is_none());
        let cross = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&other_id)).expect("cross");
        assert!(cross.is_none(), "another agent's profile runs on default login without holding");

        // `models()` acquires `Running` in `hold`, then drops it when `start_with` fails so removal succeeds.
        let start_err = models(&hub.store, "not-installed-test-agent", Some(&own_id))
            .await
            .expect_err("unknown backend should fail in start_with");
        assert!(start_err.to_string().contains("isn't set up"), "{start_err:#}");

        // Concurrent/nested `hold` guards keep `remove` refused until every guard drops (no self-lock or early release).
        let first = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&own_id))
            .expect("first hold")
            .expect("guard");
        let second = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&own_id))
            .expect("second hold")
            .expect("guard");
        let err = login_profiles::remove(&hub, &own_id).await.expect_err("held by two guards");
        assert_eq!(err.downcast_ref::<Refused>().map(|r| r.category), Some("in_use"));
        drop(first);
        let err = login_profiles::remove(&hub, &own_id).await.expect_err("still held by second guard");
        assert_eq!(err.downcast_ref::<Refused>().map(|r| r.category), Some("in_use"));
        drop(second);

        login_profiles::remove(&hub, &own_id).await.expect("all guards dropped");
        login_profiles::remove(&hub, &other_id).await.expect("other profile was never held");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
