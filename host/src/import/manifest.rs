//! The export's manifest, read into a [`Plan`]. Three formats share one shape: Codync's own
//! (`codync-import.json`, written by the export skill), the agent export other apps import from Grok Bot
//! (`openbot-import.json`) and those apps' own transfer file (`openbot-transfer.json`). A bot, group or
//! routine that doesn't check out is left out with a warning; the rest still imports.

use super::archive::Archive;
use super::schedule;
use crate::routines::Trigger;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Each manifest file and the `format` it must name.
const FORMATS: [(&str, &str); 3] = [
    ("codync-import.json", "codync-bot-import"),
    ("openbot-import.json", "openbot-agent-import"),
    ("openbot-transfer.json", "openbot-instance-transfer"),
];
const VERSION: u64 = 1;
const MAX_MANIFEST: u64 = 8 * 1024 * 1024;
const MAX_BOTS: usize = 200;
const MAX_ROUTINES: usize = 64;
const MAX_MEMORIES: usize = 512;
const MAX_MEMORY_CHARS: usize = 2_000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// The app the agents come from, as the file names it (`grok-bot`…).
    pub source: String,
    pub bots: Vec<Bot>,
    pub groups: Vec<Group>,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Bot {
    pub key: String,
    pub name: String,
    /// The bot's description: its role, then its instructions.
    pub description: String,
    /// The agent it ran on, when Codync knows it (the import keeps it only if it runs on this computer).
    pub backend: Option<String>,
    pub avatar: Option<String>,
    pub skills: Vec<String>,
    pub routines: Vec<Routine>,
    pub memories: Vec<String>,
    /// The folder copied into its workspace as `imported/`.
    pub files: Option<String>,
    pub section: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub key: String,
    pub name: String,
    pub description: String,
    /// Bot keys; the lead first.
    pub members: Vec<String>,
    pub routines: Vec<Routine>,
    pub memories: Vec<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub name: String,
    pub instruction: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAgent {
    key: String,
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    routines: Vec<Value>,
    #[serde(default)]
    one_time_routines: Vec<Value>,
    #[serde(default)]
    memories: Vec<RawMemory>,
    #[serde(default)]
    files: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChannel {
    key: String,
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    instructions: String,
    members: Vec<String>,
    #[serde(default)]
    lead: Option<String>,
    #[serde(default)]
    routines: Vec<Value>,
    #[serde(default)]
    memories: Vec<RawMemory>,
}

/// A memory is a string in the agent export and `{text}` in the transfer file.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawMemory {
    Text(String),
    Item { text: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRoutine {
    name: String,
    instruction: String,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    timezone: String,
    schedule: Value,
}

fn yes() -> bool {
    true
}

/// Reads whichever manifest `archive` holds (making its folder the root) and checks it.
pub fn read(archive: &mut Archive, now: i64) -> Result<Plan> {
    let Some((file, format)) = FORMATS.into_iter().find(|(file, _)| archive.rooted_at(file)) else {
        bail!("this isn't a bot export: it has no codync-import.json");
    };
    let manifest: Value = serde_json::from_slice(&archive.read(file, MAX_MANIFEST)?)
        .map_err(|e| anyhow::anyhow!("{file} isn't valid JSON: {e}"))?;
    if manifest["format"] != format {
        bail!("{file} names the format {}, not {format}", manifest["format"]);
    }
    if manifest["version"].as_u64().is_none_or(|v| v > VERSION) {
        bail!("this export is from a newer version: update Codync to import it");
    }
    let mut plan = Plan {
        source: manifest["source"]["app"]
            .as_str()
            .unwrap_or(if format == FORMATS[2].1 { "openbot" } else { "" })
            .into(),
        ..Plan::default()
    };
    let agents = manifest["agents"].as_array().map_or(&[][..], Vec::as_slice);
    for raw in agents.iter().take(MAX_BOTS) {
        match bot(archive, raw, now, &mut plan.warnings) {
            Ok(bot) if plan.bots.iter().any(|b| b.key == bot.key) => {
                plan.warnings.push(format!("Left out a second agent with the key {}", bot.key));
            }
            Ok(bot) => plan.bots.push(bot),
            Err(e) => plan.warnings.push(format!("Left out the agent {}: {e:#}", label(raw))),
        }
    }
    sections(&manifest["settings"]["sidebarSections"], &mut plan.bots);
    for raw in manifest["channels"].as_array().into_iter().flatten().take(MAX_BOTS) {
        match group(raw, &plan.bots, now, &mut plan.warnings) {
            Ok(group) => plan.groups.push(group),
            Err(e) => plan.warnings.push(format!("Left out the group chat {}: {e:#}", label(raw))),
        }
    }
    if plan.bots.is_empty() {
        bail!("the export has no agent Codync can import{}", first_warning(&plan.warnings));
    }
    Ok(plan)
}

fn first_warning(warnings: &[String]) -> String {
    warnings.first().map(|w| format!(" ({w})")).unwrap_or_default()
}

fn label(raw: &Value) -> String {
    raw["name"].as_str().or_else(|| raw["key"].as_str()).unwrap_or("without a name").to_owned()
}

fn bot(archive: &Archive, raw: &Value, now: i64, warnings: &mut Vec<String>) -> Result<Bot> {
    let raw: RawAgent = serde_json::from_value(raw.clone())?;
    let key = checked_key(&raw.key)?;
    let name = one_line(&raw.name);
    if name.is_empty() {
        bail!("it has no name");
    }
    let own = |path: Option<String>, what: &str, warnings: &mut Vec<String>| {
        let path = path.map(|p| p.trim_end_matches('/').to_owned()).filter(|p| !p.is_empty())?;
        if path.starts_with(&format!("agents/{key}/")) {
            Some(path)
        } else {
            warnings.push(format!("{name}: left out its {what} {path}, which isn't in its own folder"));
            None
        }
    };
    let avatar = own(raw.avatar, "avatar", warnings).filter(|p| archive.has(p));
    let files = own(raw.files, "files", warnings).filter(|p| !archive.under(p).is_empty());
    let mut skills = Vec::new();
    for path in raw.skills {
        let Some(path) = own(Some(path), "skill", warnings) else { continue };
        if archive.has(&format!("{path}/SKILL.md")) {
            skills.push(path);
        } else {
            warnings.push(format!("{name}: left out the skill {path}, which has no SKILL.md"));
        }
    }
    let routines = raw.routines.iter().chain(&raw.one_time_routines);
    Ok(Bot {
        description: described(&raw.title, &raw.description),
        backend: raw.provider.filter(|p| crate::agent::backends::is_known(p)),
        avatar,
        skills,
        routines: routines.take(MAX_ROUTINES).filter_map(|r| routine(&name, r, now, warnings)).collect(),
        memories: memories(raw.memories, MAX_MEMORIES),
        files,
        section: None,
        key,
        name,
    })
}

fn group(raw: &Value, bots: &[Bot], now: i64, warnings: &mut Vec<String>) -> Result<Group> {
    let raw: RawChannel = serde_json::from_value(raw.clone())?;
    let key = checked_key(&raw.key)?;
    let name = one_line(&raw.name);
    if name.is_empty() {
        bail!("it has no name");
    }
    let mut members: Vec<String> = Vec::new();
    for member in raw.lead.iter().chain(&raw.members) {
        if bots.iter().any(|b| &b.key == member) && !members.contains(member) {
            members.push(member.clone());
        }
    }
    if members.is_empty() {
        bail!("none of its agents are in the export");
    }
    Ok(Group {
        description: described(&raw.title, &raw.instructions),
        routines: raw.routines.iter().take(MAX_ROUTINES).filter_map(|r| routine(&name, r, now, warnings)).collect(),
        memories: memories(raw.memories, MAX_MEMORIES),
        members,
        key,
        name,
    })
}

/// A routine Codync can run, or a warning that says why it's left out.
fn routine(owner: &str, raw: &Value, now: i64, warnings: &mut Vec<String>) -> Option<Routine> {
    let parsed = serde_json::from_value::<RawRoutine>(raw.clone()).map_err(anyhow::Error::from).and_then(|r| {
        let zone = Some(r.timezone.trim())
            .filter(|z| z.parse::<chrono_tz::Tz>().is_ok())
            .map_or_else(crate::routines::local_zone, str::to_owned);
        let trigger = schedule::trigger(&r.schedule, &zone, now)?;
        let (name, instruction) = (one_line(&r.name), r.instruction.trim().to_owned());
        if name.is_empty() || instruction.is_empty() {
            bail!("it has no name or instruction");
        }
        Ok(Routine { name, instruction, enabled: r.active, triggers: vec![trigger] })
    });
    parsed.map_err(|e| warnings.push(format!("{owner}: left out the routine {}: {e:#}", label(raw)))).ok()
}

/// The role, then the instructions.
fn described(title: &str, instructions: &str) -> String {
    [title.trim(), instructions.trim()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n")
}

fn memories(raw: Vec<RawMemory>, max: usize) -> Vec<String> {
    raw.into_iter()
        .map(|m| match m {
            RawMemory::Text(text) | RawMemory::Item { text } => one_line(&text),
        })
        .filter(|t| !t.is_empty() && t.chars().count() <= MAX_MEMORY_CHARS)
        .take(max)
        .collect()
}

/// The transfer file's sidebar sections, onto the bots they hold.
fn sections(raw: &Value, bots: &mut [Bot]) {
    for section in raw.as_array().into_iter().flatten() {
        let name = one_line(section["name"].as_str().unwrap_or_default());
        for key in section["agentKeys"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if let Some(bot) = bots.iter_mut().find(|b| b.key == key).filter(|_| !name.is_empty()) {
                bot.section = Some(name.clone());
            }
        }
    }
}

/// Lowercase letters, digits and inner hyphens, at most 64: safe as a folder name in the archive's paths.
fn checked_key(key: &str) -> Result<String> {
    let ok = !key.is_empty()
        && key.len() <= 64
        && !key.starts_with('-')
        && !key.ends_with('-')
        && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !ok {
        bail!("its key `{key}` isn't lowercase letters, digits and hyphens");
    }
    Ok(key.to_owned())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
