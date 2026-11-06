//! `watch` triggers: a routine wakes only when what it watches changes. When one is due, the
//! host fingerprints its target without the model (a file or folder: names, sizes and
//! modification times; a URL: status and body; a command: stdout/stderr SHA-256) and queues a
//! run only when the fingerprint differs from the last one. The first check only records a
//! baseline. Repeated failures pause the routine and say why.
//!
//! Idea from rakazo's `packages/adapters/src/routine-watch.ts` (Apache-2.0) and `bdots` (Idea 42).

use super::{EntryKind, Hub, Lane, Result, Routines, Status, Store, Trigger, bail, json, new_run};
use crate::egress::Policy;
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt as _;

/// Entries a folder fingerprint may cover.
const MAX_ENTRIES: usize = 2000;
/// Folders a fingerprint skips: they churn on their own and are rarely what's being watched.
const SKIPPED: [&str; 2] = [".git", "node_modules"];
/// Failed checks in a row before the routine pauses.
const FAILURES_BEFORE_PAUSE: u32 = 3;
const URL_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
/// Largest page a URL fingerprint reads.
const MAX_PAGE: usize = 4 * 1024 * 1024;
/// Largest output a command watch reads.
const MAX_COMMAND_OUTPUT: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WatchKind {
    /// A file, or a folder and everything under it.
    Files,
    /// A public HTTPS page or API answer.
    Url,
    /// A lightweight shell command whose output is fingerprinted before waking the LLM (Idea 42).
    Command,
}

/// What a watch trigger last saw.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub failures: u32,
}

/// One due check, taken out of the routine state so it can run without its lock.
pub struct Check {
    pub routine_id: String,
    pub bot_id: String,
    pub index: usize,
    pub kind: WatchKind,
    pub target: String,
}

pub fn validate(kind: WatchKind, target: &str, every_minutes: i64) -> Result<()> {
    if !(1..=10_080).contains(&every_minutes) {
        bail!("a watch is checked every 1 minute to 7 days (everyMinutes 1-10080)");
    }
    if target.trim().is_empty() || target.len() > 2048 || target.contains('\0') {
        bail!("a watch needs a target: a path, an https URL, or a shell command");
    }
    if kind == WatchKind::Url {
        let url = reqwest::Url::parse(target).context("a url watch needs an https URL")?;
        if url.scheme() != "https" {
            bail!("a url watch needs an https URL");
        }
    }
    Ok(())
}

/// The fingerprint of what `kind`/`target` names; relative paths and commands run under `cwd`.
pub async fn fingerprint(kind: WatchKind, target: &str, cwd: &Path) -> Result<String> {
    match kind {
        WatchKind::Files => {
            let path = cwd.join(target);
            tokio::task::spawn_blocking(move || files(&path)).await.context("the file check stopped")?
        }
        WatchKind::Url => page(target).await,
        WatchKind::Command => command_output(target, cwd).await,
    }
}

/// Names, sizes and modification times of a file, or of a folder's tree (the target itself may be
/// a symlink; links inside aren't followed). A missing target is a state of its own, so "when
/// report.pdf appears" works.
fn files(path: &Path) -> Result<String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(crate::agent::registry::sha256_hex(b"absent"));
        }
        Err(error) => return Err(error).with_context(|| format!("can't read {}", path.display())),
    };
    let mut entries = vec![];
    if meta.is_dir() {
        let mut folders = vec![PathBuf::new()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(path.join(&folder))? {
                let entry = entry?;
                let name = folder.join(entry.file_name());
                let meta = entry.metadata()?;
                // A skipped folder is an entry by name only: what a tool writes inside it (`.git/HEAD` on every
                // commit, a lock file) is not watched, and its own mtime would change on every write.
                let skipped = meta.is_dir() && SKIPPED.iter().any(|s| entry.file_name() == *s);
                if meta.is_dir() && !skipped {
                    folders.push(name.clone());
                }
                entries.push(if skipped { name.display().to_string() } else { line(&name, &meta) });
                if entries.len() > MAX_ENTRIES {
                    bail!("{} holds more than {MAX_ENTRIES} entries; watch a smaller folder", path.display());
                }
            }
        }
    } else {
        entries.push(line(Path::new(""), &meta));
    }
    entries.sort();
    Ok(crate::agent::registry::sha256_hex(entries.join("\n").as_bytes()))
}

