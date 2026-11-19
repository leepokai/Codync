//! Versions of a bot's own folder (`workspace`): Codync keeps it a git repository of its own and commits it before
//! and after each turn that works there, when something changed, so what a turn did can be seen or undone (the bot
//! does that with git when the user asks). Only the bot's own folder, and only a repository Codync made there: a
//! project folder's history is the user's, and so is a repository the bot or the user made in its folder before.
//! Without git the folder works as before, with no versions.

use crate::LockExt as _;
use crate::agent::workspace;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

/// A new or changed file bigger than this stays out of the versions (a download, a video): each one would hold it.
const BIGGEST: u64 = 50 * 1024 * 1024;
/// How long a turn waits for the version before it, before it starts anyway.
const BEFORE_WAIT: Duration = Duration::from_secs(10);
/// The versions are Codync's, whatever the user set for git: none of their names, signing, hooks or fsmonitor.
const AS_CODYNC: [&str; 12] = [
    "-c",
    "user.name=Codync",
    "-c",
    "user.email=noreply@codync.dev",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "core.hooksPath=.git/no-hooks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "init.defaultBranch=main",
];
/// What a new repository leaves out: folders tools fill again by themselves (packages, caches, build output).
const IGNORED: &str = "# Codync keeps versions of this folder. What is listed here stays out of them.\n\
.DS_Store\nThumbs.db\nnode_modules/\n.venv/\nvenv/\n__pycache__/\n.cache/\ntarget/\ndist/\nbuild/\n.next/\n\
.gradle/\n.turbo/\n";
/// What Codync leaves in a repository it made, in `.git`: it saves versions in no other.
const MARK: &str = "codync-versions";
/// The files sent to the bot (`chat::uploads`): Codync removes some of them on purpose, which a version would keep.
const UPLOADS: &str = ":(exclude,literal)uploads";
/// Git is at work in the folder (the bot runs it, or stopped halfway through a merge): no version this time.
const BUSY: [&str; 5] = ["index.lock", "MERGE_HEAD", "CHERRY_PICK_HEAD", "rebase-merge", "rebase-apply"];

/// The step a save failed at (`own_folder_save_failed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Init,
    /// Git was at work there ([`BUSY`]).
    Busy,
    Look,
    Add,
    Commit,
}

impl Stage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Busy => "busy",
            Self::Look => "look",
            Self::Add => "add",
            Self::Commit => "commit",
        }
    }
}

/// The versions one bot or thread saves in its bot's own folder, in the order it asks for them: what a turn did is
/// never saved under the next turn's name.
#[derive(Default)]
pub struct Saves {
    /// The folder the running turn works in, and the name of its version.
    turn: Option<(PathBuf, String)>,
    /// The last save asked for, while it may still run: the next one waits for it.
    last: Option<JoinHandle<()>>,
}

impl Saves {
    /// `cfg`'s own folder gets versions: made a repository when it has none yet (a new bot, one from before
    /// versions, git installed since), and the bot shown again as one.
    pub fn ready(&mut self, hub: &Arc<Hub>, cfg: &BotConfig) {
        if !workspace::is_managed(cfg) {
            return;
        }
        let dir = workspace::path(&cfg.id);
        if dir.join(".git").exists() {
            return;
        }
        let (hub, id) = (hub.clone(), cfg.id.clone());
        self.then(async move {
            // Asked again (a settings change) before an earlier ask made it: that one did.
            if dir.join(".git").exists() {
                return;
            }
            if save(&hub, &dir, "Codync keeps versions of this folder").await {
                hub.emit_bot(&id);
            }
        });
    }

    /// Before a turn of `cfg` asked `text`: when it works in its bot's own folder, saves that as it is, so the turn's
    /// changes are a version of their own. Waits a while for that; a save that takes longer goes on by itself
    /// (stopping git halfway would leave its lock behind).
    pub async fn before(&mut self, hub: &Arc<Hub>, cfg: &BotConfig, text: &str) {
        self.turn = workspace::own_folder(cfg).map(|dir| (dir, title(text)));
        let Some((dir, title)) = self.turn.clone() else { return };
        let hub = hub.clone();
        let saving = self.then(async move {
            save(&hub, &dir, &format!("Before: {title}")).await;
        });
        if tokio::time::timeout(BEFORE_WAIT, saving).await.is_ok() {
            // Done: a finished task is never awaited again.
            self.last = None;
        }
    }

    /// After the turn: saves what it did.
    pub fn after(&mut self, hub: &Arc<Hub>) {
        let Some((dir, title)) = self.turn.take() else { return };
        let hub = hub.clone();
        self.then(async move {
            save(&hub, &dir, &title).await;
        });
    }

    /// Runs `next` once the last save asked for has ended.
    fn then(&mut self, next: impl Future<Output = ()> + Send + 'static) -> &mut JoinHandle<()> {
        let last = self.last.take();
        self.last.insert(tokio::spawn(async move {
            if let Some(last) = last {
                let _ = last.await;
            }
            next.await;
        }))
    }
}

