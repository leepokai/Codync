//! What the agents' usage readers take from the host: an agent started the way bots start it, the session kept for
//! its readings, and how each check ended, for usage analytics.

use crate::LockExt;
use crate::agent::auth;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use codync_agent::usage::{App, Stage, Started};
use futures::future::BoxFuture;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Mutex;

/// Agents whose last reading failed, so a failure is reported once, not every poll.
static FAILING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

impl App for Hub {
    fn start<'a>(
        &'a self,
        backend: &'a str,
        extra: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, anyhow::Result<Started>> {
        Box::pin(async move {
            let auth::Started { acp, inbox, init, .. } = auth::start_with(&self.store, backend, None, extra).await?;
            Ok(Started { acp, inbox, init, dir: auth::probe_dir() })
        })
    }

    fn kept_session(&self, backend: &str) -> Option<String> {
        self.store.kv_get(&session_key(backend))
    }

    fn keep_session(&self, backend: &str, id: &str) -> anyhow::Result<()> {
        self.store.kv_set(&session_key(backend), id)
    }

    /// For usage analytics (`usage_read_failed`): a failure once, not on every poll.
    fn report(&self, agent: &str, check: Result<(), Stage>) {
        let Err(stage) = check else {
            FAILING.locked().remove(agent);
            return;
        };
        tracing::debug!(agent, ?stage, "couldn't read the agent's usage");
        if FAILING.locked().insert(agent.to_owned()) {
            let properties = json!({"agent": agent, "stage": stage});
            analytics::capture(self, Event::UsageReadFailed, properties.as_object().cloned().unwrap_or_default());
        }
    }
}

fn session_key(backend: &str) -> String {
    format!("usage.session.{backend}")
}
