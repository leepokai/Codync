//! A cloud computer's host and its keeper (`crate::keeper`): at start, how the keeper stands and whether the agents
//! could still use sudo (reported once, `cloud_keeper_checked`); the keeper following this host to a newer release;
//! and, from then on, noticing when the keeper started again, which took every grant with it.

use super::{is_managed, newer};
use crate::agent::bot::Cmd;
use crate::analytics::{self, KeeperState};
use crate::environment::Environment;
use crate::hub::Hub;
use crate::keeper::client;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often the host asks whether its keeper started again.
const CHECK_EVERY: Duration = Duration::from_secs(30);

/// Set once the checks below run: at start for a computer already claimed, else once its claim lands (`claim.rs`).
static STARTED: AtomicBool = AtomicBool::new(false);

/// The cloud computer's checks, once per process: at start, or, on its first boot, once its claim lands (the host
/// started before it knew it was a cloud computer).
pub async fn on_start(hub: Arc<Hub>) {
    if !is_managed(&hub.store) || STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    shield();
    let sudo = sudo_works().await;
    if sudo {
        tracing::warn!(
            "agents on this cloud computer can still use sudo, so the keeper's secrets aren't safe from them"
        );
    }
    let keeper = match client::wait(&hub).await {
        Ok(status) => status,
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "couldn't ask this computer's keeper how it stands");
            None
        }
    };
    let state = match &keeper {
        Some(status) if status.unlocked => KeeperState::Unlocked,
        Some(_) => KeeperState::Locked,
        None => KeeperState::Missing,
    };
    analytics::cloud_keeper_checked(&hub, state, sudo);
    if let Some(status) = &keeper {
        follow_host(&hub, &status.version).await;
    }
    watch(&hub, keeper.map(|status| status.boot)).await;
}

/// Keeps this process's memory from its agents, which run as the same user: not dumpable, a process of the same user
/// can't attach to it or read `/proc/<pid>/mem`, whatever the kernel's Yama setting (some providers' kernels have none).
/// It holds a secret only on its way to the keeper. This guards the running process only: an agent can still change
/// what runs as the host next (docs/plans/codync-cloud.md, "Agents and the host's secrets"). Agents start dumpable again
/// (`exec`).
fn shield() {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: PR_SET_DUMPABLE takes plain integers and touches no memory of ours.
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
            tracing::warn!(error = %std::io::Error::last_os_error(), "couldn't keep this cloud computer's host from its agents");
        }
    }
}

/// `sudo -n true` works: the agents, which run as this user, could read the keeper's secrets.
async fn sudo_works() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let sudo = tokio::process::Command::new("sudo")
        .args(["-n", "true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    matches!(tokio::time::timeout(Duration::from_secs(10), sudo).await, Ok(Ok(status)) if status.success())
}

/// The keeper updates only when asked: a host newer than it (a release the cloud ordered) has it follow.
async fn follow_host(hub: &Arc<Hub>, keeper: &str) {
    let host = env!("CARGO_PKG_VERSION");
    if Environment::current() != Environment::Main || !newer(host, keeper) {
        return;
    }
    let asked = hub.clone();
    match tokio::task::spawn_blocking(move || client::update(&asked.store, host)).await {
        Ok(Ok(())) => tracing::info!(from = keeper, to = host, "updated this computer's keeper"),
        Ok(Err(e)) => tracing::warn!(error = format!("{e:#}"), "couldn't update this computer's keeper"),
        Err(e) => tracing::warn!(error = %e, "couldn't update this computer's keeper"),
    }
}

/// Grants live in the keeper's memory: when it starts again (an update, a crash), the agents started with some can
/// no longer reach their providers, so they start again too (`Cmd::KeeperRestarted`).
async fn watch(hub: &Arc<Hub>, mut boot: Option<String>) {
    loop {
        tokio::time::sleep(CHECK_EVERY).await;
        if restarted(hub, &mut boot).await {
            tracing::info!("this computer's keeper started again; restarting the agents that used its grants");
            restart_agents(hub);
        }
    }
}

/// Whether the keeper started again since `boot`, its boot id when last asked (kept up to date here).
async fn restarted(hub: &Arc<Hub>, boot: &mut Option<String>) -> bool {
    let asked = hub.clone();
    let Ok(Ok(Some(status))) = tokio::task::spawn_blocking(move || client::status(&asked.store)).await else {
        return false;
    };
    let again = boot.as_ref().is_some_and(|b| *b != status.boot);
    *boot = Some(status.boot);
    again
}

fn restart_agents(hub: &Hub) {
    let bots = match hub.store.bots() {
        Ok(bots) => bots,
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "couldn't list the bots to restart their agents");
            return;
        }
    };
    for row in bots.into_iter().filter(|row| !row.deleted) {
        // A bot without a running actor has no agent to restart.
        let _ = hub.send_cmd(&row.config.id, Cmd::KeeperRestarted);
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_keeper_that_started_again_is_noticed_once() {
        let dir = std::env::temp_dir().join(format!("codync-kc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(&dir.join("t.db")).unwrap();
        let first = crate::keeper::tests::test_keeper();
        first.manage(&store);
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "test".into(), identity, "test".into(), 0);
        let mut boot = None;
        assert!(!restarted(&hub, &mut boot).await, "the first look only learns its boot");
        assert!(!restarted(&hub, &mut boot).await);
        // Another keeper on the socket: a new start, which forgot every grant.
        crate::keeper::tests::test_keeper().manage(&hub.store);
        assert!(restarted(&hub, &mut boot).await);
        assert!(!restarted(&hub, &mut boot).await);
        let _ = std::fs::remove_dir_all(dir);
    }
}