/// A version's name from what its turn was asked: the first line of its words, past Codync's notes before them (a
/// message's id, a group chat's or a task's lead), kept short.
// ponytail: a first line of the user's own that starts with '[' is passed over too.
fn title(text: &str) -> String {
    const MOST: usize = 72;
    let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines().find(|l| !l.starts_with('[')).or_else(|| lines().next()).unwrap_or("A turn");
    let mut title: String = line.chars().take(MOST).collect();
    if line.chars().nth(MOST).is_some() {
        title.push('…');
    }
    title
}

/// Whether `dir` is a repository Codync made to keep its versions.
pub fn kept(dir: &Path) -> bool {
    dir.join(".git").join(MARK).exists()
}

/// One save at a time in a folder (a bot's turns and its threads' share it), and none waits for another folder's.
fn lock(dir: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static FOLDERS: LazyLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> = LazyLock::new(Mutex::default);
    FOLDERS.locked().entry(dir.to_owned()).or_default().clone()
}

/// Saves `dir` as it is now, as a version named `message`, making it a repository first when it has none. It
/// never holds a turn up with an error: what goes wrong is logged and counted. Whether it made the repository.
async fn save(hub: &Arc<Hub>, dir: &Path, message: &str) -> bool {
    if crate::git::program().await.is_none() {
        return false;
    }
    let lock = lock(dir);
    let _one = lock.lock().await;
    match commit(dir, message).await {
        Ok(true) => {
            analytics::capture(hub, Event::OwnFolderVersioned, serde_json::Map::new());
            true
        }
        Ok(false) => false,
        Err((stage, error)) => {
            tracing::warn!(dir = %dir.display(), stage = stage.as_str(), error = format!("{error:#}"), "couldn't save a version of a bot's folder");
            let properties = json!({"stage": stage.as_str()}).as_object().cloned().unwrap_or_default();
            analytics::capture(hub, Event::OwnFolderSaveFailed, properties);
            false
        }
    }
}

/// Commits `dir` as it is now, making it a repository first when it has none; one Codync didn't make is left as it
/// is. Whether it made the repository.
async fn commit(dir: &Path, message: &str) -> Result<bool, (Stage, anyhow::Error)> {
    let made = !dir.join(".git").exists();
    if made {
        // Its own repository, even inside another (a home folder kept in git): `add` stays in this one.
        git(dir, &["init", "-q"]).await.map_err(|e| (Stage::Init, e))?;
        let mark = "Codync made this repository to keep versions of the folder.\n";
        tokio::fs::write(dir.join(".git").join(MARK), mark).await.map_err(|e| (Stage::Init, e.into()))?;
        let ignore = dir.join(".gitignore");
        if !ignore.exists() {
            tokio::fs::write(&ignore, IGNORED).await.map_err(|e| (Stage::Init, e.into()))?;
        }
    } else if !kept(dir) {
        return Ok(false);
    } else if let Some(busy) = BUSY.iter().find(|name| dir.join(".git").join(name).exists()) {
        return Err((Stage::Busy, anyhow!("git is at work there ({busy})")));
    }
    let too_big = too_big(dir).await.map_err(|e| (Stage::Look, e))?;
    // What stays out stays out even when the bot staged it itself; what else it staged (a `git rm --cached`) is kept.
    let unstaged: Vec<String> = too_big.iter().map(|name| format!(":(literal){name}")).collect();
    let mut reset = vec!["reset", "-q", "--", ":(literal)uploads"];
    reset.extend(unstaged.iter().map(String::as_str));
    git(dir, &reset).await.map_err(|e| (Stage::Add, e))?;
    let left_out: Vec<String> = too_big.iter().map(|name| format!(":(exclude,literal){name}")).collect();
    let mut add = vec!["add", "-A", "--", ".", UPLOADS];
    add.extend(left_out.iter().map(String::as_str));
    git(dir, &add).await.map_err(|e| (Stage::Add, e))?;
    if !git(dir, &["diff", "--cached", "--name-only"]).await.map_err(|e| (Stage::Add, e))?.is_empty() {
        git(dir, &["commit", "-q", "--no-verify", "-m", message]).await.map_err(|e| (Stage::Commit, e))?;
    }
    Ok(made)
}

/// `git <args>` in `dir`, as Codync, to its end: a stopped `add` or `commit` would leave its lock behind, and no
/// version would be saved there again. Its output untrimmed: a `-z` list's names may start with a space.
async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let all: Vec<&str> = AS_CODYNC.iter().chain(args).copied().collect();
    crate::git::run_to_end(dir, &all).await
}

