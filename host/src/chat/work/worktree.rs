//! A git worktree for a thread that starts while another thread of its bot works in the same repository, so the two
//! don't write over each other: `~/.codync/worktrees/<bot>/<task>`, on a branch `codync/<title>` from the
//! workspace's HEAD. When its task is over, a worktree with no changes and no commits of its own goes, with its
//! branch; anything else stays, and the task's report says where, until the user has merged its commits (looked at
//! again at each host start). Codync never merges, rebases or resets, and forces nothing that held work (only a
//! worktree it made a moment ago and can't use is taken back): work on a branch is the user's to take.

use super::tasks::{Status, Task};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long one git command may take.
const LIMIT: Duration = Duration::from_mins(1);
/// Every branch Codync makes starts with this, and it deletes no other.
const BRANCH_PREFIX: &str = "codync/";
const SLUG_MOST: usize = 40;

/// Where a task's new thread works.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// The bot's folder.
    #[default]
    Folder,
    /// A git worktree of its own.
    Worktree,
    /// It was to have a worktree and git couldn't make one: the bot's folder after all.
    NoWorktree,
}

/// What became of a worktree once its task was over.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Left {
    /// Its thread still works in it, or work that follows will.
    #[default]
    InUse,
    /// Its task is over and it is being looked over (`settle_then`): work tried again waits until that says kept or
    /// removed; a message to the task's thread takes it back (`InUse`). A host that stopped meanwhile looks it over
    /// again when it starts (`recheck`).
    Settling,
    /// It has changes, or commits of its own the user hasn't merged: it stays, with its branch.
    Kept,
    /// It had no changes, and no commits the repository's own checkout doesn't have: it and its branch are gone.
    Removed,
}

/// A task's worktree (`work[].worktree` carries `branch` and `path`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub branch: String,
    /// The worktree's top folder.
    pub path: String,
    /// Where its thread works: `path`, or the workspace's own place in the repository under it.
    pub cwd: String,
    /// The commit it started from.
    base: String,
    #[serde(default)]
    pub left: Left,
}

impl Worktree {
    /// `work[].worktree`, while there is one to go to.
    pub fn wire(&self) -> Option<serde_json::Value> {
        (self.left != Left::Removed).then(|| serde_json::json!({"branch": self.branch, "path": self.path}))
    }

    /// What its task's report says of it, for the bot to tell the user.
    pub fn line(&self) -> String {
        let Self { branch, path, .. } = self;
        match self.left {
            Left::Removed => "[Codync: it worked in a git worktree of its own, which had no changes and no commits \
                              your folder doesn't have, and was removed with its branch.]"
                .to_owned(),
            Left::Kept | Left::Settling => format!(
                "[Codync: it worked in a git worktree of its own, because other work of yours ran in the same \
                 repository: branch {branch}, in {path}. Its changes are there, not in your folder, and nothing \
                 was merged: tell the user the branch, and merge it when they ask.]"
            ),
            Left::InUse => format!(
                "[Codync: it worked in a git worktree of its own: branch {branch}, in {path}. The work that follows \
                 it goes on there.]"
            ),
        }
    }

    /// What a thread that starts in it is told.
    pub fn brief(&self) -> String {
        format!(
            "[Codync: other work of yours runs in this repository now, so you work in a git worktree of your own: \
             {}, on branch {}, started from the folder's current commit. Uncommitted changes of the main folder \
             aren't here. Commit what you want kept; don't merge, and don't change the main folder's branch: your \
             report tells the chat where the work is.]",
            self.cwd, self.branch
        )
    }
}

/// Where Codync keeps the worktrees it makes.
fn root() -> PathBuf {
    crate::service::data_dir().join("worktrees")
}

/// Whether `cwd` is inside a git repository, as far as its folders say (no git is run).
pub fn in_repo(cwd: &str) -> bool {
    !cwd.is_empty() && Path::new(cwd).ancestors().any(|dir| dir.join(".git").exists())
}