fn line(name: &Path, meta: &std::fs::Metadata) -> String {
    let modified =
        meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    format!("{}\t{}\t{}\t{modified}", name.display(), u8::from(meta.is_dir()), meta.len())
}

/// A public page's body. An answer other than success is a failed check, not a change, so a
/// server's brief outage doesn't fire the routine twice.
async fn page(url: &str) -> Result<String> {
    let res = crate::egress::client(Policy::Public, url).await?.get(url).timeout(URL_TIMEOUT).send().await?;
    if !res.status().is_success() {
        bail!("{url} answered {}", res.status());
    }
    let body = crate::egress::body(res, MAX_PAGE).await?;
    Ok(crate::agent::registry::sha256_hex(&body))
}

/// Runs a lightweight shell command in `cwd` and hashes its trimmed stdout/stderr (Idea 42).
async fn command_output(command: &str, cwd: &Path) -> Result<String> {
    let (shell, _script) = crate::shell::expanding(command, &[]).await.context("couldn't prepare watch command")?;
    let mut cmd = shell.tokio();
    cmd.current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    crate::agent::process::Process::prepare(&mut cmd);
    let mut child = cmd.spawn().context("couldn't start watch command")?;
    let mut stdout = child.stdout.take();
    let status = tokio::time::timeout(COMMAND_TIMEOUT, child.wait())
        .await
        .map_err(|_| anyhow!("watch command timed out"))?
        .context("watch command failed")?;
    if !status.success() {
        bail!("watch command exited with {:?}", status.code());
    }
    let mut bytes = Vec::new();
    if let Some(ref mut out) = stdout {
        out.take(MAX_COMMAND_OUTPUT).read_to_end(&mut bytes).await?;
    }
    let trimmed = String::from_utf8_lossy(&bytes).trim().to_owned();
    Ok(crate::agent::registry::sha256_hex(trimmed.as_bytes()))
}

impl Routines {
    /// Records what `check` found: a changed fingerprint queues a run (unless one is already
    /// waiting); the first one is only a baseline; repeated failures pause the routine.
    /// Returns the routine's name and the reason when it paused it.
    pub(super) fn record_watch(
        &self,
        store: &Store,
        check: &Check,
        found: Result<String>,
    ) -> Result<Option<(String, String)>> {
        self.change(store, |s| {
            let Some(routine) = s.routines.iter_mut().find(|r| r.id == check.routine_id && !r.deleted && r.enabled)
            else {
                return Ok(None);
            };
            // The triggers changed while it ran: this check no longer applies.
            if !matches!(routine.triggers.get(check.index), Some(Trigger::Watch { kind, target, .. }) if *kind == check.kind && *target == check.target)
            {
                return Ok(None);
            }
            routine.watch.resize(routine.triggers.len(), WatchState::default());
            let state = &mut routine.watch[check.index];
            match found {
                Ok(fingerprint) => {
                    state.failures = 0;
                    let changed = state.fingerprint.as_ref().is_some_and(|old| *old != fingerprint);
                    state.fingerprint = Some(fingerprint);
                    let waiting = s.runs.iter().any(|r| r.routine_id == routine.id && r.status == Status::Pending);
                    if changed && !waiting {
                        let event = json!({"source":"watch","kind":check.kind,"target":check.target});
                        let run = new_run(routine, event, None);
                        s.runs.push(run);
                    }
                    Ok(None)
                }
                Err(error) => {
                    state.failures += 1;
                    if state.failures < FAILURES_BEFORE_PAUSE {
                        return Ok(None);
                    }
                    let reason =
                        format!("Paused: couldn't check {} {FAILURES_BEFORE_PAUSE} times in a row ({error:#})", check.target);
                    routine.enabled = false;
                    routine.last_error = Some(reason.clone());
                    let paused = (routine.name.clone(), reason);
                    // Paused like `set_enabled(false)`: what was waiting doesn't run.
                    super::cancel_pending(s, &check.routine_id);
                    Ok(Some(paused))
                }
            }
        })
    }
}

