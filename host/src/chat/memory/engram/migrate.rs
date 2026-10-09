//! One-way, retry-safe import of Codync's Markdown memory through Engram's native importer.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _, path::Path, time::Duration};

const MARKER: &str = ".codync-markdown-imported";
const BACKUP: &str = "markdown-backup.json";

pub(super) async fn run(binary: &Path, dir: &Path, project: &str) -> Result<()> {
    if tokio::fs::try_exists(dir.join(MARKER)).await? {
        return Ok(());
    }
    tokio::fs::create_dir_all(dir).await?;
    let backup = dir.join(BACKUP);
    // The immutable backup is also the import journal. A retry always reads the same input,
    // even when the old Markdown was edited after an interrupted migration.
    if !tokio::fs::try_exists(&backup).await? {
        let legacy = dir.parent().context("memory directory has no parent")?.join("memory");
        let files = tokio::task::spawn_blocking(move || read_legacy(&legacy)).await??;
        let staging = backup.with_extension("tmp");
        tokio::fs::write(&staging, serde_json::to_vec_pretty(&files)?).await?;
        tokio::fs::rename(&staging, &backup).await?;
    }
    let files: BTreeMap<String, String> = serde_json::from_slice(&tokio::fs::read(&backup).await?)?;
    let payload = bundle(project, &files)?;
    let count = payload["observations"].as_array().map_or(0, Vec::len);
    if count > 0 {
        let input = dir.join("markdown-import.json");
        tokio::fs::write(&input, serde_json::to_vec(&payload)?).await?;
        let output = tokio::time::timeout(
            Duration::from_secs(180),
            tokio::process::Command::new(binary)
                .arg("import")
                .arg(&input)
                .env("ENGRAM_DATA_DIR", dir)
                .env("ENGRAM_CLOUD_AUTOSYNC", "0")
                .current_dir(dir)
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        ensure!(output.status.success(), "Engram memory import failed: {}", String::from_utf8_lossy(&output.stderr));
        let database = dir.join("engram.db");
        let expected: Vec<String> = payload["observations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["sync_id"].as_str().map(str::to_owned))
            .collect();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let db = rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            for id in expected {
                let found: bool =
                    db.query_row("SELECT EXISTS(SELECT 1 FROM observations WHERE sync_id = ?)", [&id], |r| r.get(0))?;
                ensure!(found, "Engram import omitted a memory: {id}");
            }
            Ok(())
        })
        .await??;
        tokio::fs::remove_file(input).await?;
    }
    // This marker stays even after Forget everything: old files must never resurrect facts.
    tokio::fs::write(dir.join(MARKER), format!("Engram {}: {count} memories\n", super::VERSION)).await?;
    Ok(())
}

fn read_legacy(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    match std::fs::read_to_string(dir.join("profile.md")) {
        Ok(text) => {
            files.insert("profile.md".into(), text);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("reading profile memory"),
    }
    match std::fs::read_dir(dir.join("log")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path.extension().is_some_and(|x| x == "md") {
                    let name = path
                        .file_name()
                        .context("memory log has no filename")?
                        .to_str()
                        .context("memory log filename is not UTF-8")?;
                    files.insert(format!("log/{name}"), std::fs::read_to_string(&path)?);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("reading memory logs"),
    }
    Ok(files)
}

fn bundle(project: &str, files: &BTreeMap<String, String>) -> Result<Value> {
    let session = format!("{project}:markdown-import");
    let mut observations = Vec::new();
    for (path, text) in files {
        for (line, raw) in text.lines().enumerate() {
            let Some(rest) = raw.trim().strip_prefix("- (") else {
                continue;
            };
            let (date, content) = rest.split_once(") ").context("malformed dated memory; fix it before importing")?;
            let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").context("invalid date in memory")?;
            ensure!(!content.trim().is_empty(), "empty memory in {path}:{}", line + 1);
            let timestamp = format!("{date} 00:00:00");
            let profile = path == "profile.md";
            let source = format!("{project}\n{path}\n{}\n{raw}", line + 1);
            let mut id = String::from("obs-codync-");
            for b in Sha256::digest(source.as_bytes()) {
                let _ = write!(id, "{b:02x}");
            }
            observations.push(json!({
                "sync_id": id, "session_id": session, "project": project,
                "type": if profile { "learning" } else { "discovery" },
                "title": content.chars().take(100).collect::<String>(), "content": content,
                "scope": if profile { "personal" } else { "project" },
                "topic_key": format!("codync/import/{}/{}", path, line + 1),
                "tool_name": format!("codync-import:{path}:{}", line + 1),
                "created_at": timestamp, "updated_at": timestamp, "revision_count": 1,
            }));
        }
    }
    let start = observations.first().and_then(|o| o["created_at"].as_str()).unwrap_or("1970-01-01 00:00:00");
    Ok(json!({
        "version": "0.2.0", "exported_at": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "sessions": [{"id": session, "project": project, "directory": "", "started_at": start,
            "ended_at": start, "summary": "Imported Codync Markdown memory; original files preserved in markdown-backup.json"}],
        "observations": observations, "prompts": [],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_dates_sources_and_repeatable_import_identity() {
        let files = BTreeMap::from([
            ("profile.md".into(), "# Profile\n- (2026-01-01) 喜歡繁體中文\n".into()),
            ("log/2026-02.md".into(), "- (2026-02-02) 發布 Codync\n".into()),
        ]);
        let a = bundle("bot-a", &files).unwrap();
        let b = bundle("bot-a", &files).unwrap();
        assert_eq!(a["observations"], b["observations"]);
        let profile = &a["observations"][1];
        assert_eq!(profile["created_at"], "2026-01-01 00:00:00");
        assert_eq!(profile["content"], "喜歡繁體中文");
        assert_eq!(profile["tool_name"], "codync-import:profile.md:2");
        assert_ne!(profile["sync_id"], bundle("bot-b", &files).unwrap()["observations"][1]["sync_id"]);
    }

    #[test]
    fn malformed_dated_fact_stops_import_instead_of_disappearing() {
        let files = BTreeMap::from([("profile.md".into(), "- (2026-02-31) fact".into())]);
        assert!(bundle("bot", &files).is_err());
    }
}
