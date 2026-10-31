//! Skills: the anthropics/skills catalog, downloads, hand-written skills and the profile's skills block.

use super::{SKILLS_REPO, Skill, TIMEOUT, UA, save, skills, skills_dir, slug, unique_id};
use crate::LockExt;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
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
        let s = all.iter_mut().find(|s| s.id == id).ok_or_else(|| anyhow!("There's no skill with the id {id}"))?;
        if s.written_by.as_deref() != Some(bot) {
            bail!("{} wasn't saved by you: only skills you saved can be changed", s.name);
        }
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
}
