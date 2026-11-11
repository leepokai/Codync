//! Versioned edits to skills a bot owns. Undo is another version, guarded against stale writes.

use super::bundled_skills::SKILLS_LOCK;
use super::{Skill, skills};
use crate::LockExt;
use crate::store::Store;
use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const KEPT: usize = 50;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Revision {
    version: u64,
    name: String,
    description: String,
    markdown: String,
    at: i64,
}

fn key(id: &str) -> String {
    format!("skill.history.{id}")
}

pub(super) fn remove(store: &Store, id: &str) -> Result<()> {
    store.kv_delete(&key(id))
}

fn revisions(store: &Store, skill: &Skill) -> Result<Vec<Revision>> {
    let mut versions: Vec<Revision> =
        store.kv_read(&key(&skill.id))?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    let markdown = std::fs::read_to_string(skill.dir().join("SKILL.md")).context("reading skill version")?;
    if versions
        .last()
        .is_none_or(|v| v.markdown != markdown || v.name != skill.name || v.description != skill.description)
    {
        versions.push(Revision {
            version: versions.last().map_or(1, |v| v.version + 1),
            name: skill.name.clone(),
            description: skill.description.clone(),
            markdown,
            at: crate::store::now_ms(),
        });
        let keep = versions.len().saturating_sub(KEPT);
        versions.drain(..keep);
        store.kv_set(&key(&skill.id), &serde_json::to_string(&versions)?)?;
    }
    Ok(versions)
}

fn owned<'a>(all: &'a [Skill], bot: &str, id: &str) -> Result<&'a Skill> {
    let skill = all.iter().find(|s| s.id == id).ok_or_else(|| anyhow!("Unknown skill"))?;
    ensure!(skill.written_by.as_deref() == Some(bot), "Only skills you saved can be changed");
    Ok(skill)
}

pub fn list(store: &Store, bot: &str, id: &str, version: Option<u64>) -> Result<Value> {
    let _guard = SKILLS_LOCK.locked();
    let all = skills(store);
    let skill = owned(&all, bot, id)?;
    let versions = revisions(store, skill)?;
    let current = versions.last().context("Skill has no current version")?.version;
    let mut result = json!({"id":id, "current":current, "versions":versions.iter().rev().map(|v| {
        json!({"version":v.version,"name":v.name,"description":v.description,"at":v.at})
    }).collect::<Vec<_>>()});
    if let Some(version) = version {
        let saved = versions.iter().find(|v| v.version == version).context("Skill version is no longer kept")?;
        result["markdown"] = saved.markdown.clone().into();
    }
    Ok(result)
}

pub(super) fn update(store: &Store, bot: &str, id: &str, name: &str, description: &str, body: &str) -> Result<Skill> {
    let _guard = SKILLS_LOCK.locked();
    let mut all = skills(store);
    let current = owned(&all, bot, id)?.clone();
    let versions = revisions(store, &current)?;
    let mut next = current.clone();
    next.name = skills::one_line(name);
    next.description = skills::one_line(description);
    let markdown = skills::markdown(&next, body);
    apply(store, &mut all, &next, &markdown, versions)?;
    Ok(next)
}

pub fn restore(store: &Store, bot: &str, id: &str, version: u64, expected: u64) -> Result<Skill> {
    let _guard = SKILLS_LOCK.locked();
    let mut all = skills(store);
    let mut next = owned(&all, bot, id)?.clone();
    let versions = revisions(store, &next)?;
    ensure!(
        versions.last().is_some_and(|v| v.version == expected),
        "The skill changed; read its history again before restoring it"
    );
    let old = versions.iter().find(|v| v.version == version).context("Skill version is no longer kept")?.clone();
    next.name = old.name;
    next.description = old.description;
    apply(store, &mut all, &next, &old.markdown, versions)?;
    Ok(next)
}

