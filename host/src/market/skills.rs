//! Skills: the anthropics/skills catalog, downloads, hand-written skills and the profile's skills block.

use super::{SKILLS_REPO, Skill, TIMEOUT, UA, get_json, save, skills, skills_dir, slug, unique_id};
use crate::LockExt;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SKILL_CATALOG: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

/// `name` and `description` from a SKILL.md front matter.
fn front_matter(md: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut description = None;
    let mut lines = md.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let mut lines = lines.take_while(|l| l.trim() != "---").peekable();
    while let Some(line) = lines.next() {
        let Some((key, v)) = line.split_once(':').filter(|(k, _)| matches!(*k, "name" | "description")) else {
            continue;
        };
        let mut v = v.trim().trim_matches('"').to_owned();
        // A YAML block (`>`, `|-`, …): the indented lines under it.
        if matches!(v.as_str(), ">" | "|" | ">-" | "|-" | ">+" | "|+") {
            let mut parts = vec![];
            while let Some(next) = lines.next_if(|l| l.starts_with([' ', '\t']) || l.trim().is_empty()) {
                parts.push(next.trim());
            }
            v = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        }
        if key == "name" { name = Some(v) } else { description = Some(v) }
    }
    (name, description)
}

/// `SKILL.md` without its front matter: what a skill tells the agent to do.
fn body(md: &str) -> &str {
    let mut lines = md.split_inclusive('\n');
    if lines.next().map(str::trim) != Some("---") {
        return md.trim();
    }
    let mut at = md.find('\n').map_or(md.len(), |i| i + 1);
    for line in lines {
        at += line.len();
        if line.trim() == "---" {
            return md[at..].trim();
        }
    }
    ""
}

/// Skills published in anthropics/skills, with their `SKILL.md` body (cached for an hour).
async fn catalog() -> Result<Value> {
    let cached = SKILL_CATALOG
        .locked()
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(3600))
        .map(|(_, v)| v.clone());
    let catalog = if let Some(v) = cached {
        v
    } else {
        let dirs = get_json(&format!("https://api.github.com/repos/{SKILLS_REPO}/contents/skills"))
            .await
            .context("listing skills on GitHub")?;
        let names: Vec<String> = dirs
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["type"] == "dir")
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect();
        let fetches = names.iter().map(|dir| async move {
            let url = format!("https://raw.githubusercontent.com/{SKILLS_REPO}/main/skills/{dir}/SKILL.md");
            let md = crate::http().get(&url).header("user-agent", UA).timeout(TIMEOUT).send().await.ok()?.text().await.ok()?;
            let (name, description) = front_matter(&md);
            Some(json!({"source": dir, "name": name.unwrap_or_else(|| dir.clone()), "description": description.unwrap_or_default(), "body": body(&md)}))
        });
        let v = Value::Array(futures::future::join_all(fetches).await.into_iter().flatten().collect());
        *SKILL_CATALOG.locked() = Some((Instant::now(), v.clone()));
        v
    };
    Ok(catalog)
}

/// The anthropics/skills catalog for the Marketplace (bodies come one at a time from `skill_details`).
pub async fn browse_skills(store: &crate::store::Store) -> Result<Value> {
    let installed: Vec<String> = skills(store).into_iter().map(|s| s.id).collect();
    let items: Vec<Value> = catalog()
        .await?
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let mut s = s.clone();
            if let Some(o) = s.as_object_mut() {
                o.remove("body");
            }
            s["installed"] = json!(installed.contains(&slug(s["source"].as_str().unwrap_or_default())));
            s
        })
        .collect();
    Ok(json!({"items": items}))
}

/// A skill's instructions for its details sheet: an installed one by `id` (read from disk),
/// a catalog one by `source`. Only names from those lists are looked up, never a path.
pub async fn skill_details(store: &crate::store::Store, b: &Value) -> Result<Value> {
    const MAX: usize = 256 * 1024;
    let mut markdown = if let Some(id) = b["id"].as_str() {
        let s = skills(store).into_iter().find(|s| s.id == id).ok_or_else(|| anyhow!("Unknown skill"))?;
        let md = tokio::fs::read_to_string(s.dir().join("SKILL.md")).await.context("reading the skill")?;
        body(&md).to_owned()
    } else {
        let source = b["source"].as_str().ok_or_else(|| anyhow!("`id` or `source` is required"))?;
        let catalog = catalog().await?;
        let s = catalog.as_array().into_iter().flatten().find(|s| s["source"] == source);
        s.ok_or_else(|| anyhow!("Unknown skill"))?["body"].as_str().unwrap_or_default().to_owned()
    };
    if markdown.len() > MAX {
        let mut end = MAX;
        while !markdown.is_char_boundary(end) {
            end -= 1;
        }
        markdown.truncate(end);
        markdown.push_str("\n\n…");
    }
    Ok(json!({"markdown": markdown}))
}

