//! A tool's output: appended to `<logs>/<id>.log`, rotated to `<id>.log.1` at the next start
//! once it passes [`ROTATE_AT`], with the last lines kept in memory for error messages.

use crate::LockExt;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};

const ROTATE_AT: u64 = 5 * 1024 * 1024;
const TAIL_LINES: usize = 40;

pub struct Log {
    path: PathBuf,
    tail: Mutex<VecDeque<String>>,
}

impl Log {
    /// Opens the log of tool `id` under `dir`, moving a big one aside first. A folder that can't be
    /// prepared is logged, not fatal: the tool still runs and its output stays in memory.
    pub fn open(dir: &Path, id: &str) -> Self {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!(error = format!("{e:#}"), "creating the tools log folder");
        }
        let path = dir.join(format!("{id}.log"));
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_AT)
            && let Err(e) = std::fs::rename(&path, dir.join(format!("{id}.log.1")))
        {
            tracing::warn!(error = format!("{e:#}"), "rotating the tool log");
        }
        Self { path, tail: Mutex::default() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The last lines the tool printed, oldest first.
    pub fn tail(&self) -> String {
        self.tail.locked().iter().map(String::as_str).collect::<Vec<_>>().join("\n")
    }

    /// Appends a line of the host's own to the log (a restart, a port change).
    pub async fn note(&self, line: &str) {
        self.append(line).await;
    }

    /// Copies a child's pipe into the log until it closes.
    pub fn pump(self: &std::sync::Arc<Self>, reader: Option<impl AsyncRead + Unpin + Send + 'static>) {
        let Some(reader) = reader else { return };
        let log = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log.append(&line).await;
            }
        });
    }

    async fn append(&self, line: &str) {
        {
            let mut tail = self.tail.locked();
            if tail.len() == TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line.to_string());
        }
        let file = tokio::fs::OpenOptions::new().create(true).append(true).open(&self.path).await;
        match file {
            Ok(mut file) => {
                if let Err(e) = file.write_all(format!("{line}\n").as_bytes()).await {
                    tracing::warn!(error = format!("{e:#}"), path = %self.path.display(), "writing the tool log");
                }
            }
            Err(e) => tracing::warn!(error = format!("{e:#}"), path = %self.path.display(), "opening the tool log"),
        }
    }
}
