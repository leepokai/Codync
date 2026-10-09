//! Native JSON backup/restore; no application-written SQLite rows.

use super::{directory, install};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{path::Path, time::Duration};

pub(in crate::chat::memory) async fn export(bot: &str) -> Result<Value> {
    super::prepare(bot).await?;
    let dir = directory(bot)?;
    let file = dir.join(format!("export-{}.json", uuid::Uuid::new_v4()));
    let result = async {
        command(bot, "export", &file).await?;
        Ok(serde_json::from_slice(&tokio::fs::read(&file).await?)?)
    }
    .await;
    cleanup(&file).await;
    result
}

pub(in crate::chat::memory) async fn import(bot: &str, data: &Value) -> Result<Value> {
    ensure!(data["sessions"].is_array() && data["observations"].is_array(), "invalid Engram backup");
    let data = scoped_backup(bot, data)?;
    let bytes = serde_json::to_vec(&data)?;
    ensure!(bytes.len() <= 100 * 1024 * 1024, "memory backup exceeds 100 MB");
    let before = export(bot).await?;
    let dir = directory(bot)?;
    let backup = dir.join(format!("before-import-{}.json", uuid::Uuid::new_v4()));
    tokio::fs::write(&backup, serde_json::to_vec(&before)?).await?;
    let file = dir.join(format!("import-{}.json", uuid::Uuid::new_v4()));
    tokio::fs::write(&file, bytes).await?;
    let result = command(bot, "import", &file).await;
    cleanup(&file).await;
    result?;
    Ok(serde_json::json!({"backup":backup}))
}

fn scoped_backup(bot: &str, data: &Value) -> Result<Value> {
    let project = super::project(bot);
    let mut data = data.clone();
    let mut sessions = std::collections::HashMap::new();
    for session in data["sessions"].as_array_mut().context("backup has no sessions")? {
        let old = session["id"].as_str().context("backup session has no id")?.to_owned();
        let new = if session["project"] == project {
            old.clone()
        } else {
            format!("{project}:import:{}", crate::chat::memory::read::content_hash(&old))
        };
        if new != old {
            session["directory"] = "".into();
        }
        session["id"] = new.clone().into();
        session["project"] = project.clone().into();
        sessions.insert(old, new);
    }
    for collection in ["observations", "prompts", "prompt_tombstones"] {
        for record in data[collection].as_array_mut().into_iter().flatten() {
            if let Some(original) = record["project"].as_str().filter(|p| *p != project)
                && collection == "observations"
            {
                record["tool_name"] =
                    format!("Imported from {original}; {}", record["tool_name"].as_str().unwrap_or_default()).into();
            }
            record["project"] = project.clone().into();
            let source = record["session_id"].as_str().context("backup record has no session")?;
            record["session_id"] = sessions.get(source).context("backup references an unknown session")?.clone().into();
        }
    }
    for relation in data["relations"].as_array_mut().into_iter().flatten() {
        if let Some(source) = relation["session_id"].as_str() {
            relation["session_id"] =
                sessions.get(source).context("relation references an unknown session")?.clone().into();
        }
    }
    Ok(data)
}

async fn command(bot: &str, action: &str, file: &Path) -> Result<()> {
    let mut command = tokio::process::Command::new(install::binary().await?);
    command.arg(action).arg(file);
    if action == "export" {
        command.arg("--all");
    }
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        command
            .env("ENGRAM_DATA_DIR", directory(bot)?)
            .env("ENGRAM_CLOUD_AUTOSYNC", "0")
            .current_dir(directory(bot)?)
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    ensure!(output.status.success(), "Engram {action} failed: {}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

async fn cleanup(file: &Path) {
    if let Err(error) = tokio::fs::remove_file(file).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %file.display(), "removing temporary memory backup failed");
    }
}