/// File and database updates share the skills lock. If SQLite refuses publication, restore the
/// original file and propagate the failure, leaving both metadata and version history unchanged.
fn apply(store: &Store, all: &mut [Skill], next: &Skill, markdown: &str, mut versions: Vec<Revision>) -> Result<()> {
    let previous = versions.last().context("Skill has no current version")?.clone();
    if previous.markdown == markdown && previous.name == next.name && previous.description == next.description {
        return Ok(());
    }
    let version = previous.version + 1;
    versions.push(Revision {
        version,
        name: next.name.clone(),
        description: next.description.clone(),
        markdown: markdown.to_owned(),
        at: crate::store::now_ms(),
    });
    let keep = versions.len().saturating_sub(KEPT);
    let history = serde_json::to_string(&versions[keep..])?;
    let slot = all.iter_mut().find(|s| s.id == next.id).context("Unknown skill")?;
    *slot = next.clone();
    let metadata = serde_json::to_string(all)?;
    let path = next.dir().join("SKILL.md");
    replace(&path, markdown)?;
    let saved = store.with_connection(|c| {
        let tx = c.unchecked_transaction()?;
        for (key, value) in [(key(&next.id), history), ("skills".to_owned(), metadata)] {
            tx.execute(
                "INSERT INTO kv(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
                rusqlite::params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    });
    if let Err(error) = saved {
        replace(&path, &previous.markdown).context("restoring the skill after its save failed")?;
        return Err(error);
    }
    Ok(())
}

fn replace(path: &std::path::Path, markdown: &str) -> Result<()> {
    let parent = path.parent().context("A skill file needs a directory")?;
    let temporary = parent.join(format!(".skill-{}.tmp", uuid::Uuid::new_v4()));
    let written = std::fs::write(&temporary, markdown).and_then(|()| std::fs::rename(&temporary, path));
    if let Err(error) = written {
        if let Err(cleanup) = std::fs::remove_file(&temporary)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%cleanup, "could not remove a temporary skill file");
        }
        return Err(error).context("replacing skill instructions");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_skill_edits_are_versioned_and_stale_undo_never_overwrites_new_work() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let name = format!("history-{}", uuid::Uuid::new_v4());
        let skill = super::super::add_custom_skill(&store, &name, "Test", "First", Some("a")).unwrap();
        update(&store, "a", &skill.id, &name, "Test", "Second").unwrap();
        assert_eq!(list(&store, "a", &skill.id, None).unwrap()["current"], 2);
        assert!(restore(&store, "a", &skill.id, 1, 1).is_err());
        assert!(restore(&store, "b", &skill.id, 1, 2).is_err());
        restore(&store, "a", &skill.id, 1, 2).unwrap();
        assert!(std::fs::read_to_string(skill.dir().join("SKILL.md")).unwrap().ends_with("First\n"));
        assert_eq!(list(&store, "a", &skill.id, None).unwrap()["current"], 3);
        super::super::remove_skill(&store, &skill.id).unwrap();
    }

    #[test]
    fn hermes_skill_publication_failure_restores_the_file_metadata_and_history() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let name = format!("rollback-{}", uuid::Uuid::new_v4());
        let skill = super::super::add_custom_skill(&store, &name, "Test", "Original", Some("a")).unwrap();
        let before = std::fs::read(skill.dir().join("SKILL.md")).unwrap();
        list(&store, "a", &skill.id, None).unwrap();
        let history_before = store.kv_read(&key(&skill.id)).unwrap();
        store.with_connection(|c| {
            c.execute_batch("CREATE TRIGGER fail_skill_publication BEFORE UPDATE ON kv WHEN new.k='skills' BEGIN SELECT RAISE(FAIL, 'injected save failure'); END;")?;
            Ok(())
        }).unwrap();
        assert!(update(&store, "a", &skill.id, "Changed", "Changed", "Replaced").is_err());
        assert_eq!(std::fs::read(skill.dir().join("SKILL.md")).unwrap(), before);
        assert_eq!(skills(&store)[0].name, name);
        assert_eq!(list(&store, "a", &skill.id, None).unwrap()["current"], 1);
        assert_eq!(store.kv_read(&key(&skill.id)).unwrap(), history_before);
        store
            .with_connection(|c| {
                c.execute_batch("DROP TRIGGER fail_skill_publication")?;
                Ok(())
            })
            .unwrap();
        super::super::remove_skill(&store, &skill.id).unwrap();
    }

    #[test]
    fn hermes_skill_restore_refuses_a_second_external_edit_after_reading_history() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let name = format!("external-{}", uuid::Uuid::new_v4());
        let skill = super::super::add_custom_skill(&store, &name, "Test", "Original", Some("a")).unwrap();
        list(&store, "a", &skill.id, None).unwrap();
        std::fs::write(skill.dir().join("SKILL.md"), "First outside edit").unwrap();
        let observed = list(&store, "a", &skill.id, None).unwrap()["current"].as_u64().unwrap();
        std::fs::write(skill.dir().join("SKILL.md"), "Second outside edit").unwrap();
        assert!(restore(&store, "a", &skill.id, 1, observed).is_err());
        assert_eq!(std::fs::read_to_string(skill.dir().join("SKILL.md")).unwrap(), "Second outside edit");
        super::super::remove_skill(&store, &skill.id).unwrap();
    }

    #[test]
    fn hermes_skill_history_keeps_fifty_versions_without_reusing_revision_numbers() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let name = format!("retention-{}", uuid::Uuid::new_v4());
        let skill = super::super::add_custom_skill(&store, &name, "Test", "Original", Some("a")).unwrap();
        list(&store, "a", &skill.id, None).unwrap();
        for version in 2..=55 {
            std::fs::write(skill.dir().join("SKILL.md"), format!("External edit {version}")).unwrap();
            let observed = list(&store, "a", &skill.id, None).unwrap();
            assert_eq!(observed["current"], version);
            assert!(observed["versions"].as_array().unwrap().len() <= KEPT);
        }
        let observed = list(&store, "a", &skill.id, None).unwrap();
        assert_eq!(observed["versions"].as_array().unwrap().len(), KEPT);
        assert_eq!(observed["versions"][KEPT - 1]["version"], 6);
        assert!(restore(&store, "a", &skill.id, 1, 55).is_err());
        restore(&store, "a", &skill.id, 6, 55).unwrap();
        assert_eq!(list(&store, "a", &skill.id, None).unwrap()["current"], 56);
        super::super::remove_skill(&store, &skill.id).unwrap();
    }
}
