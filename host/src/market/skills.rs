//! Skills: the anthropics/skills catalog, downloads, hand-written skills and the profile's skills block.

use super::{SKILLS_REPO, Skill, TIMEOUT, UA, save, skills, skills_dir, slug, unique_id};
use crate::LockExt;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SKILL_CATALOG: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
static SKILLS_LOCK: Mutex<()> = Mutex::new(());

/// `Skill::source` of a skill written in Codync rather than downloaded.
const CUSTOM: &str = "custom";

/// `name` and `description` from a SKILL.md front matter, including a folded `description: >` block.
pub(crate) fn front_matter(md: &str) -> (Option<String>, Option<String>) {
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
pub(crate) fn body(md: &str) -> &str {
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

/// One entry on a public GitHub tree page.
struct TreeEntry {
    name: String,
    path: String,
    dir: bool,
}

/// Folders and files on the public tree page. The contents API shares a 60-request hour
/// with every other unauthenticated call from this computer, and answers 403 once that
/// quota is spent, so the catalog never uses it.
async fn tree_entries(repo_path: &str) -> Result<Vec<TreeEntry>> {
    let url = format!("https://github.com/{SKILLS_REPO}/tree/main/{repo_path}");
    // github.com sometimes answers the first handshake with a non-TLS record. One try
    // then surfaces as the red error under the list, so the catalog tries again.
    let mut last = anyhow!("listing skills on GitHub");
    for attempt in 0u64..3 {
        match fetch_tree_page(&url).await {
            Ok(html) => return parse_tree_entries(&html).context("listing skills on GitHub"),
            Err(e) if attempt < 2 => {
                last = e;
                tokio::time::sleep(Duration::from_millis(300 * (attempt + 1))).await;
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn fetch_tree_page(url: &str) -> Result<String> {
    let response = crate::http()
        .get(url)
        .header("user-agent", UA)
        .timeout(TIMEOUT)
        .send()
        .await
        .context("listing skills on GitHub")?;
    response.text().await.context("listing skills on GitHub")
}

/// `items` in the tree page's embedded catalog: name, path, and whether it is a directory.
fn parse_tree_entries(html: &str) -> Result<Vec<TreeEntry>> {
    const MARK: &str = r#"data-target="react-app.embeddedData">"#;
    let start = html.find(MARK).context("the skill list page has no catalog")?;
    let json = &html[start + MARK.len()..];
    let end = json.find("</script>").context("the skill list page has no catalog")?;
    let value: Value = serde_json::from_str(&json[..end]).context("the skill list page")?;
    let items = value["payload"]["codeViewTreeRoute"]["tree"]["items"]
        .as_array()
        .context("the skill list page has no catalog")?
        .clone();
    Ok(items
        .into_iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?.to_owned();
            let path = item["path"].as_str()?.to_owned();
            // Names become paths: plain file names only.
            if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
                return None;
            }
            Some(TreeEntry { name, path, dir: item["contentType"] == "directory" })
        })
        .collect())
}

/// Skills published in anthropics/skills, with their `SKILL.md` body (cached for an hour).
async fn catalog() -> Result<Value> {
    let cached = SKILL_CATALOG
        .locked()
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_hours(1))
        .map(|(_, v)| v.clone());
    let catalog = if let Some(v) = cached {
        v
    } else {
        let names: Vec<String> =
            tree_entries("skills").await?.into_iter().filter(|entry| entry.dir).map(|entry| entry.name).collect();
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
    let installed: Vec<String> = skills(store).into_iter().filter(|s| s.source == SKILLS_REPO).map(|s| s.id).collect();
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

const MAX_SKILL_FILE: usize = 1_000_000;

/// One skill file, or nothing when it is over [`MAX_SKILL_FILE`]. Reading stops at the limit.
async fn skill_file(url: &str) -> Result<Option<Vec<u8>>> {
    let mut response = crate::http().get(url).header("user-agent", UA).timeout(TIMEOUT).send().await?;
    if response.content_length().is_some_and(|size| size > MAX_SKILL_FILE as u64) {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > MAX_SKILL_FILE {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

/// Downloads a skill folder from anthropics/skills (files up to 1 MB, two levels deep).
async fn download_dir(repo_path: &str, dest: &std::path::Path, depth: u8) -> Result<()> {
    let listing = tree_entries(repo_path).await?;
    tokio::fs::create_dir_all(dest).await?;
    for entry in listing {
        // The page's path has to be the folder we asked for, not somewhere else.
        if entry.path != format!("{repo_path}/{}", entry.name) {
            continue;
        }
        if entry.dir {
            if depth > 0 {
                Box::pin(download_dir(&entry.path, &dest.join(&entry.name), depth - 1)).await?;
            }
            continue;
        }
        let url = format!("https://raw.githubusercontent.com/{SKILLS_REPO}/main/{}", entry.path);
        if let Some(bytes) = skill_file(&url).await? {
            tokio::fs::write(dest.join(entry.name), &bytes).await?;
        }
    }
    Ok(())
}

/// Read/modify/write of the installed list, one at a time, so a skill a bot saves while another
/// downloads can't drop either.
fn update_skills<T>(store: &crate::store::Store, change: impl FnOnce(&mut Vec<Skill>) -> Result<T>) -> Result<T> {
    let _guard = SKILLS_LOCK.locked();
    let mut all = skills(store);
    let result = change(&mut all)?;
    save(store, "skills", &all)?;
    Ok(result)
}

/// The downloaded skill already installed as `id`, or `None` once its folder is taken for the download (under the
/// lock, so a skill written meanwhile picks another id). A written skill that has the id blocks the download.
fn reserve_download(store: &crate::store::Store, id: &str) -> Result<Option<Value>> {
    let _guard = SKILLS_LOCK.locked();
    match skills(store).into_iter().find(|s| s.id == id) {
        Some(s) if s.source == SKILLS_REPO => Ok(Some(s.public())),
        Some(s) => bail!("The skill {} written in Codync has the id {id}: remove it to add this one", s.name),
        None => {
            std::fs::create_dir_all(skills_dir().join(id))?;
            Ok(None)
        }
    }
}

pub async fn install_skill(store: &crate::store::Store, source: &str) -> Result<Value> {
    let id = slug(source);
    if let Some(installed) = reserve_download(store, &id)? {
        return Ok(installed);
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
        written_by: None,
    };
    update_skills(store, |all| {
        if !all.iter().any(|x| x.id == s.id) {
            all.push(s.clone());
        }
        Ok(s.public())
    })
}

/// A skill written in Codync (by the user, or by the bot `written_by`): a name, when to use it, and the instructions.
pub fn add_custom_skill(
    store: &crate::store::Store,
    name: &str,
    description: &str,
    body: &str,
    written_by: Option<&str>,
) -> Result<Skill> {
    let name = name.trim();
    if name.is_empty() {
        bail!("`name` is required");
    }
    update_skills(store, |all| {
        // A folder the list doesn't have yet (a download in progress) is taken too.
        let folders = std::fs::read_dir(skills_dir()).into_iter().flatten().flatten();
        let taken: Vec<String> =
            all.iter().map(|s| s.id.clone()).chain(folders.filter_map(|f| f.file_name().into_string().ok())).collect();
        let s = Skill {
            id: unique_id(name, &taken),
            name: one_line(name),
            description: one_line(description),
            source: CUSTOM.into(),
            written_by: written_by.map(str::to_owned),
        };
        write_skill_md(&s, body)?;
        all.push(s.clone());
        Ok(s)
    })
}

#[derive(Debug)]
pub(crate) enum SkillSaveError {
    Invalid(String),
    NotOwner(String),
    ParkedThreadOwner(String),
    Enable(anyhow::Error),
}

impl SkillSaveError {
    pub(crate) fn category(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid",
            Self::NotOwner(_) => "not_owner",
            Self::ParkedThreadOwner(_) => "parked_thread_owner",
            Self::Enable(_) => "enable",
        }
    }
}

impl fmt::Display for SkillSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::NotOwner(name) | Self::ParkedThreadOwner(name) => {
                write!(f, "{name} wasn't saved by you: only skills you saved can be changed")
            }
            Self::Enable(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SkillSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Enable(error) => error.source(),
            _ => None,
        }
    }
}

/// New text for a skill `bot` wrote; files beside its `SKILL.md` stay. The user's and other bots' skills stay as
/// they are.
pub fn update_custom_skill(
    store: &crate::store::Store,
    bot: &str,
    id: &str,
    name: &str,
    description: &str,
    body: &str,
) -> Result<Skill> {
    update_skills(store, |all| {
        let s = all
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| SkillSaveError::Invalid(format!("There's no skill with the id {id}")))?;
        let owner = s.written_by.as_deref().map(|writer| crate::chat::work::owner(store, writer));
        if owner.as_deref() != Some(bot) {
            let parked_owner = s.written_by.as_deref().is_some_and(|writer| {
                crate::store::is_thread_id(writer)
                    && matches!(store.bot(writer), Ok(None))
                    && matches!(store.parked_thread(writer), Ok(Some(cfg)) if cfg.thread_of.as_deref() == Some(bot))
            });
            return Err(if parked_owner {
                SkillSaveError::ParkedThreadOwner(s.name.clone())
            } else {
                SkillSaveError::NotOwner(s.name.clone())
            }
            .into());
        }
        s.written_by = Some(bot.to_owned());
        s.name = one_line(name);
        s.description = one_line(description);
        write_skill_md(s, body)?;
        Ok(s.clone())
    })
}

/// A name or description for the front matter, which takes one line each.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn write_skill_md(s: &Skill, body: &str) -> Result<()> {
    std::fs::create_dir_all(s.dir())?;
    std::fs::write(
        s.dir().join("SKILL.md"),
        format!("---\nname: {}\ndescription: {}\n---\n\n{}\n", s.name, s.description, body.trim()),
    )?;
    Ok(())
}

pub fn list_skills(store: &crate::store::Store) -> Value {
    json!({"items": skills(store).iter().map(Skill::public).collect::<Vec<_>>()})
}

pub fn remove_skill(store: &crate::store::Store, id: &str) -> Result<()> {
    update_skills(store, |all| {
        if let Some(s) = all.iter().find(|s| s.id == id) {
            let _ = std::fs::remove_dir_all(s.dir());
        }
        all.retain(|s| s.id != id);
        Ok(())
    })
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
    let ids = super::Installed::Skills.ids(store)?;
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

/// Turns on each skill saved by an old thread and gives its bot ownership in the same commit.
pub fn adopt_thread_skills(store: &crate::store::Store) -> Result<()> {
    #[cfg(test)]
    if let Ok(read) = tests::PARKED_THREAD_READER.try_with(|read| *read) {
        return adopt_thread_skills_with(store, |writer| read(store, writer));
    }
    adopt_thread_skills_with(store, |writer| store.parked_thread(writer))
}

fn adopt_thread_skills_with(
    store: &crate::store::Store,
    parked_thread: impl Fn(&str) -> Result<Option<crate::store::BotConfig>>,
) -> Result<()> {
    let _guard = SKILLS_LOCK.locked();
    let Some(raw) = store.kv_read("skills")? else { return Ok(()) };
    let mut all: Vec<Skill> = serde_json::from_str(&raw)?;
    for index in 0..all.len() {
        let Some(writer) = all[index].written_by.as_deref().filter(|w| crate::store::is_thread_id(w)) else {
            continue;
        };
        let owner = match store.bot(writer) {
            Ok(Some(row)) => row.config.thread_of,
            Ok(None) => match parked_thread(writer) {
                Ok(Some(cfg)) => cfg.thread_of,
                Ok(None) => None,
                Err(_) => {
                    // Archive decode errors can contain text from the record.
                    tracing::warn!("couldn't read a parked thread during skill adoption");
                    continue;
                }
            },
            Err(_) => {
                tracing::warn!("couldn't read an active thread during skill adoption");
                continue;
            }
        };
        let Some(owner) = owner else { continue };
        let previous = all[index].written_by.replace(owner.clone());
        let saved = serde_json::to_string(&all)
            .map_err(anyhow::Error::from)
            .and_then(|list| store.adopt_thread_skill(&owner, &all[index].id, &list));
        match saved {
            Ok(true) => continue,
            Ok(false) => {}
            Err(_) => tracing::warn!("couldn't commit a thread skill adoption"),
        }
        all[index].written_by = previous;
    }
    Ok(())
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
    (!lines.is_empty()).then(|| format!("Skills you can use when they fit the request:\n{}", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    tokio::task_local! {
        pub(super) static PARKED_THREAD_READER: fn(&crate::store::Store, &str) -> Result<Option<crate::store::BotConfig>>;
    }

    /// Serves `/declared` (a Content-Length over the limit), `/streamed` (chunks past the limit, no
    /// length) and anything else as a five-byte body.
    async fn file_server() -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a local port");
        let base = format!("http://{}", listener.local_addr().expect("a bound address"));
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = [0u8; 1024];
                    let read = socket.read(&mut request).await.unwrap_or(0);
                    let head = String::from_utf8_lossy(&request[..read]).into_owned();
                    // The client hangs up as soon as it sees too much; later writes may fail.
                    if head.starts_with("GET /declared") {
                        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2000000\r\n\r\n").await;
                    } else if head.starts_with("GET /streamed") {
                        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n").await;
                        let chunk = vec![b'x'; 65_536];
                        for _ in 0..17 {
                            let _ = socket.write_all(b"10000\r\n").await;
                            let _ = socket.write_all(&chunk).await;
                            let _ = socket.write_all(b"\r\n").await;
                        }
                        let _ = socket.write_all(b"0\r\n\r\n").await;
                    } else {
                        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello").await;
                    }
                });
            }
        });
        base
    }

    #[tokio::test]
    async fn a_skill_file_over_the_limit_is_skipped_without_reading_it_all() {
        let base = file_server().await;
        assert_eq!(skill_file(&format!("{base}/declared")).await.expect("declared"), None);
        assert_eq!(skill_file(&format!("{base}/streamed")).await.expect("streamed"), None);
        assert_eq!(skill_file(&format!("{base}/small")).await.expect("small").as_deref(), Some(&b"hello"[..]));
    }

    #[test]
    fn the_public_tree_page_lists_skill_folders() {
        let html = r#"<script type="application/json" data-target="react-app.embeddedData">{"payload":{"codeViewTreeRoute":{"tree":{"items":[
            {"name":"pdf","path":"skills/pdf","contentType":"directory"},
            {"name":"SKILL.md","path":"skills/pdf/SKILL.md","contentType":"file"},
            {"name":".claude-plugin","path":"skills/.claude-plugin","contentType":"directory"},
            {"name":"../etc","path":"skills/../etc","contentType":"file"}
        ]}}}}</script>"#;
        let entries = parse_tree_entries(html).expect("tree page");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].dir && entries[0].name == "pdf" && entries[0].path == "skills/pdf");
        assert!(!entries[1].dir && entries[1].name == "SKILL.md");
    }

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
        let pdf: Skill =
            serde_json::from_value(json!({"id": "pdf", "name": "PDF", "source": "custom"})).expect("skill");
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
        let pdf: Skill =
            serde_json::from_value(json!({"id": "pdf", "name": "PDF", "source": "custom"})).expect("skill");
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

    #[test]
    fn parked_thread_skills_keep_their_author_until_their_parent_can_adopt_them() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        let config = |id: &str, kind: &str| -> crate::store::BotConfig {
            serde_json::from_value(json!({"id": id, "name": id, "kind": kind, "backend": "fixture", "cwd": "/tmp"}))
                .unwrap()
        };
        store.save_bot(&config("group", "group")).unwrap();
        store.save_bot(&config("deleted", "agent")).unwrap();
        store.delete_bot("deleted").unwrap();
        store.save_bot(&config("unrelated", "agent")).unwrap();
        let legacy = ["missing", "group", "deleted"].map(|parent| {
            let mut thread = config(&format!("thread-{parent}"), "agent");
            thread.thread_of = Some(parent.into());
            store.save_bot(&thread).unwrap();
            store.park_thread(&thread).unwrap();
            Skill {
                id: format!("skill-{parent}"),
                name: parent.into(),
                description: "Old skill".into(),
                source: CUSTOM.into(),
                written_by: Some(thread.id),
            }
        });
        save(&store, "skills", &legacy).unwrap();

        adopt_thread_skills(&store).unwrap();
        assert_eq!(skills(&store), legacy, "ineligible parents leave each original author intact");
        for id in ["group", "deleted", "unrelated"] {
            assert!(store.bot(id).unwrap().unwrap().config.skills.is_empty(), "{id} received no skill");
        }

        store.save_bot(&config("missing", "agent")).unwrap();
        adopt_thread_skills(&store).unwrap();
        let after = skills(&store);
        assert_eq!(after[0].written_by.as_deref(), Some("missing"));
        assert_eq!(after[1..], legacy[1..]);
        assert_eq!(store.bot("missing").unwrap().unwrap().config.skills, ["skill-missing"]);
    }

    #[tokio::test]
    async fn startup_skips_an_unreadable_parked_thread_and_retries_its_skill_later() {
        use crate::hub::Hub;
        use crate::remote::identity::Identity;
        use crate::store::{BotConfig, Store};
        use std::sync::Arc;

        const SKILL_DESCRIPTION: &str = "sample private skill instructions";
        const ARCHIVE_PATH: &str = "/sample/private-archive";

        #[derive(Clone, Default)]
        struct Log(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for Log {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.locked().extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let dir = std::env::temp_dir().join(format!("codync-skill-adoption-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let config = |id: &str| -> BotConfig {
            serde_json::from_value(json!({"id": id, "name": id, "backend": "fixture", "cwd": &dir})).unwrap()
        };
        store.save_bot(&config("owner")).unwrap();
        turn_on_installed_once(&store).unwrap();
        let legacy = ["unreadable", "readable"].map(|id| {
            let mut thread = config(&format!("thread-{id}"));
            thread.thread_of = Some("owner".into());
            store.save_bot(&thread).unwrap();
            store.park_thread(&thread).unwrap();
            Skill {
                id: id.into(),
                name: id.into(),
                description: SKILL_DESCRIPTION.into(),
                source: CUSTOM.into(),
                written_by: Some(thread.id),
            }
        });
        save(&store, "skills", &legacy).unwrap();

        let hub =
            Hub::new(store, "skill-adoption-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(move || writer.clone()).finish();
        let read = |store: &Store, writer: &str| {
            if writer == "thread-unreadable" {
                bail!("couldn't decode an archive at {ARCHIVE_PATH}: {SKILL_DESCRIPTION}");
            }
            store.parked_thread(writer)
        };
        let started =
            PARKED_THREAD_READER.sync_scope(read, || tracing::subscriber::with_default(subscriber, || hub.start()));
        assert!(started.is_ok(), "one parked thread read failure does not fail startup");
        assert!(hub.has_actor("owner"));
        let after = skills(&hub.store);
        assert_eq!(after[0], legacy[0]);
        assert_eq!(after[1].written_by.as_deref(), Some("owner"));
        let enabled = || hub.store.bot("owner").unwrap().unwrap().config.skills;
        assert_eq!(enabled(), ["readable"]);
        let log = String::from_utf8(log.0.locked().clone()).unwrap();
        assert!(
            log.lines()
                .any(|line| line.contains("WARN")
                    && line.contains("couldn't read a parked thread during skill adoption"))
        );
        assert!(!log.contains(SKILL_DESCRIPTION) && !log.contains(ARCHIVE_PATH));

        adopt_thread_skills(&hub.store).unwrap();
        assert_eq!(enabled(), ["readable", "unreadable"]);
        assert_eq!(skills(&hub.store)[0].written_by.as_deref(), Some("owner"));

        hub.shutdown().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thread_owned_skills_are_adopted_by_their_bot() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let bot_cfg = |id: &str, thread_of: Option<&str>| -> crate::store::BotConfig {
            let mut cfg: crate::store::BotConfig = serde_json::from_value(
                json!({"id": id, "name": id, "backend": "fixture", "command": "true", "cwd": "/tmp", "notify": false}),
            )
            .expect("bot config");
            cfg.thread_of = thread_of.map(str::to_owned);
            cfg
        };
        store.save_bot(&bot_cfg("a", None)).expect("save a");
        store.save_bot(&bot_cfg("b", None)).expect("save b");
        store.save_bot(&bot_cfg("thread-1", Some("a"))).expect("save thread-1");
        // Even a soft-deleted thread row keeps its `threadOf` in `bots`.
        store.delete_bot("thread-1").expect("soft delete thread-1");

        let tag = uuid::Uuid::new_v4().simple().to_string();
        let orphan_id = format!("orphan-{}", &tag[..8]);
        let direct_id = format!("direct-{}", &tag[..8]);
        let list = vec![
            Skill {
                id: orphan_id.clone(),
                name: "Orphan".into(),
                description: "Saved in a thread".into(),
                source: CUSTOM.into(),
                written_by: Some("thread-1".into()),
            },
            Skill {
                id: direct_id.clone(),
                name: "Direct".into(),
                description: "Rewritten before restart".into(),
                source: CUSTOM.into(),
                written_by: Some("thread-1".into()),
            },
            Skill {
                id: "missing-thread".into(),
                name: "Missing".into(),
                description: "Thread row gone".into(),
                source: CUSTOM.into(),
                written_by: Some("thread-gone".into()),
            },
        ];
        save(&store, "skills", &list).expect("save skills");

        assert!(update_custom_skill(&store, "b", &direct_id, "Direct", "x", "body").is_err());
        let updated =
            update_custom_skill(&store, "a", &direct_id, "Direct", "x", "body").expect("owner updates orphan");
        assert_eq!(updated.written_by.as_deref(), Some("a"));
        let _ = std::fs::remove_dir_all(updated.dir());

        adopt_thread_skills(&store).expect("adopt");
        let after = skills(&store);
        let orphan = after.iter().find(|s| s.id == orphan_id).expect("orphan skill");
        assert_eq!(orphan.written_by.as_deref(), Some("a"));
        let missing = after.iter().find(|s| s.id == "missing-thread").expect("missing skill");
        assert_eq!(missing.written_by.as_deref(), Some("thread-gone"));

        let skills_of = |id: &str| store.bot(id).expect("read").expect("bot").config.skills;
        assert_eq!(skills_of("a"), [orphan_id.as_str()]);
        assert_eq!(skills_of("b"), Vec::<String>::new());

        let mut cfg = store.bot("a").expect("read").expect("bot").config;
        cfg.skills.clear();
        store.save_bot(&cfg).expect("save a without skill");
        adopt_thread_skills(&store).expect("second adopt");
        assert!(skills_of("a").is_empty(), "a skill turned off afterwards stays off");

        // Recovery after a mid-adoption interruption: bot `a` already had its skill enabled, `b` did not,
        // and `written_by` still pointed to the threads. Re-running finishes enabling `b` without duplicating `a`.
        store.save_bot(&bot_cfg("thread-2", Some("b"))).expect("save thread-2");
        let mut a_cfg = store.bot("a").expect("read").expect("bot").config;
        a_cfg.skills = vec!["partial-a".into()];
        store.save_bot(&a_cfg).expect("save a with partial skill");
        save(
            &store,
            "skills",
            &[
                Skill {
                    id: "partial-a".into(),
                    name: "Partial A".into(),
                    description: "Already enabled before crash".into(),
                    source: CUSTOM.into(),
                    written_by: Some("thread-1".into()),
                },
                Skill {
                    id: "pending-b".into(),
                    name: "Pending B".into(),
                    description: "Not yet enabled before crash".into(),
                    source: CUSTOM.into(),
                    written_by: Some("thread-2".into()),
                },
            ],
        )
        .expect("save interrupted skills");
        adopt_thread_skills(&store).expect("recover interrupted adoption");
        assert_eq!(skills_of("a"), ["partial-a"], "no duplicate when already enabled");
        assert_eq!(skills_of("b"), ["pending-b"], "remaining bot gets its skill on retry");
        let recovered = skills(&store);
        assert_eq!(recovered.iter().find(|s| s.id == "partial-a").unwrap().written_by.as_deref(), Some("a"));
        assert_eq!(recovered.iter().find(|s| s.id == "pending-b").unwrap().written_by.as_deref(), Some("b"));
    }
}