/// A branch name's last part from a task's title: lowercase letters and digits, the rest dashes.
fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= SLUG_MOST {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { "work".to_owned() } else { slug.to_owned() }
}

/// Only ids the host issued become folder names.
fn folder(bot: &str, task: &str) -> Result<PathBuf> {
    for id in [bot, task] {
        uuid::Uuid::parse_str(id).context("not an id Codync issued")?;
    }
    Ok(root().join(bot).join(task))
}

/// Whether `worktree` is one Codync made: under its own folder, with nothing in the path that leaves it, on a
/// branch of its own naming, its thread's folder inside it. What a box says is checked before git is pointed at it.
fn ours(worktree: &Worktree) -> bool {
    let plain = |rest: &Path| rest.components().all(|part| matches!(part, Component::Normal(_)));
    let path = Path::new(&worktree.path);
    let inside = path.strip_prefix(root()).is_ok_and(|rest| plain(rest) && rest.components().count() == 2);
    let branch = worktree.branch.strip_prefix(BRANCH_PREFIX);
    let named = branch.is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
    inside && named && Path::new(&worktree.cwd).strip_prefix(path).is_ok_and(plain)
}

/// Whether `worktree` is still there to work in (one being looked over too: `dispatch` holds work back until that
/// is done).
pub fn usable(worktree: &Worktree) -> bool {
    worktree.left != Left::Removed && ours(worktree) && Path::new(&worktree.cwd).is_dir()
}

/// Makes a worktree of the repository `workspace` is in, for `task` of `bot`, from the workspace's HEAD.
pub async fn create(workspace: &str, bot: &str, task: &str, title: &str) -> Result<Worktree> {
    let workspace = Path::new(workspace);
    let base = crate::git::run(workspace, &["rev-parse", "--verify", "HEAD"], LIMIT).await?;
    // The workspace's own place in the repository (`apps/web/`), empty at its top.
    let within = crate::git::run(workspace, &["rev-parse", "--show-prefix"], LIMIT).await?;
    let dir = folder(bot, task)?;
    ensure!(!dir.exists(), "a worktree is already at {}", dir.display());
    let parent = dir.parent().context("a worktree's folder is inside Codync's")?.to_owned();
    tokio::fs::create_dir_all(&parent).await.context("creating the worktrees folder")?;
    let name = slug(title);
    let mut branch = format!("{BRANCH_PREFIX}{name}");
    for n in 2..=20 {
        let taken = ["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")];
        if crate::git::run(workspace, &taken, LIMIT).await.is_err() {
            break;
        }
        branch = format!("{BRANCH_PREFIX}{name}-{n}");
    }
    let path = dir.to_string_lossy().into_owned();
    let made = async {
        crate::git::run(workspace, &["worktree", "add", "-b", &branch, &path, &base], LIMIT).await?;
        let cwd = dir.join(within.trim_end_matches('/'));
        // A folder git doesn't track (ignored, or never added) isn't in the checkout: the thread would work at the
        // repository's top, without the bot's files.
        ensure!(cwd.is_dir(), "the bot's folder isn't in the repository's checkout");
        let cwd = cwd.to_string_lossy().into_owned();
        let made = Worktree { branch: branch.clone(), path: path.clone(), cwd, base, left: Left::InUse };
        ensure!(ours(&made), "the worktree isn't where Codync keeps them");
        Ok(made)
    }
    .await;
    if made.is_err() {
        discard(workspace, &dir, &branch).await;
    }
    made
}

/// Takes back the worktree `create` made at `dir` a moment ago and can't use (git checked it out and still failed,
/// as with a hook that fails, or it lacks the bot's folder), with its `branch`: nothing has worked there. Without
/// a folder git made nothing here, and `branch` may be one that was already there: it stays.
async fn discard(workspace: &Path, dir: &Path, branch: &str) {
    if !dir.exists() {
        return;
    }
    let path = dir.to_string_lossy();
    for args in [&["worktree", "remove", "--force", &path][..], &["branch", "-D", branch]] {
        if let Err(error) = crate::git::run(workspace, args, LIMIT).await {
            tracing::warn!(path = %path, error = format!("{error:#}"), "couldn't take back an unusable worktree");
        }
    }
}