/// Downloads a skill folder from anthropics/skills (files up to 1 MB, two levels deep).
async fn download_dir(repo_path: &str, dest: &std::path::Path, depth: u8) -> Result<()> {
    let listing = get_json(&format!("https://api.github.com/repos/{SKILLS_REPO}/contents/{repo_path}")).await?;
    tokio::fs::create_dir_all(dest).await?;
    for item in listing.as_array().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default();
        // Names become paths: plain file names only.
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            continue;
        }
        match item["type"].as_str() {
            Some("file") if item["size"].as_u64().unwrap_or(0) <= 1_000_000 => {
                let Some(url) = item["download_url"].as_str() else { continue };
                let bytes =
                    crate::http().get(url).header("user-agent", UA).timeout(TIMEOUT).send().await?.bytes().await?;
                tokio::fs::write(dest.join(name), &bytes).await?;
            }
            Some("dir") if depth > 0 => {
                Box::pin(download_dir(&format!("{repo_path}/{name}"), &dest.join(name), depth - 1)).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub async fn install_skill(store: &crate::store::Store, source: &str) -> Result<Value> {
    let id = slug(source);
    let mut all = skills(store);
    if let Some(s) = all.iter().find(|s| s.id == id) {
        return Ok(s.public());
    }
    let dest = skills_dir().join(&id);
    download_dir(&format!("skills/{source}"), &dest, 2).await.context("downloading the skill")?;
    let md = tokio::fs::read_to_string(dest.join("SKILL.md")).await.context("the skill has no SKILL.md")?;
    let (name, description) = front_matter(&md);
    let s = Skill {
        id,
        name: name.unwrap_or_else(|| source.to_owned()),
        description: description.unwrap_or_default(),
        source: SKILLS_REPO.into(),
    };
    all.push(s.clone());
    save(store, "skills", &all)?;
    Ok(s.public())
}

/// A skill you write yourself: a name, when to use it, and the instructions.
pub fn add_custom_skill(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let description = b["description"].as_str().unwrap_or_default().trim().replace('\n', " ");
    let body = b["instructions"].as_str().unwrap_or_default();
    let mut all = skills(store);
    let s = Skill {
        id: unique_id(name, &all.iter().map(|s| s.id.clone()).collect::<Vec<_>>()),
        name: name.to_owned(),
        description,
        source: "custom".into(),
    };
    std::fs::create_dir_all(s.dir())?;
    std::fs::write(
        s.dir().join("SKILL.md"),
        format!("---\nname: {}\ndescription: {}\n---\n\n{body}\n", s.name, s.description),
    )?;
    all.push(s.clone());
    save(store, "skills", &all)?;
    Ok(s.public())
}

pub fn list_skills(store: &crate::store::Store) -> Value {
    json!({"items": skills(store).iter().map(Skill::public).collect::<Vec<_>>()})
}

pub fn remove_skill(store: &crate::store::Store, id: &str) -> Result<()> {
    let mut all = skills(store);
    if let Some(s) = all.iter().find(|s| s.id == id) {
        let _ = std::fs::remove_dir_all(s.dir());
    }
    all.retain(|s| s.id != id);
    save(store, "skills", &all)
}

/// `/name rest` → `name` and the rest; the rest is `None` while the name is still being typed.
pub fn slash_command(text: &str) -> Option<(&str, Option<&str>)> {
    let body = text.strip_prefix('/')?;
    Some(match body.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, Some(rest)),
        None => (body, None),
    })
}

/// A message that starts with `/<skill id>`, for a skill the bot has on, asks for that skill:
/// the agent gets told to read it first. `None` sends the message as written.
pub fn expand_command(store: &crate::store::Store, enabled: &[String], text: &str) -> Option<String> {
    let (id, rest) = slash_command(text)?;
    if !enabled.iter().any(|e| e == id) {
        return None;
    }
    let s = skills(store).into_iter().find(|s| s.id == id)?;
    let note = format!(
        "[Codync: the user picked the \"{}\" skill for this message. Read {} first and follow it.]",
        s.name,
        s.dir().join("SKILL.md").display()
    );
    Some(match rest.map(str::trim).filter(|r| !r.is_empty()) {
        Some(task) => format!("{note}\n\n{task}"),
        None => note,
    })
}