/// The new and changed files of `dir` bigger than [`BIGGEST`], as git names them, the ones the bot staged included.
async fn too_big(dir: &Path) -> Result<Vec<String>> {
    let changed = git(dir, &["ls-files", "-z", "--others", "--modified", "--exclude-standard"]).await?;
    let staged = git(dir, &["diff", "--cached", "--name-only", "-z", "--diff-filter=AM"]).await?;
    let dir = dir.to_owned();
    let big = tokio::task::spawn_blocking(move || {
        changed
            .split('\0')
            .chain(staged.split('\0'))
            .filter(|name| std::fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.len() > BIGGEST))
            .map(str::to_owned)
            .collect()
    });
    Ok(big.await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for one test, or none where git isn't installed.
    async fn folder() -> Option<PathBuf> {
        crate::git::program().await?;
        let dir = std::env::temp_dir().join(format!("codync-versions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("uploads")).expect("a folder");
        Some(dir)
    }

    async fn log(dir: &Path) -> Vec<String> {
        let out = git(dir, &["log", "--format=%s %an"]).await.expect("a log");
        out.lines().map(str::to_owned).collect()
    }

    async fn files(dir: &Path) -> Vec<String> {
        git(dir, &["ls-files"]).await.expect("its files").lines().map(str::to_owned).collect()
    }

    #[tokio::test]
    async fn a_folder_becomes_a_repository_and_each_change_a_version() {
        let Some(dir) = folder().await else { return };
        std::fs::write(dir.join("notes.md"), "one").expect("a file");
        std::fs::write(dir.join("uploads/photo.png"), "sent").expect("an upload");
        assert_eq!(commit(&dir, "First").await.ok(), Some(true));
        assert!(kept(&dir) && dir.join(".gitignore").exists(), "marked, and leaves out what tools fill again");
        assert_eq!(commit(&dir, "Nothing new").await.ok(), Some(false));
        std::fs::write(dir.join("notes.md"), "two").expect("a change");
        commit(&dir, "Second").await.expect("saved");
        assert_eq!(log(&dir).await, ["Second Codync", "First Codync"], "no version when nothing changed");
        assert_eq!(files(&dir).await, [".gitignore", "notes.md"], "no upload");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_file_too_big_stays_out() {
        let Some(dir) = folder().await else { return };
        let big = std::fs::File::create(dir.join("video [1].mp4")).expect("a file");
        big.set_len(BIGGEST + 1).expect("a big one");
        std::fs::write(dir.join("notes.md"), "kept").expect("a file");
        commit(&dir, "First").await.expect("made");
        assert_eq!(files(&dir).await, [".gitignore", "notes.md"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn what_the_bot_staged_itself_stays_out_too() {
        let Some(dir) = folder().await else { return };
        commit(&dir, "First").await.expect("made");
        std::fs::write(dir.join("uploads/photo.png"), "sent").expect("an upload");
        let big = std::fs::File::create(dir.join(" video.mp4")).expect("a file");
        big.set_len(BIGGEST + 1).expect("a big one");
        std::fs::write(dir.join("notes.md"), "kept").expect("a file");
        git(&dir, &["add", "-A", "--", ".", "uploads"]).await.expect("the bot's own add");
        commit(&dir, "Second").await.expect("saved");
        assert_eq!(files(&dir).await, [".gitignore", "notes.md"], "no upload, no big file with a leading space");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_file_the_bot_stopped_tracking_stays_untracked() {
        let Some(dir) = folder().await else { return };
        std::fs::write(dir.join("secret.env"), "key").expect("a file");
        commit(&dir, "First").await.expect("made");
        std::fs::write(dir.join(".gitignore"), "secret.env\n").expect("ignored now");
        git(&dir, &["rm", "-q", "--cached", "secret.env"]).await.expect("the bot's own rm");
        commit(&dir, "Second").await.expect("saved");
        assert_eq!(files(&dir).await, [".gitignore"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn no_version_while_git_is_at_work_there() {
        let Some(dir) = folder().await else { return };
        commit(&dir, "First").await.expect("made");
        std::fs::write(dir.join(".git/index.lock"), "").expect("a lock");
        std::fs::write(dir.join("notes.md"), "new").expect("a change");
        assert_eq!(commit(&dir, "Second").await.map_err(|(stage, _)| stage), Err(Stage::Busy));
        assert_eq!(log(&dir).await, ["First Codync"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_repository_codync_did_not_make_is_left_alone() {
        let Some(dir) = folder().await else { return };
        git(&dir, &["init", "-q"]).await.expect("the bot's own repository");
        std::fs::write(dir.join("notes.md"), "new").expect("a change");
        assert_eq!(commit(&dir, "Before: a turn").await.ok(), Some(false));
        assert!(!kept(&dir) && git(&dir, &["log"]).await.is_err(), "nothing committed on the user's branch");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_title_is_the_first_line_kept_short() {
        assert_eq!(title("\n  Fix the login page \nand more"), "Fix the login page");
        let asked = "[Codync: message m1]\n[Codync: this replies to your message: \"Plan\"]\nShip it\nnow";
        assert_eq!(title(asked), "Ship it");
        assert_eq!(title("[Codync: Ada replied]"), "[Codync: Ada replied]", "nothing but a note");
        assert_eq!(title(""), "A turn");
        assert_eq!(title(&"a".repeat(80)), format!("{}…", "a".repeat(72)));
    }
}
