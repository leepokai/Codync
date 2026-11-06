//! Bots from another app: an export file (zip) read on this computer and turned into bots and groups with
//! their instructions, avatars, skills, routines, memory and workspace files. Chats aren't carried over; the
//! export keeps what matters in them as memories. The export skill (`EXPORT_SKILL`) writes Codync's own format
//! from Grok Bot; `manifest` also reads the exports other apps take from Grok Bot and their own transfer files.

mod archive;
mod manifest;
mod schedule;
#[cfg(test)]
mod tests;

use crate::hub::Hub;
use crate::market::{self, Installed};
use crate::store::{AVATAR_COLORS, AVATAR_SHAPES};
use anyhow::{Context, Result, bail};
use archive::Archive;
use base64::Engine as _;
use manifest::{Bot, Plan};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The skill a Grok Bot agent runs to write the export file.
pub const EXPORT_SKILL: &str = include_str!("export-skill.md");

const MAX_AVATAR: u64 = 5 * 1024 * 1024;
const MAX_SKILL_MD: u64 = 256 * 1024;

/// What the export at `path` holds, for the user to pick from.
pub async fn preview(b: &Value) -> Result<Value> {
    let path = export_path(b)?;
    let (plan, skipped) = tokio::task::spawn_blocking(move || {
        let mut archive = Archive::open(&path)?;
        anyhow::Ok((manifest::read(&mut archive, crate::store::now_ms())?, archive.skipped))
    })
    .await??;
    Ok(json!({"plan": plan, "skipped": skipped}))
}

/// Imports the bots (`bots`: their keys, default all) and groups (`groups`, default all) of the export at
/// `path`. A bot whose import fails is removed again and the others carry on.
pub async fn apply(hub: &Arc<Hub>, b: &Value) -> Result<Value> {
    let path = export_path(b)?;
    let picked = |key: &str| -> Option<Vec<String>> { serde_json::from_value(b[key].clone()).ok() };
    let (bots, groups) = (picked("bots"), picked("groups"));
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || {
        let mut archive = Archive::open(&path)?;
        let mut plan = manifest::read(&mut archive, crate::store::now_ms())?;
        plan.bots.retain(|bot| bots.as_ref().is_none_or(|keys| keys.contains(&bot.key)));
        plan.groups.retain(|group| groups.as_ref().is_none_or(|keys| keys.contains(&group.key)));
        if plan.bots.is_empty() {
            bail!("pick at least one agent to import");
        }
        let agents = crate::agent::backends::list()
            .into_iter()
            .filter(|a| a["available"] == true)
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect();
        Ok(Importer { hub, archive, agents, warnings: std::mem::take(&mut plan.warnings) }.run(&plan))
    })
    .await?
}

fn export_path(b: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(b["path"].as_str().context("`path` is required")?);
    if !path.is_absolute() || !path.is_file() {
        bail!("there's no file at {}", path.display());
    }
    Ok(path)
}

struct Importer {
    hub: Arc<Hub>,
    archive: Archive,
    /// The agents that run on this computer.
    agents: HashSet<String>,
    warnings: Vec<String>,
}

impl Importer {
    fn run(mut self, plan: &Plan) -> Value {
        let (mut bots, mut ids) = (Vec::new(), HashMap::new());
        for (i, bot) in plan.bots.iter().enumerate() {
            match self.bot(bot, i) {
                Ok(done) => {
                    ids.insert(bot.key.clone(), done["id"].as_str().unwrap_or_default().to_owned());
                    bots.push(done);
                }
                Err(e) => self.warnings.push(format!("Couldn't import {}: {e:#}", bot.name)),
            }
        }
        let mut groups = Vec::new();
        for group in &plan.groups {
            let members: Vec<String> = group.members.iter().filter_map(|k| ids.get(k).cloned()).collect();
            let Some(first) = members.first().cloned() else { continue };
            match self.group(group, &members) {
                Ok(Some(id)) => groups.push(json!({"key": group.key, "id": id, "name": group.name})),
                // The same bots share one group: the routines and memory still go to its lead.
                Ok(None) => self.warnings.push(format!(
                    "The group chat {} has the same bots as one you have, so it joins that one",
                    group.name
                )),
                Err(e) => {
                    self.warnings.push(format!("Couldn't import the group chat {}: {e:#}", group.name));
                    continue;
                }
            }
            // Routines and memory belong to a bot in Codync: the group's go to its lead (or first bot).
            for routine in &group.routines {
                let name = format!("{}: {}", group.name, routine.name);
                self.routine(&first, &name, routine);
            }
            self.memories(&first, &group.name, &group.memories);
        }
        json!({"source": plan.source, "bots": bots, "groups": groups, "warnings": self.warnings})
    }

