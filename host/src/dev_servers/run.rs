//! Starting and stopping: a server is the host's own child in a tree of its own.

use super::{
    Leader, Live, Run, STOP_GRACE, STOP_WAIT, Saved, Status, detected, forget, has_room, logs, no_room, platform,
    report, same_server, save,
};
use crate::LockExt;
use crate::agent::process::{self, Process};
use crate::analytics::Event;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Context, Result, bail};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

const EXIT_POLL: Duration = Duration::from_millis(200);

/// Adds a new server and starts it, or returns the same one a parallel start already added, saying so with true.
/// `approved`: someone allowed it on a card.
pub(super) async fn create(
    hub: &Arc<Hub>,
    bot: &str,
    name: String,
    command: String,
    folder: String,
    approved: bool,
) -> Result<(String, bool)> {
    let id = uuid::Uuid::now_v7().to_string();
    let saved = Saved {
        id: id.clone(),
        bot: bot.to_owned(),
        name,
        command,
        folder,
        created_at: now_ms(),
        wanted: true,
        approved,
        leader: None,
        boot_ms: None,
    };
    {
        let mut live = hub.dev_servers.live.locked();
        // A parallel start of the same thing got here first, while this one waited on its card.
        if let Some(same) = same_server(&live, bot, &saved.folder, &saved.command) {
            return Ok((same, true));
        }
        if !has_room(&live, bot) {
            drop(live);
            return Err(no_room(hub, bot));
        }
        save(&hub.store, &saved);
        let mut server = Live::stopped(saved.clone());
        server.begin(now_ms());
        live.insert(id.clone(), server);
    }
    run_begun(hub, saved).await.inspect_err(|_| report(hub, Event::DevServerStarted, "outcome", "failed"))?;
    Ok((id, false))
}

/// Starts a stopped server: the host's own child in a tree of its own, its output going to its log.
pub(super) async fn launch(hub: &Arc<Hub>, id: &str) -> Result<()> {
    let saved = {
        let mut live = hub.dev_servers.live.locked();
        let entry = live.get_mut(id).context("unknown dev server")?;
        if entry.run.is_some() || entry.status == Status::Starting {
            // On its way already, and wanted after all.
            entry.stop_asked = false;
            return Ok(());
        }
        let bot = entry.saved.bot.clone();
        if !has_room(&live, &bot) {
            drop(live);
            return Err(no_room(hub, &bot));
        }
        let entry = live.get_mut(id).expect("found above, under this same lock");
        entry.begin(now_ms());
        entry.saved.clone()
    };
    run_begun(hub, saved).await
}

/// Runs a server just marked as starting.
async fn run_begun(hub: &Arc<Hub>, saved: Saved) -> Result<()> {
    hub.emit_bot(&saved.bot);
    // A task of its own: a request that goes away (a closed connection, a cancelled tool call) can't abandon it halfway.
    tokio::spawn(start(hub.clone(), saved)).await?
}

async fn start(hub: Arc<Hub>, saved: Saved) -> Result<()> {
    let spawned = spawn(&hub, &saved).await;
    match &spawned {
        Ok(()) => hub.dev_servers.wake.notify_one(),
        Err(error) => {
            let known = hub.dev_servers.live.locked().get_mut(&saved.id).map(|entry| {
                entry.status = Status::Failed;
                entry.saved.wanted = false;
                save(&hub.store, &entry.saved);
            });
            if known.is_some() {
                logs::note(&saved.id, &format!("{error:#}"));
            }
            hub.emit_bot(&saved.bot);
        }
    }
    spawned
}

