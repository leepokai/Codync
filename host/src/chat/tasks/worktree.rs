//! A git worktree for a task that starts while another task of its bot works in the same repository, so the two
//! don't write over each other: `~/.codync/worktrees/<bot>/<item>`, on a branch `codync/<title>` from the
//! workspace's HEAD. When its item is over, a worktree with no changes and no commits of its own goes, with its
//! branch; anything else stays, and the item's report says where. Codync never merges, rebases, resets or forces:
//! work on a branch is the user's to take.

use super::items::{Item, Status};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long one git command may take.
const LIMIT: Duration = Duration::from_mins(1);
/// Every branch Codync makes starts with this, and it deletes no other.
const BRANCH_PREFIX: &str = "codync/";
const SLUG_MOST: usize = 40;

/// Where an item's new task works.
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

/// What became of a worktree once its item was over.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Left {
    /// Its task still works in it, or work that follows will.
    #[default]
    InUse,
    /// It has changes or commits of its own: it stays, with its branch.
    Kept,
    /// It had neither: it and its branch are gone.
    Removed,
}

/// An item's worktree (`work[].worktree` carries `branch` and `path`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub branch: String,
    /// The worktree's top folder.
    pub path: String,
    /// Where its task works: `path`, or the workspace's own place in the repository under it.
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

    /// What its item's report says of it, for the bot to tell the user.
    pub fn line(&self) -> String {
        let Self { branch, path, .. } = self;
        match self.left {
            Left::Removed => "[Codync: it worked in a git worktree of its own, which had no changes and no commits \
                              and was removed with its branch.]"
                .to_owned(),
            Left::Kept => format!(
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

    /// What a task that starts in it is told.
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

/// A branch name's last part from an item's title: lowercase letters and digits, the rest dashes.
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
fn folder(bot: &str, item: &str) -> Result<PathBuf> {
    for id in [bot, item] {
        uuid::Uuid::parse_str(id).context("not an id Codync issued")?;
    }
    Ok(root().join(bot).join(item))
}

/// Whether `worktree` is one Codync made: under its own folder, with nothing in the path that leaves it, on a
/// branch of its own naming. What a box says is checked before git is pointed at it.
fn ours(worktree: &Worktree) -> bool {
    let path = Path::new(&worktree.path);
    let inside = path.strip_prefix(root()).is_ok_and(|rest| {
        let mut parts = rest.components();
        let two = parts.by_ref().take(2).filter(|part| matches!(part, Component::Normal(_))).count() == 2;
        two && parts.next().is_none()
    });
    let branch = worktree.branch.strip_prefix(BRANCH_PREFIX);
    let named = branch.is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
    inside && named && Path::new(&worktree.cwd).starts_with(path)
}

/// Whether `worktree` is still there to work in.
pub fn usable(worktree: &Worktree) -> bool {
    worktree.left != Left::Removed && ours(worktree) && Path::new(&worktree.cwd).is_dir()
}

/// Makes a worktree of the repository `workspace` is in, for `item` of `bot`, from the workspace's HEAD.
pub async fn create(workspace: &str, bot: &str, item: &str, title: &str) -> Result<Worktree> {
    let workspace = Path::new(workspace);
    let base = crate::git::run(workspace, &["rev-parse", "--verify", "HEAD"], LIMIT).await?;
    // The workspace's own place in the repository (`apps/web/`), empty at its top.
    let within = crate::git::run(workspace, &["rev-parse", "--show-prefix"], LIMIT).await?;
    let dir = folder(bot, item)?;
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
    crate::git::run(workspace, &["worktree", "add", "-b", &branch, &path, &base], LIMIT).await?;
    let inner = dir.join(within.trim_end_matches('/'));
    let cwd = if inner.is_dir() { inner } else { dir };
    let made = Worktree { branch, path, cwd: cwd.to_string_lossy().into_owned(), base, left: Left::InUse };
    ensure!(ours(&made), "the worktree isn't where Codync keeps them");
    Ok(made)
}

/// `worktree`'s item is over: a worktree with no changes and no commits of its own goes, with its branch; anything
/// else stays. One Codync didn't make, or can't read, is never touched.
pub async fn settle(worktree: &Worktree) -> Left {
    match remove_if_untouched(worktree).await {
        Ok(left) => left,
        Err(error) => {
            tracing::warn!(path = worktree.path, error = format!("{error:#}"), "left a task's worktree as it is");
            Left::Kept
        }
    }
}

async fn remove_if_untouched(worktree: &Worktree) -> Result<Left> {
    ensure!(ours(worktree), "not a worktree Codync made");
    let dir = Path::new(&worktree.path);
    if !dir.is_dir() {
        bail!("its folder is gone");
    }
    let git = |args: &'static [&'static str]| crate::git::run(dir, args, LIMIT);
    let dirty = !git(&["status", "--porcelain"]).await?.is_empty();
    let head = git(&["rev-parse", "HEAD"]).await?;
    let on = git(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let tip = crate::git::run(dir, &["rev-parse", &format!("refs/heads/{}", worktree.branch)], LIMIT).await?;
    // On another branch, or with its own moved: it did something there.
    if dirty || head != worktree.base || tip != worktree.base || on != worktree.branch {
        return Ok(Left::Kept);
    }
    // From the repository itself: a worktree can't remove the folder it is run in.
    let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;
    let repo = Path::new(&common);
    // Without --force: git refuses a worktree that changed meanwhile, and a branch that isn't merged.
    crate::git::run(repo, &["worktree", "remove", &worktree.path], LIMIT).await?;
    if let Err(error) = crate::git::run(repo, &["branch", "-d", &worktree.branch], LIMIT).await {
        tracing::info!(branch = worktree.branch, error = format!("{error:#}"), "left a task's empty branch");
    }
    Ok(Left::Removed)
}

/// The worktree of item `id` of `items`, when the item is over and no work waits to follow it there: taken to be
/// looked over ([`settle_then`]), once. It reads as kept until that says otherwise.
pub(super) fn claim(items: &mut [Item], id: &str) -> Option<Worktree> {
    let followed = items.iter().any(|i| i.status == Status::Waiting && i.after.as_deref() == Some(id));
    let item = items.iter_mut().find(|i| i.id == id && i.status.ended() && !followed)?;
    let worktree = item.worktree.as_mut().filter(|w| w.left == Left::InUse)?;
    worktree.left = Left::Kept;
    Some(worktree.clone())
}

/// The worktrees of `items` that are over and that nothing works in any more (`busy`: a task's turn still runs),
/// each with its item: what a stopped item, or one whose follower never came, left behind.
pub(super) fn over(items: &mut [Item], busy: impl Fn(&str) -> bool) -> Vec<(String, Worktree)> {
    let idle = |item: &Item| {
        let task = item.task.as_deref();
        !task.is_some_and(|task| super::items::of_task(items, task).is_some() || busy(task))
    };
    let ids: Vec<String> = items
        .iter()
        .filter(|i| i.status.ended() && i.worktree.as_ref().is_some_and(|w| w.left == Left::InUse) && idle(i))
        .map(|i| i.id.clone())
        .collect();
    ids.into_iter().filter_map(|id| claim(items, &id).map(|worktree| (id, worktree))).collect()
}

/// Looks over the claimed `worktree` of `bot`'s item `item` (it goes when untouched), then does `then`: what had to
/// wait for where the work is. Without a runtime (a unit test) it stays as it is.
pub(super) fn settle_then(
    hub: &Arc<Hub>,
    bot: &str,
    item: &str,
    worktree: Worktree,
    then: impl FnOnce(&Arc<Hub>) + Send + 'static,
) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return then(hub) };
    let (hub, bot, item) = (hub.clone(), bot.to_owned(), item.to_owned());
    runtime.spawn(async move {
        // ponytail: a host that stops here leaves an untouched worktree behind, marked kept; look them over at
        // host start if that ever piles up.
        if settle(&worktree).await == Left::Removed {
            let noted = super::items::change(&hub, &bot, |items| {
                if let Some(w) = items.iter_mut().find(|i| i.id == item).and_then(|i| i.worktree.as_mut()) {
                    w.left = Left::Removed;
                }
                Ok(())
            });
            if let Err(error) = noted {
                tracing::warn!(item, error = format!("{error:#}"), "couldn't note that a worktree went");
            }
        }
        then(&hub);
    });
}

/// `items` leave their box (dropped, or their bot deleted): a worktree one still had goes if nothing was done in it.
pub(super) fn leave(items: &[Item]) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    for worktree in items.iter().filter_map(|i| i.worktree.clone()).filter(|w| w.left == Left::InUse) {
        runtime.spawn(async move { settle(&worktree).await });
    }
}

