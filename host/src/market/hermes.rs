//! Source: NousResearch/hermes-agent skills/ and optional-skills/ at 10d25f8a (MIT).
//! Adaptation: offline listing and installation through Codync's existing skill store.

use anyhow::{Context, Result, anyhow, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Component, Path};
use std::sync::LazyLock;

pub const SOURCE: &str = "hermes/";

#[derive(Deserialize)]
struct Catalog {
    commit: String,
    skills: Vec<Seed>,
}

#[derive(Deserialize)]
struct Seed {
    id: String,
    name: String,
    description: String,
    category: String,
    path: String,
    body: String,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("hermes/skills.json")).expect("valid pinned Hermes skill catalog")
});
const ARCHIVE: &[u8] = include_bytes!("hermes/skills.tar.gz");

pub fn browse_skills(installed: &[String]) -> Vec<Value> {
    CATALOG
        .skills
        .iter()
        .map(|s| {
            let source = format!("{SOURCE}{}", s.id);
            json!({
                "source": source, "name": s.name, "description": s.description,
                "category": s.category, "creator": "Nous Research / Hermes", "featured": false,
                "installed": installed.contains(&source),
            })
        })
        .collect()
}

fn seed(source: &str) -> Result<&'static Seed> {
    let id = source.strip_prefix(SOURCE).ok_or_else(|| anyhow!("Unknown Hermes skill"))?;
    CATALOG.skills.iter().find(|s| s.id == id).ok_or_else(|| anyhow!("Unknown Hermes skill"))
}

pub fn skill_details(source: &str) -> Result<Value> {
    let s = seed(source)?;
    Ok(json!({
        "markdown": s.body,
        "origin": format!("https://github.com/NousResearch/hermes-agent/tree/{}/{}", CATALOG.commit, s.path),
        "license": "MIT",
    }))
}

/// Catalog skills never overwrite a folder occupied by a different source.
pub fn install(store: &crate::store::Store, source: &str) -> Result<super::Skill> {
    let s = seed(source)?;
    super::bundled_skills::install(store, &s.id, source, &s.name, &s.description, |dest| extract(&s.id, dest))
}

/// An improvement belongs to its bot. Copy the pinned bundle before editing so other bots
/// keep the imported original, including its scripts and references.
pub fn fork(
    store: &crate::store::Store,
    bot: &str,
    source: &str,
    name: &str,
    description: &str,
    instructions: &str,
) -> Result<super::Skill> {
    let original = seed(source)?;
    let skill = super::add_custom_skill(store, &original.name, &original.description, &original.body, Some(bot))?;
    let result = extract(&original.id, &skill.dir())
        .and_then(|()| super::update_custom_skill(store, bot, &skill.id, name, description, instructions));
    if result.is_err() {
        super::remove_skill(store, &skill.id).context("removing an incomplete Hermes skill improvement")?;
    }
    result
}

fn extract(id: &str, dest: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(ARCHIVE);
    let mut archive = tar::Archive::new(decoder);
    let prefix = format!("{id}/");
    let mut found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let Some(rest) = path.strip_prefix(&prefix) else { continue };
        ensure!(entry.header().entry_type().is_file(), "Hermes skill contains a non-file entry");
        ensure!(safe_path(rest), "Hermes skill contains an unsafe path");
        ensure!(entry.size() <= 16 * 1024 * 1024, "Hermes skill file exceeds 16 MiB");
        let target = dest.join(rest);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target).context("extracting Hermes skill")?;
        found |= rest == "SKILL.md";
    }
    ensure!(found, "Hermes skill instructions are missing");
    Ok(())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && Path::new(path).components().all(|p| matches!(p, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_skills_have_unique_ids_and_preserve_their_instructions() {
        assert_eq!(CATALOG.commit, "10d25f8ad65fdf71304e9272959421a0c22e5d23");
        assert!(!CATALOG.skills.is_empty());
        let mut ids = std::collections::HashSet::new();
        for s in &CATALOG.skills {
            assert!(ids.insert(&s.id), "duplicate {}", s.id);
            assert_eq!(super::super::slug(&s.id), s.id);
            assert!(!s.body.is_empty(), "{} has no instructions", s.path);
            assert!(matches!(
                s.category.as_str(),
                "coding"
                    | "design"
                    | "documents"
                    | "research"
                    | "data-analytics"
                    | "productivity"
                    | "automation"
                    | "other"
            ));
            let dir = std::env::temp_dir().join(format!("codync-hermes-{}", uuid::Uuid::new_v4()));
            extract(&s.id, &dir).unwrap();
            let md = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
            assert_eq!(super::super::skill_body(&md), s.body);
            assert!(dir.join("HERMES-LICENSE").is_file());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn extraction_rejects_paths_outside_the_skill() {
        for bad in ["", "../secret", "/secret", "C:/secret", "a/../../secret", "a\\secret"] {
            assert!(!safe_path(bad), "{bad}");
        }
        assert!(safe_path("scripts/run.py"));
    }

    #[test]
    fn cards_mark_installed_skills_and_details_keep_the_source() {
        let s = &CATALOG.skills[0];
        let source = format!("{SOURCE}{}", s.id);
        let cards = browse_skills(std::slice::from_ref(&source));
        assert_eq!(cards[0]["installed"], true);
        assert_eq!(skill_details(&source).unwrap()["license"], "MIT");
        assert!(skill_details("hermes/unknown").is_err());
    }

    #[test]
    fn hermes_improvements_keep_the_imported_original_and_can_restore_the_pinned_text() {
        let store = crate::store::Store::open(Path::new(":memory:")).unwrap();
        let seed = &CATALOG.skills[0];
        let original = install(&store, &format!("{SOURCE}{}", seed.id)).unwrap();
        let before = std::fs::read(original.dir().join("SKILL.md")).unwrap();
        let improved =
            fork(&store, "test-bot", &original.source, &seed.name, &seed.description, "Improved steps").unwrap();
        assert_ne!(original.id, improved.id);
        assert_eq!(std::fs::read(original.dir().join("SKILL.md")).unwrap(), before);
        assert!(improved.dir().join("HERMES-LICENSE").exists());
        assert_eq!(super::super::skill_history::list(&store, "test-bot", &improved.id, None).unwrap()["current"], 2);
        super::super::skill_history::restore(&store, "test-bot", &improved.id, 1, 2).unwrap();
        assert_eq!(std::fs::read(improved.dir().join("SKILL.md")).unwrap(), before);
        super::super::remove_skill(&store, &original.id).unwrap();
        super::super::remove_skill(&store, &improved.id).unwrap();
    }
}