/// `worktree`'s task is over: a worktree with no changes goes, with its branch, when it has no commits of its own or
/// they are merged into the repository's own checkout; anything else stays. One Codync didn't make, or can't read, is
/// never touched.
pub async fn settle(worktree: &Worktree) -> Left {
    match remove_if_done(worktree).await {
        Ok(left) => left,
        Err(error) => {
            tracing::warn!(path = worktree.path, error = format!("{error:#}"), "left a thread's worktree as it is");
            Left::Kept
        }
    }
}

async fn remove_if_done(worktree: &Worktree) -> Result<Left> {
    ensure!(ours(worktree), "not a worktree Codync made");
    let dir = Path::new(&worktree.path);
    if !dir.is_dir() {
        bail!("its folder is gone");
    }
    let git = |args: &'static [&'static str]| crate::git::run(dir, args, LIMIT);
    // Untracked files count whatever the user's `status.showUntrackedFiles` says, which git's own check on
    // removal follows: this one is what keeps a thread's new files.
    let dirty = !git(&["status", "--porcelain", "--untracked-files=all"]).await?.is_empty();
    let on = git(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    // On another branch: it did something there.
    if dirty || on != worktree.branch {
        return Ok(Left::Kept);
    }
    let tip = git(&["rev-parse", "HEAD"]).await?;
    // From the repository itself: a worktree can't remove the folder it is run in.
    let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;
    let repo = Path::new(&common);
    // Commits of its own stay until the repository's own checkout has them (the user merged them).
    if tip != worktree.base
        && crate::git::run(repo, &["merge-base", "--is-ancestor", &tip, "HEAD"], LIMIT).await.is_err()
    {
        return Ok(Left::Kept);
    }
    // Without --force: git refuses a worktree that changed meanwhile, and a branch that isn't merged.
    crate::git::run(repo, &["worktree", "remove", &worktree.path], LIMIT).await?;
    if let Err(error) = crate::git::run(repo, &["branch", "-d", &worktree.branch], LIMIT).await {
        tracing::info!(branch = worktree.branch, error = format!("{error:#}"), "left a thread's empty branch");
    }
    Ok(Left::Removed)
}

/// The worktree of task `id` of `tasks`, when the task is over and no work waits to follow it there: taken to be
/// looked over ([`settle_then`]), once. It reads as settling until that says what became of it.
pub(super) fn claim(tasks: &mut [Task], id: &str) -> Option<Worktree> {
    take(tasks, id, |left| left == Left::InUse)
}

/// At host start: the worktrees the last process kept (the user may have merged one since), or didn't finish
/// looking over, each with its task, taken to be looked over again as [`claim`] takes one.
// ponytail: only at host start (updates restart it); on a timer if merged worktrees pile up between restarts.
pub(super) fn recheck(tasks: &mut [Task]) -> Vec<(String, Worktree)> {
    let again = |left: Left| matches!(left, Left::Kept | Left::Settling);
    let ids: Vec<String> =
        tasks.iter().filter(|i| i.worktree.as_ref().is_some_and(|w| again(w.left))).map(|i| i.id.clone()).collect();
    ids.into_iter().filter_map(|id| take(tasks, &id, again).map(|worktree| (id, worktree))).collect()
}

fn take(tasks: &mut [Task], id: &str, from: impl Fn(Left) -> bool) -> Option<Worktree> {
    let followed = tasks.iter().any(|i| i.status == Status::Waiting && i.after.as_deref() == Some(id));
    let task = tasks.iter_mut().find(|i| i.id == id && i.status.ended() && !followed)?;
    let worktree = task.worktree.as_mut().filter(|w| from(w.left))?;
    worktree.left = Left::Settling;
    Some(worktree.clone())
}

