//! A bot's projects: the user's folders it sends work to, the first its default (`BotConfig::projects`). The bot
//! itself always lives in its own folder (`cwd`); whatever sets `cwd` to another folder (an older app's folder picker,
//! a managing bot's `folder`, an import) puts that folder first among its projects instead.

mod migrate;

pub(crate) use migrate::migrate;

use super::Hub;
use crate::analytics::{self, Event};
use crate::store::BotConfig;
use anyhow::{Result, ensure};
use serde_json::json;
use std::path::Path;

/// Whether `cfg` is a bot that has projects: not a group, a thread, or the box of threads without a bot (which works
/// in the user's home folder on purpose).
fn has_projects(cfg: &BotConfig) -> bool {
    !(cfg.is_group() || cfg.is_thread() || cfg.plain)
}

/// A `cwd` that isn't the bot's own folder becomes its first project, and `cwd` empty, which `workspace::prepare`
/// fills with the bot's own folder.
pub(crate) fn adopt_cwd(cfg: &mut BotConfig) {
    if !has_projects(cfg) || cfg.cwd.is_empty() || Path::new(&cfg.cwd) == crate::agent::workspace::path(&cfg.id) {
        return;
    }
    let folder = std::mem::take(&mut cfg.cwd);
    cfg.projects.insert(0, folder);
}

/// Makes `cfg`'s projects ready to save over `before` (none: a new bot): a new `cwd` taken in as the first, no two
/// alike, each new one an absolute folder that exists. One saved before whose folder went since stays: an older app
/// can't take it off.
pub(super) fn settle(cfg: &mut BotConfig, before: Option<&BotConfig>) -> Result<()> {
    if before.is_none_or(|b| b.cwd != cfg.cwd) {
        adopt_cwd(cfg);
    }
    if !has_projects(cfg) {
        cfg.projects.clear();
        return Ok(());
    }
    dedupe(&mut cfg.projects);
    for project in &cfg.projects {
        if !before.is_some_and(|b| b.projects.contains(project)) {
            check(project)?;
        }
    }
    Ok(())
}

/// Each project once, where it first is.
fn dedupe(projects: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    projects.retain(|p| seen.insert(p.clone()));
}

/// A project the user adds: the full path of a folder that exists.
fn check(project: &str) -> Result<()> {
    ensure!(
        crate::agent::workspace::is_existing_folder(project),
        "project folder does not exist or isn't a full path: {project}"
    );
    Ok(())
}

/// `bot_project_added` / `bot_project_removed` for a save that took `before` to `after`: how many, never which.
pub(super) fn changed(hub: &Hub, before: &[String], after: &[String]) {
    let added = after.iter().filter(|p| !before.contains(p)).count();
    let removed = before.iter().filter(|p| !after.contains(p)).count();
    for (event, count) in [(Event::BotProjectAdded, added), (Event::BotProjectRemoved, removed)] {
        if count > 0 {
            analytics::capture(hub, event, json!({"count": count}).as_object().cloned().unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "5f0c7a52-0000-4000-8000-00000000c0de";

    fn bot(extra: &serde_json::Value) -> BotConfig {
        let mut v = json!({"id": ID, "name": "a", "backend": "claude"});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    fn folder() -> String {
        let dir = std::env::temp_dir().join(format!("codync-project-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn a_project_cwd_becomes_the_first_project_and_the_bot_goes_home() {
        let (a, b) = (folder(), folder());
        let mut cfg = bot(&json!({"cwd": b, "projects": [a, b]}));
        settle(&mut cfg, None).unwrap();
        assert_eq!((cfg.cwd.as_str(), cfg.projects.clone()), ("", vec![b.clone(), a.clone()]), "front, deduped");

        let own = crate::agent::workspace::path(ID).to_string_lossy().into_owned();
        let mut home = bot(&json!({"cwd": own, "projects": [a]}));
        settle(&mut home, None).unwrap();
        assert_eq!((home.cwd, home.projects), (own, vec![a.clone()]), "its own folder stays");

        let mut group = bot(&json!({"kind": "group", "cwd": "", "projects": [a]}));
        settle(&mut group, None).unwrap();
        assert!(group.projects.is_empty(), "a group has none");
        let mut plain = bot(&json!({"plain": true, "hidden": true, "cwd": a}));
        settle(&mut plain, None).unwrap();
        assert_eq!((plain.cwd, plain.projects.len()), (a, 0), "the box of threads without a bot keeps its folder");
    }

    #[test]
    fn an_unchanged_cwd_isnt_taken_in_on_an_edit() {
        let a = folder();
        let stored = bot(&json!({"cwd": a}));
        let mut cfg = stored.clone();
        cfg.name = "b".into();
        settle(&mut cfg, Some(&stored)).unwrap();
        assert_eq!((cfg.cwd, cfg.projects.len()), (a, 0), "the migration moves it, not any save");
    }

    #[test]
    fn a_new_project_must_exist_and_a_saved_one_that_went_stays() {
        let gone =
            std::env::temp_dir().join(format!("codync-gone-{}", uuid::Uuid::new_v4())).to_string_lossy().into_owned();
        let mut cfg = bot(&json!({"projects": [gone]}));
        assert!(settle(&mut cfg, None).unwrap_err().to_string().contains("does not exist"));
        let mut relative = bot(&json!({"projects": ["projects/x"]}));
        assert!(settle(&mut relative, None).unwrap_err().to_string().contains("full path"));

        let stored = bot(&json!({"projects": [gone]}));
        let mut edit = stored.clone();
        edit.description = "new".into();
        settle(&mut edit, Some(&stored)).unwrap();
        assert_eq!(edit.projects, vec![gone]);
    }
}