    /// One bot: made first, then filled; removed again if a step it can't do without fails.
    fn bot(&mut self, bot: &Bot, index: usize) -> Result<Value> {
        let backend = bot.backend.as_ref().filter(|agent| self.agents.contains(*agent));
        if let (Some(agent), None) = (&bot.backend, backend) {
            self.warnings
                .push(format!("{} ran on {agent}, which isn't on this computer, so it gets the default", bot.name));
        }
        let id = self.create(&json!({
            "name": bot.name,
            "description": bot.description,
            "backend": backend,
            "section": bot.section,
            "avatarColor": AVATAR_COLORS[index % AVATAR_COLORS.len()],
            "avatarShape": AVATAR_SHAPES[index % AVATAR_SHAPES.len()],
        }))?;
        let filled = self.fill(&id, bot);
        if filled.is_err()
            && let Err(e) = self.hub.delete_bot(&id)
        {
            tracing::warn!(bot = %id, error = format!("{e:#}"), "couldn't remove a bot whose import failed");
        }
        let (skills, files) = filled?;
        let routines = bot.routines.iter().filter(|r| self.routine(&id, &r.name, r)).count();
        let memories = self.memories(&id, &bot.name, &bot.memories);
        if let Some(avatar) = &bot.avatar {
            self.avatar(&id, &bot.name, avatar);
        }
        Ok(json!({
            "key": bot.key, "id": id, "name": bot.name,
            "skills": skills, "routines": routines, "memories": memories, "files": files,
        }))
    }

    fn create(&self, b: &Value) -> Result<String> {
        let runtime = tokio::runtime::Handle::current();
        let cfg = runtime.block_on(crate::agent::new_bot::with_defaults(&self.hub, b.clone()))?;
        let made = self.hub.create_bot(cfg)?;
        Ok(made["id"].as_str().context("the new bot has an id")?.to_owned())
    }

    /// A new group of `members`, or None when a group of the same bots exists (creating would open that one).
    fn group(&self, group: &manifest::Group, members: &[String]) -> Result<Option<String>> {
        if crate::chat::group::with_members(&self.hub.store, members)?.is_some() {
            return Ok(None);
        }
        let b = json!({"kind": "group", "name": group.name, "description": group.description, "members": members});
        self.create(&b).map(Some)
    }

    /// Its skills and workspace files: how many of each. On a failure, those it added go again.
    fn fill(&mut self, id: &str, bot: &Bot) -> Result<(usize, usize)> {
        let cwd = self.hub.store.bot(id)?.context("the new bot")?.config.cwd;
        let imported = Path::new(&cwd).join("imported");
        let mut skills = Vec::new();
        let files = self.fill_into(id, bot, &imported, &mut skills);
        if files.is_err() {
            for skill in &skills {
                if let Err(e) = market::remove_skill(&self.hub.store, skill) {
                    tracing::warn!(skill, error = format!("{e:#}"), "couldn't remove a skill whose import failed");
                }
            }
            if let Err(e) = std::fs::remove_dir_all(&imported)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %e, "couldn't remove files whose import failed");
            }
        }
        Ok((skills.len(), files?))
    }

    /// `fill`'s steps; `skills` collects the ids of the skills it adds.
    fn fill_into(&mut self, id: &str, bot: &Bot, imported: &Path, skills: &mut Vec<String>) -> Result<usize> {
        for dir in &bot.skills {
            skills.push(self.skill(id, dir).with_context(|| format!("the skill {dir}"))?);
        }
        market::turn_on(&self.hub, Installed::Skills, skills, Some(id))?;
        match &bot.files {
            Some(dir) => self.archive.extract(dir, imported),
            None => Ok(0),
        }
    }

    /// A skill folder as a skill of that bot's own (on for it only); returns its id.
    fn skill(&mut self, id: &str, dir: &str) -> Result<String> {
        let md = String::from_utf8(self.archive.read(&format!("{dir}/SKILL.md"), MAX_SKILL_MD)?)
            .context("SKILL.md isn't UTF-8 text")?;
        let (name, description) = market::front_matter(&md);
        let name = name.unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(dir).to_owned());
        let skill = market::add_custom_skill(
            &self.hub.store,
            &name,
            &description.unwrap_or_default(),
            market::skill_body(&md),
            Some(id),
        )?;
        // The whole folder (scripts, references), its own SKILL.md over the one just written from it.
        if let Err(e) = self.archive.extract(dir, &skill.dir()) {
            market::remove_skill(&self.hub.store, &skill.id)?;
            return Err(e);
        }
        Ok(skill.id)
    }

    fn routine(&mut self, id: &str, name: &str, routine: &manifest::Routine) -> bool {
        let body = json!({
            "name": name, "instruction": routine.instruction, "enabled": routine.enabled, "triggers": routine.triggers,
        });
        let saved = self.hub.routines.save(&self.hub, id, &body);
        if let Err(e) = &saved {
            self.warnings.push(format!("Couldn't add the routine {name}: {e:#}"));
        }
        saved.is_ok()
    }

    fn memories(&mut self, id: &str, owner: &str, facts: &[String]) -> usize {
        if facts.is_empty() {
            return 0;
        }
        match crate::chat::memory::add_facts(&self.hub, id, facts) {
            Ok(written) => written,
            Err(e) => {
                self.warnings.push(format!("Couldn't add {owner}'s memories: {e:#}"));
                0
            }
        }
    }

    /// The avatar image as the bot's custom icon (which needs this computer signed in to Codync).
    fn avatar(&mut self, id: &str, name: &str, path: &str) {
        let set = self.archive.read(path, MAX_AVATAR).and_then(|bytes| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            let b = json!({"target": format!("bot:{id}"), "data": data});
            tokio::runtime::Handle::current().block_on(crate::icons::set(&self.hub, &b))
        });
        if let Err(e) = set {
            self.warnings.push(format!("{name} keeps a Codync avatar: {e:#}"));
        }
    }
}