/// Skills used to start off for every bot; installs now turn them on everywhere, so the ones
/// installed before that are turned on once too.
pub fn turn_on_installed_once(store: &crate::store::Store) -> Result<()> {
    const DONE: &str = "skillsOnForEveryBot";
    if store.kv_get(DONE).is_some() {
        return Ok(());
    }
    let ids: Vec<String> = skills(store).into_iter().map(|s| s.id).collect();
    for row in store.bots()? {
        let mut cfg = row.config;
        if row.deleted || cfg.is_group() || ids.iter().all(|id| cfg.skills.contains(id)) {
            continue;
        }
        for id in &ids {
            if !cfg.skills.contains(id) {
                cfg.skills.push(id.clone());
            }
        }
        store.save_bot(&cfg)?;
    }
    store.kv_set(DONE, "1")
}

/// The skills block of a bot's profile: what each is for and where to read it.
pub fn skills_brief(store: &crate::store::Store, enabled: &[String]) -> Option<String> {
    let lines: Vec<String> = skills(store)
        .iter()
        .filter(|s| enabled.contains(&s.id))
        .map(|s| {
            format!("- {}: {} (read {} before using it)", s.name, s.description, s.dir().join("SKILL.md").display())
        })
        .collect();
    (!lines.is_empty()).then(|| format!("Skills you can use when they fit the task:\n{}", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_is_read() {
        let md = "---\nname: pdf\ndescription: \"Work with PDFs\"\n---\n# PDF";
        assert_eq!(front_matter(md), (Some("pdf".into()), Some("Work with PDFs".into())));
        assert_eq!(front_matter("# no front matter"), (None, None));
        let folded = "---\nname: guide\ndescription: >\n  Walks you\n  through it.\nlicense: MIT\n---\n";
        assert_eq!(front_matter(folded), (Some("guide".into()), Some("Walks you through it.".into())));
    }

    #[test]
    fn the_body_leaves_out_the_front_matter() {
        assert_eq!(body("---\nname: pdf\ndescription: x\n---\n\n# PDF\nUse it.\n"), "# PDF\nUse it.");
        assert_eq!(body("# No front matter\n"), "# No front matter");
        assert_eq!(body("---\nname: open\n"), "", "an unclosed front matter has no body");
    }

    #[test]
    fn slash_commands_name_an_enabled_skill() {
        assert_eq!(slash_command("/pd"), Some(("pd", None)));
        assert_eq!(slash_command("/pdf merge\nthese"), Some(("pdf", Some("merge\nthese"))));
        assert_eq!(slash_command("hi /pdf"), None);

        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let pdf = Skill { id: "pdf".into(), name: "PDF".into(), description: String::new(), source: "custom".into() };
        save(&store, "skills", std::slice::from_ref(&pdf)).expect("save skills");
        let on = ["pdf".to_owned()];
        let asked = expand_command(&store, &on, "/pdf  merge these ").expect("an enabled skill");
        assert!(asked.starts_with("[Codync: the user picked the \"PDF\" skill"), "{asked}");
        assert!(asked.contains(&pdf.dir().join("SKILL.md").display().to_string()));
        assert!(asked.ends_with("]\n\nmerge these"), "{asked}");
        assert!(expand_command(&store, &on, "/pdf").expect("no task").ends_with("follow it.]"));
        assert_eq!(expand_command(&store, &[], "/pdf merge"), None, "a skill the bot has off");
        assert_eq!(expand_command(&store, &on, "/compact"), None, "the agent's own commands pass through");
        assert_eq!(expand_command(&store, &on, "/pdfx"), None);
    }

    #[test]
    fn installed_skills_turn_on_once() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let pdf = Skill { id: "pdf".into(), name: "PDF".into(), description: String::new(), source: "custom".into() };
        save(&store, "skills", &[pdf]).expect("save skills");
        let mut cfg: crate::store::BotConfig = serde_json::from_value(
            json!({"id": "a", "name": "a", "backend": "fixture", "command": "true", "cwd": "/tmp", "notify": false}),
        )
        .expect("bot config");
        store.save_bot(&cfg).expect("save bot");
        let skills_of = |store: &crate::store::Store| store.bot("a").expect("read").expect("bot").config.skills;
        turn_on_installed_once(&store).expect("turn on");
        assert_eq!(skills_of(&store), ["pdf"]);
        cfg.skills.clear();
        store.save_bot(&cfg).expect("save bot");
        turn_on_installed_once(&store).expect("second run");
        assert!(skills_of(&store).is_empty(), "a skill turned off afterwards stays off");
    }
}