/// At host start, once every box was picked up: the worktrees no task knows any more (it was over long ago, or its
/// bot is gone) go when their commits are merged.
pub(super) fn tidy(hub: &Arc<Hub>) {
    // Unit tests share one data folder: a hub starting in one would take another test's worktrees.
    if cfg!(test) {
        return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let hub = hub.clone();
    runtime.spawn(async move {
        let mut removed = 0;
        let Ok(mut bots) = tokio::fs::read_dir(root()).await else { return };
        while let Ok(Some(bot)) = bots.next_entry().await {
            let tasks = super::tasks::load(&hub.store, &bot.file_name().to_string_lossy());
            removed += tidy_orphans(&bot.path(), &known(&tasks)).await;
        }
        if removed > 0 {
            let properties = serde_json::json!({"removed": removed}).as_object().cloned().unwrap_or_default();
            crate::analytics::capture(&hub, crate::analytics::Event::WorktreesTidied, properties);
        }
    });
}

/// The worktree folders `tasks` still use: each task's own, and the one it works in, named after the task it went
/// on from, which may be gone since.
fn known(tasks: &[Task]) -> HashSet<String> {
    let used = tasks.iter().filter_map(|i| i.worktree.as_ref()).filter_map(|w| Path::new(&w.path).file_name());
    tasks.iter().map(|i| i.id.clone()).chain(used.map(|name| name.to_string_lossy().into_owned())).collect()
}

/// The worktrees in a bot's folder of them, `dir`, whose task isn't one of `known`: each goes when its commits are
/// merged, as [`settle`] says. How many went.
async fn tidy_orphans(dir: &Path, known: &HashSet<String>) -> usize {
    let mut removed = 0;
    let Ok(mut tasks) = tokio::fs::read_dir(dir).await else { return 0 };
    while let Ok(Some(task)) = tasks.next_entry().await {
        if known.contains(&*task.file_name().to_string_lossy()) {
            continue;
        }
        let dir = task.path();
        let Ok(branch) = crate::git::run(&dir, &["symbolic-ref", "--short", "HEAD"], LIMIT).await else { continue };
        let path = dir.to_string_lossy().into_owned();
        // No base known: only its merged commits let it go.
        let orphan = Worktree { branch, cwd: path.clone(), path, base: String::new(), left: Left::Kept };
        removed += usize::from(settle(&orphan).await == Left::Removed);
    }
    removed
}

/// The worktrees of `tasks` that are over and that nothing works in any more (`busy`: a thread's turn still runs),
/// each with its task: what a stopped task, or one whose follower never came, left behind.
pub(super) fn over(tasks: &mut [Task], busy: impl Fn(&str) -> bool) -> Vec<(String, Worktree)> {
    let idle = |task: &Task| {
        let thread = task.thread.as_deref();
        !thread.is_some_and(|thread| super::tasks::of_thread(tasks, thread).is_some() || busy(thread))
    };
    let ids: Vec<String> = tasks
        .iter()
        .filter(|i| i.status.ended() && i.worktree.as_ref().is_some_and(|w| w.left == Left::InUse) && idle(i))
        .map(|i| i.id.clone())
        .collect();
    ids.into_iter().filter_map(|id| claim(tasks, &id).map(|worktree| (id, worktree))).collect()
}

/// Looks over the claimed `worktree` of `bot`'s task `task` (it goes when untouched) and notes what became of it,
/// then does `then`: what had to wait for where the work is. Without a runtime (a unit test) it stays as it is.
pub(super) fn settle_then(
    hub: &Arc<Hub>,
    bot: &str,
    task: &str,
    worktree: Worktree,
    then: impl FnOnce(&Arc<Hub>) + Send + 'static,
) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return then(hub) };
    let (hub, bot, task) = (hub.clone(), bot.to_owned(), task.to_owned());
    runtime.spawn(async move {
        // A host that stops here looks it over again when it starts (`recheck`).
        let left = settle(&worktree).await;
        let noted = super::tasks::change(&hub, &bot, |tasks| {
            let task = tasks.iter_mut().find(|i| i.id == task);
            if let Some(w) = task.and_then(|i| i.worktree.as_mut()).filter(|w| w.left == Left::Settling) {
                w.left = left;
            }
            Ok(())
        });
        if let Err(error) = noted {
            tracing::warn!(task, error = format!("{error:#}"), "couldn't note what became of a worktree");
        }
        then(&hub);
    });
}