/// Task `cfg` starts on its item: the worktree it works in, when it gets one (made now, or the one the item has
/// from before). None: it works in the bot's folder; so it does when git fails.
pub async fn enter(hub: &Arc<Hub>, cfg: &BotConfig) -> Option<Worktree> {
    let bot = cfg.task_of.as_deref()?;
    let item = super::item_of(&hub.store, cfg).filter(|i| i.place == Place::Worktree && i.start.is_none())?;
    let worktree = match item.worktree.filter(usable) {
        Some(worktree) => worktree,
        None => match create(&cfg.cwd, bot, &item.id, &item.title).await {
            Ok(worktree) => worktree,
            Err(error) => {
                tracing::warn!(task = %cfg.id, error = format!("{error:#}"), "no worktree: it works in the bot's folder");
                super::update(hub, cfg, "its missing worktree", |item| item.place = Place::NoWorktree);
                return None;
            }
        },
    };
    let worktree = Worktree { left: Left::InUse, ..worktree };
    super::update(hub, cfg, "its worktree", |item| item.worktree = Some(worktree.clone()));
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

    fn item() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    async fn branches(repo: &Path) -> String {
        crate::git::run(repo, &["branch", "--list", "codync/*", "--format=%(refname:short)"], LIMIT)
            .await
            .expect("branches")
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
        let dir = folder(BOT, &item()).expect("a folder");
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
        assert!(!ours(&Worktree { cwd: "/tmp".into(), ..made }), "a folder to work in outside it");
        assert!(folder("../x", BOT).is_err(), "only ids become folder names");
    }

    #[tokio::test]
    async fn an_untouched_worktree_goes_with_its_branch_and_one_with_work_stays() {
        let Some(repo) = repo().await else { return };
        let web = repo.join("web").to_string_lossy().into_owned();

        // From a folder inside the repository: the task works in the same place of its worktree.
        let clean = create(&web, BOT, &item(), "Fix login").await.expect("a worktree");
        assert_eq!(clean.branch, "codync/fix-login");
        assert_eq!(Path::new(&clean.cwd), Path::new(&clean.path).join("web"));
        assert!(Path::new(&clean.cwd).join("index.html").exists());
        // The same title again takes another branch.
        let dirty = create(&web, BOT, &item(), "Fix login").await.expect("a second");
        assert_eq!(dirty.branch, "codync/fix-login-2");
        let committed = create(&web, BOT, &item(), "Docs").await.expect("a third");

        std::fs::write(Path::new(&dirty.cwd).join("new.txt"), "x").expect("a file");
        std::fs::write(Path::new(&committed.cwd).join("index.html"), "<p>new</p>").expect("a file");
        let commit = ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "second"];
        crate::git::run(Path::new(&committed.path), &commit, LIMIT).await.expect("a commit");

        assert_eq!(settle(&clean).await, Left::Removed);
        assert!(!Path::new(&clean.path).exists());
        assert_eq!(settle(&dirty).await, Left::Kept, "an uncommitted file");
        assert_eq!(settle(&committed).await, Left::Kept, "a commit of its own");
        assert!(Path::new(&dirty.cwd).join("new.txt").exists() && Path::new(&committed.path).exists());
        assert_eq!(branches(&repo).await, "codync/docs\ncodync/fix-login-2", "the untouched one's branch went too");
        assert!(usable(&dirty) && !usable(&Worktree { left: Left::Removed, ..clean.clone() }));

        // A record that points elsewhere is left alone, whatever is there.
        let foreign = Worktree { path: web.clone(), cwd: web.clone(), ..clean };
        assert_eq!(settle(&foreign).await, Left::Kept);
        assert!(Path::new(&web).exists());

        for kept in [&dirty, &committed] {
            let _ = std::fs::remove_dir_all(&kept.path);
        }
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
        assert!(create(&plain, BOT, &item(), "x").await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
