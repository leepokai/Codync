//! Clear observations, captured prompts and session summaries through the native CLI.

use anyhow::{Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::time::Duration;

pub(super) async fn run(bot: &str) -> Result<()> {
    let dir = super::directory(bot)?;
    let path = dir.join("engram.db");
    let projects = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut query = db.prepare("SELECT project FROM sessions UNION SELECT project FROM observations")?;
        Ok(query
            .query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect())
    })
    .await??;
    let binary = super::install::binary().await?;
    for project in projects {
        let result = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new(&binary)
                .args(["delete", "project", &project, "--hard"])
                .env("ENGRAM_DATA_DIR", &dir)
                .env("ENGRAM_CLOUD_AUTOSYNC", "0")
                .current_dir(&dir)
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        ensure!(result.status.success(), "clearing Engram memory failed: {}", String::from_utf8_lossy(&result.stderr));
    }
    // The migration journal and completion marker remain: old Markdown must not be imported again.
    Ok(())
}