/// `tasks` leave their box (dropped, or their bot deleted): a worktree one still had goes if nothing was done in it.
pub(super) fn leave(tasks: &[Task]) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    for worktree in tasks.iter().filter_map(|i| i.worktree.clone()).filter(|w| w.left == Left::InUse) {
        runtime.spawn(async move { settle(&worktree).await });
    }
}

/// Thread `cfg` starts on its task: the worktree it works in, when it gets one (made now, or the one the task has
/// from before). None: it works in the bot's folder; so it does when git fails.
pub async fn enter(hub: &Arc<Hub>, cfg: &BotConfig) -> Option<Worktree> {
    let bot = cfg.thread_of.as_deref()?;
    let task = super::task_of(&hub.store, cfg).filter(|i| i.place == Place::Worktree && i.start.is_none())?;
    let worktree = match task.worktree.filter(usable) {
        Some(worktree) => worktree,
        None => match create(&cfg.cwd, bot, &task.id, &task.title).await {
            Ok(worktree) => worktree,
            Err(error) => {
                tracing::warn!(thread = %cfg.id, error = format!("{error:#}"), "no worktree: it works in the bot's folder");
                super::update(hub, cfg, "its missing worktree", |task| task.place = Place::NoWorktree);
                return None;
            }
        },
    };
    let worktree = Worktree { left: Left::InUse, ..worktree };
    super::update(hub, cfg, "its worktree", |task| task.worktree = Some(worktree.clone()));
    Some(worktree)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOT: &str = "5f0c7a52-0000-4000-8000-0000000000b1";

    /// A repository with one commit and a `web/` folder, or none where git isn't installed.
    async fn repo() -> Option<PathBuf> {
        crate::git::program().await?;
        let dir = std::env::temp_dir().join(format!("codync-worktree-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("web")).expect("a folder");
        std::fs::write(dir.join("web/index.html"), "<p>hi</p>").expect("a file");
        let dir = dir.canonicalize().expect("its real path");
        for args in [
            &["init", "-q"][..],
            &["add", "."],
            &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"],
        ] {
            crate::git::run(&dir, args, LIMIT).await.expect("git");
        }
        Some(dir)
    }

    fn task() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    async fn branches(repo: &Path) -> String {
        crate::git::run(repo, &["branch", "--list", "codync/*", "--format=%(refname:short)"], LIMIT)
            .await
            .expect("branches")
    }

    #[test]
    fn a_worktree_kept_or_being_looked_over_is_looked_over_again_after_a_restart() {
        let worktree = serde_json::json!({"branch": "codync/t", "path": "/w", "cwd": "/w", "base": "abc"});
        let task = serde_json::json!({"id": "i1", "title": "t", "brief": "x", "status": "failed", "order": 1,
            "createdAt": 0, "worktree": worktree});
        let mut tasks: Vec<Task> = serde_json::from_value(serde_json::json!([task])).expect("tasks");
        let left = |tasks: &[Task]| tasks[0].worktree.as_ref().map(|w| w.left);
        assert_eq!(claim(&mut tasks, "i1").map(|w| w.left), Some(Left::Settling));
        assert!(claim(&mut tasks, "i1").is_none(), "once");
        assert_eq!(recheck(&mut tasks).len(), 1, "a look the last process didn't finish");
        for (then, again) in [(Left::Kept, 1), (Left::Removed, 0), (Left::InUse, 0)] {
            tasks[0].worktree.as_mut().expect("a worktree").left = then;
            assert_eq!(recheck(&mut tasks).len(), again, "{then:?}");
        }
        assert_eq!(left(&tasks), Some(Left::InUse), "one in use is left as it is");
    }

    #[test]
    fn a_worktree_a_task_went_on_in_is_known_after_the_first_task_is_gone() {
        let worktree = serde_json::json!({"branch": "codync/t", "path": "/w/bot/i1", "cwd": "/w/bot/i1", "base": ""});
        let task = serde_json::json!({"id": "i2", "title": "t", "brief": "x", "status": "working", "order": 2,
            "createdAt": 0, "worktree": worktree});
        let tasks: Vec<Task> = serde_json::from_value(serde_json::json!([task])).expect("tasks");
        assert_eq!(known(&tasks), HashSet::from(["i1".to_owned(), "i2".to_owned()]));
    }

    #[test]
    fn a_branch_is_named_from_the_title() {
        assert_eq!(slug("Fix the Login redirect!"), "fix-the-login-redirect");
        assert_eq!(slug("  ../../etc  "), "etc");
        assert_eq!(slug("修正登入"), "work");
        assert!(slug(&"long word ".repeat(20)).len() <= SLUG_MOST);
    }

    #[test]
    fn only_a_worktree_codync_made_is_ever_touched() {
        let dir = folder(BOT, &task()).expect("a folder");
        let path = dir.to_string_lossy().into_owned();
        let made =
            Worktree { branch: "codync/fix".into(), cwd: path.clone(), path, base: "abc".into(), left: Left::InUse };
        assert!(ours(&made));
        let elsewhere = Worktree { path: "/tmp/repo".into(), cwd: "/tmp/repo".into(), ..made.clone() };
        assert!(!ours(&elsewhere), "outside Codync's folder");
        let climbs = format!("{}/../../..", made.path);
        assert!(!ours(&Worktree { cwd: climbs.clone(), path: climbs, ..made.clone() }), "a path that leaves it");
        assert!(!ours(&Worktree { branch: "main".into(), ..made.clone() }), "a branch that isn't Codync's");
        assert!(!ours(&Worktree { branch: "codync/../main".into(), ..made.clone() }));
        assert!(!ours(&Worktree { cwd: "/tmp".into(), ..made.clone() }), "a folder to work in outside it");
        let leaves = format!("{}/../../../elsewhere", made.path);
        assert!(!ours(&Worktree { cwd: leaves, ..made }), "a folder to work in that climbs out of it");
        assert!(folder("../x", BOT).is_err(), "only ids become folder names");
    }

    #[tokio::test]
    async fn an_untouched_worktree_goes_with_its_branch_and_one_with_work_stays() {
        let Some(repo) = repo().await else { return };
        let web = repo.join("web").to_string_lossy().into_owned();

        // From a folder inside the repository: the thread works in the same place of its worktree.
        let clean = create(&web, BOT, &task(), "Fix login").await.expect("a worktree");
        assert_eq!(clean.branch, "codync/fix-login");
        assert_eq!(Path::new(&clean.cwd), Path::new(&clean.path).join("web"));
        assert!(Path::new(&clean.cwd).join("index.html").exists());
        // The same title again takes another branch.
        let dirty = create(&web, BOT, &task(), "Fix login").await.expect("a second");
        assert_eq!(dirty.branch, "codync/fix-login-2");
        let committed = create(&web, BOT, &task(), "Docs").await.expect("a third");

        std::fs::write(Path::new(&dirty.cwd).join("new.txt"), "x").expect("a file");
        std::fs::write(Path::new(&committed.cwd).join("index.html"), "<p>new</p>").expect("a file");
        let commit = ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "second"];
        crate::git::run(Path::new(&committed.path), &commit, LIMIT).await.expect("a commit");
        // A repository that hides untracked files from `git status` still keeps a thread's new file.
        crate::git::run(&repo, &["config", "status.showUntrackedFiles", "no"], LIMIT).await.expect("a setting");

        assert_eq!(settle(&clean).await, Left::Removed);
        assert!(!Path::new(&clean.path).exists());
        assert_eq!(settle(&dirty).await, Left::Kept, "an uncommitted file");
        assert_eq!(settle(&committed).await, Left::Kept, "a commit of its own");
        assert!(Path::new(&dirty.cwd).join("new.txt").exists() && Path::new(&committed.path).exists());
        assert_eq!(branches(&repo).await, "codync/docs\ncodync/fix-login-2", "the untouched one's branch went too");
        assert!(usable(&dirty) && !usable(&Worktree { left: Left::Removed, ..clean.clone() }));
        assert!(usable(&Worktree { left: Left::Settling, ..dirty.clone() }), "a reopened task takes it back");

        // A record that points elsewhere is left alone, whatever is there.
        let foreign = Worktree { path: web.clone(), cwd: web.clone(), ..clean };
        assert_eq!(settle(&foreign).await, Left::Kept);
        assert!(Path::new(&web).exists());

        // Once the user merged its commits, a kept one goes, as one no task knows any more (no base) does.
        let orphan = Worktree { base: String::new(), left: Left::Kept, ..committed.clone() };
        assert_eq!(settle(&orphan).await, Left::Kept, "not merged yet");
        crate::git::run(&repo, &["merge", "-q", "--ff-only", &committed.branch], LIMIT).await.expect("merged");
        assert_eq!(settle(&orphan).await, Left::Removed);
        assert_eq!(branches(&repo).await, "codync/fix-login-2", "with its branch");

        let _ = std::fs::remove_dir_all(&dirty.path);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn at_host_start_a_worktree_no_task_knows_goes_only_once_merged() {
        let Some(repo) = repo().await else { return };
        let web = repo.join("web").to_string_lossy().into_owned();
        // A bot of its own: the other tests' worktrees are in other folders.
        let (bot, known) = (task(), task());
        // Untouched, so it would go too if it weren't a task's.
        let working = create(&web, &bot, &known, "Working").await.expect("a worktree");
        let commit = ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "work"];
        let mut orphans = Vec::new();
        for title in ["Merged", "Not merged"] {
            let orphan = create(&web, &bot, &task(), title).await.expect("a worktree");
            std::fs::write(Path::new(&orphan.cwd).join("index.html"), title).expect("a change");
            crate::git::run(Path::new(&orphan.path), &commit, LIMIT).await.expect("a commit");
            orphans.push(orphan);
        }
        crate::git::run(&repo, &["merge", "-q", "--ff-only", &orphans[0].branch], LIMIT).await.expect("merged");

        assert_eq!(tidy_orphans(&root().join(&bot), &HashSet::from([known])).await, 1);
        let there = |w: &Worktree| Path::new(&w.path).exists();
        assert!(!there(&orphans[0]) && there(&orphans[1]) && there(&working));
        assert_eq!(branches(&repo).await, "codync/not-merged\ncodync/working");

        let _ = std::fs::remove_dir_all(root().join(&bot));
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn a_folder_git_does_not_track_gets_no_worktree_and_leaves_none_behind() {
        let Some(repo) = repo().await else { return };
        std::fs::create_dir_all(repo.join("notes")).expect("a folder");
        let notes = repo.join("notes").to_string_lossy().into_owned();
        let id = task();
        assert!(create(&notes, BOT, &id, "Notes").await.is_err());
        assert!(!folder(BOT, &id).expect("a folder").exists(), "the checkout it made is taken back");
        assert_eq!(branches(&repo).await, "", "and so is its branch");
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn a_folder_outside_a_repository_gets_no_worktree() {
        if crate::git::program().await.is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("codync-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let plain = dir.to_string_lossy().into_owned();
        assert!(!in_repo(&plain));
        assert!(create(&plain, BOT, &task(), "x").await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