/// Runs `check`, records it, and tells the bot's chat when it paused the routine.
pub async fn run(hub: std::sync::Arc<Hub>, check: Check) {
    let cwd = hub.store.bot(&check.bot_id).ok().flatten().map(|b| PathBuf::from(b.config.cwd));
    let found = match cwd {
        Some(cwd) => fingerprint(check.kind, &check.target, &cwd).await,
        None => Err(anyhow!("the routine's bot is gone")),
    };
    match hub.routines.record_watch(&hub.store, &check, found) {
        Ok(Some((name, reason))) => {
            tracing::warn!(routine = %check.routine_id, reason = %reason, "watch paused its routine");
            let notice = json!({"text":format!("{name}: {reason}"),"routineId":check.routine_id,"style":"error"});
            if hub.add_entry(&Lane::main(&check.bot_id), EntryKind::Notice, 0, &notice).is_none() {
                tracing::error!(routine = %check.routine_id, "couldn't post the paused routine notice");
            }
        }
        Ok(None) => {}
        Err(error) => {
            tracing::error!(routine = %check.routine_id, error = format!("{error:#}"), "couldn't record a watch check");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LockExt;

    #[test]
    fn a_folder_fingerprint_changes_with_its_files() {
        let dir = std::env::temp_dir().join(format!("codync-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        let first = files(&dir).unwrap();
        assert_eq!(files(&dir).unwrap(), first);
        std::fs::write(dir.join("sub/b.txt"), "two").unwrap();
        let second = files(&dir).unwrap();
        assert_ne!(second, first);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "x").unwrap();
        // The .git folder itself is an entry, its contents aren't.
        let with_git = files(&dir).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "changed").unwrap();
        assert_eq!(files(&dir).unwrap(), with_git);
        // A missing file is a state, and its appearing is a change.
        let absent = files(&dir.join("report.pdf")).unwrap();
        std::fs::write(dir.join("report.pdf"), "pdf").unwrap();
        assert_ne!(files(&dir.join("report.pdf")).unwrap(), absent);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_changed_fingerprint_queues_a_run_and_failures_pause() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let routines = Routines::default();
        let mut routine = super::super::tests::routine();
        routine.triggers = vec![Trigger::Watch { kind: WatchKind::Files, target: "notes".into(), every_minutes: 5 }];
        routines
            .change(&store, |s| {
                s.routines.push(routine);
                Ok(())
            })
            .unwrap();
        let check = Check {
            routine_id: "routine".into(),
            bot_id: "bot".into(),
            index: 0,
            kind: WatchKind::Files,
            target: "notes".into(),
        };
        let pending = |r: &Routines| r.state.locked().runs.iter().filter(|r| r.status == Status::Pending).count();
        routines.record_watch(&store, &check, Ok("a".into())).unwrap();
        assert_eq!(pending(&routines), 0, "the first check is a baseline");
        routines.record_watch(&store, &check, Ok("a".into())).unwrap();
        assert_eq!(pending(&routines), 0);
        routines.record_watch(&store, &check, Ok("b".into())).unwrap();
        assert_eq!(pending(&routines), 1);
        routines.record_watch(&store, &check, Ok("c".into())).unwrap();
        assert_eq!(pending(&routines), 1, "one waiting run covers later changes");
        for _ in 0..2 {
            assert!(routines.record_watch(&store, &check, Err(anyhow!("offline"))).unwrap().is_none());
        }
        let paused = routines.record_watch(&store, &check, Err(anyhow!("offline"))).unwrap();
        assert!(paused.unwrap().1.contains("offline"));
        assert!(!routines.state.locked().routines[0].enabled);
    }

    #[test]
    fn watches_are_validated() {
        assert!(validate(WatchKind::Files, "notes", 5).is_ok());
        assert!(validate(WatchKind::Files, "", 5).is_err());
        assert!(validate(WatchKind::Files, "notes", 0).is_err());
        assert!(validate(WatchKind::Url, "http://example.com", 5).is_err());
        assert!(validate(WatchKind::Url, "https://example.com/status", 60).is_ok());
        assert!(validate(WatchKind::Command, "git status --porcelain", 15).is_ok());
    }
}