async fn spawn(hub: &Arc<Hub>, saved: &Saved) -> Result<()> {
    // Not `shell::exec`: a command line may start with variables or chain several commands. The login shell's
    // environment, never the agent's: that one can hold its API keys.
    let mut command = crate::shell::line(&saved.command).tokio();
    command
        .current_dir(&saved.folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process::own_tree(&mut command);
    let mut child = command.spawn().with_context(|| format!("starting `{}`", saved.command))?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let process = Arc::new(tokio::sync::Mutex::new(Process::new(child)?));
    logs::capture(&saved.id, stdout, stderr).await?;
    let (leader, boot_ms) = tokio::task::spawn_blocking(move || {
        let leader = pid.and_then(|pid| Some(Leader { pid, start: platform::process_start(pid)? }));
        (leader, platform::boot_ms())
    })
    .await?;
    let (stop, stop_seen) = watch::channel(false);
    let (ended_tx, ended) = watch::channel(false);
    let stopped = {
        let mut live = hub.dev_servers.live.locked();
        // Removed or stopped while it was starting: dropping `process` here ends it.
        let entry = live.get_mut(&saved.id).context("unknown dev server")?;
        let stopped = std::mem::take(&mut entry.stop_asked);
        if stopped {
            entry.ended(true, None);
        } else {
            entry.saved.wanted = true;
            entry.saved.leader = leader;
            entry.saved.boot_ms = boot_ms;
            save(&hub.store, &entry.saved);
            entry.run = Some(Run { stop, ended, process: process.clone() });
        }
        stopped
    };
    if stopped {
        hub.emit_bot(&saved.bot);
    } else {
        tokio::spawn(supervise(hub.clone(), saved.id.clone(), process, stop_seen, ended_tx));
    }
    Ok(())
}

async fn supervise(
    hub: Arc<Hub>,
    id: String,
    process: Arc<tokio::sync::Mutex<Process>>,
    mut stop: watch::Receiver<bool>,
    ended: watch::Sender<bool>,
) {
    let asked = loop {
        tokio::select! {
            // A dropped sender (the server was removed) asks too.
            _ = stop.changed() => break true,
            () = tokio::time::sleep(EXIT_POLL) => {
                if process.lock().await.exited().unwrap_or(true) {
                    break false;
                }
            }
        }
    };
    let code = {
        let mut process = process.lock().await;
        if asked {
            process.stop_gracefully(STOP_GRACE).await;
        } else {
            process.kill().await;
        }
        process.exit_code()
    };
    finished(&hub, &id, asked, code);
    ended.send_replace(true);
}

fn finished(hub: &Hub, id: &str, asked: bool, code: Option<i32>) {
    let bot = {
        let mut live = hub.dev_servers.live.locked();
        let Some(entry) = live.get_mut(id) else { return };
        entry.ended(asked, code);
        if !asked {
            save(&hub.store, &entry.saved);
            let by = if entry.status == Status::Failed { "failed" } else { "exited" };
            report(hub, Event::DevServerStopped, "by", by);
        }
        entry.saved.bot.clone()
    };
    hub.emit_bot(&bot);
}

/// Stops a server and waits until it ended; for what a bot left running through its shell, ends that.
pub async fn stop(hub: &Hub, bot: &str, id: &str, by: &str) -> Result<()> {
    if detected::stop(hub, bot, id).await? {
        report(hub, Event::DevServerStopped, "by", by);
        return Ok(());
    }
    let ended = {
        let mut live = hub.dev_servers.live.locked();
        let entry = live.get_mut(id).filter(|entry| entry.saved.bot == bot).context("unknown dev server")?;
        entry.saved.wanted = false;
        save(&hub.store, &entry.saved);
        entry.stop_asked = entry.run.is_none() && entry.status == Status::Starting;
        entry.run.as_ref().map(Run::ask_to_stop)
    };
    if let Some(mut ended) = ended
        && tokio::time::timeout(STOP_WAIT, ended.wait_for(|done| *done)).await.is_err()
    {
        tracing::warn!("a dev server didn't stop in time");
    }
    report(hub, Event::DevServerStopped, "by", by);
    Ok(())
}

/// Starts it again, stopping it first if it runs.
pub async fn restart(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    if hub.dev_servers.view(bot, id)?.status.active() {
        stop(hub, bot, id, "restart").await?;
    }
    launch(hub, id).await
}

pub fn remove(hub: &Hub, bot: &str, id: &str) -> Result<()> {
    let removed = {
        let mut live = hub.dev_servers.live.locked();
        if live.get(id).is_none_or(|entry| entry.saved.bot != bot) {
            bail!("unknown dev server");
        }
        live.remove(id)
    };
    drop(removed);
    forget(&hub.store, id)?;
    hub.emit_bot(bot);
    Ok(())
}
