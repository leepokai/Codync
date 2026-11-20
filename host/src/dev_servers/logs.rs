//! A dev server's output: `~/.codync/dev-servers/<id>.log`, the current run only, rotated once so it stays bounded.

use anyhow::{Context, Result};
use std::io::{ErrorKind, Read as _, Seek as _, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::Mutex;

/// A log moves aside at this size; the one before it is dropped.
const ROTATE_AT: u64 = 5 * 1024 * 1024;
/// How far back from the end `tail` reads.
const TAIL_BYTES: u64 = 256 * 1024;
/// Lines the apps show, and the most a bot can ask for.
pub const SHOWN_LINES: usize = 200;
pub const MOST_LINES: usize = 1000;

fn folder() -> PathBuf {
    crate::service::data_dir().join("dev-servers")
}

fn path(id: &str) -> PathBuf {
    folder().join(format!("{id}.log"))
}

fn rotated(id: &str) -> PathBuf {
    folder().join(format!("{id}.log.1"))
}

struct Writer {
    id: String,
    file: tokio::fs::File,
    size: u64,
}

impl Writer {
    async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if self.size + bytes.len() as u64 > ROTATE_AT {
            tokio::fs::rename(path(&self.id), rotated(&self.id)).await?;
            self.file = tokio::fs::File::create(path(&self.id)).await?;
            self.size = 0;
        }
        self.file.write_all(bytes).await?;
        self.size += bytes.len() as u64;
        Ok(())
    }
}

/// Starts a new log for this run and copies `stdout` and `stderr` into it until they close.
pub async fn capture(id: &str, stdout: ChildStdout, stderr: ChildStderr) -> Result<()> {
    tokio::fs::create_dir_all(folder()).await?;
    remove(&rotated(id));
    let file = tokio::fs::File::create(path(id)).await?;
    let writer = Arc::new(Mutex::new(Writer { id: id.to_owned(), file, size: 0 }));
    tokio::spawn(pump(stdout, writer.clone()));
    tokio::spawn(pump(stderr, writer));
    Ok(())
}

async fn pump(mut from: impl AsyncRead + Unpin, writer: Arc<Mutex<Writer>>) {
    let mut buffer = vec![0; 8192];
    loop {
        match from.read(&mut buffer).await {
            Ok(0) => break,
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "couldn't read a dev server's output");
                break;
            }
            Ok(read) => {
                if let Err(error) = writer.lock().await.write(&buffer[..read]).await {
                    // Keep reading: a full disk mustn't block the server on a full pipe.
                    tracing::debug!(error = format!("{error:#}"), "couldn't write a dev server's log");
                }
            }
        }
    }
}

/// Why it couldn't start, where its output would have been.
pub fn note(id: &str, text: &str) {
    let written = std::fs::create_dir_all(folder()).and_then(|()| std::fs::write(path(id), text));
    if let Err(error) = written {
        tracing::debug!(%error, "couldn't write a dev server's log");
    }
}

pub fn delete(id: &str) {
    remove(&path(id));
    remove(&rotated(id));
}

fn remove(file: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(file)
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!(%error, file = %file.display(), "couldn't delete a dev server's log");
    }
}

/// The log's last `lines` lines, as plain text.
pub async fn tail(id: &str, lines: usize) -> Result<String> {
    let (current, before) = (path(id), rotated(id));
    let text = tokio::task::spawn_blocking(move || {
        let mut text = read_end(&current)?;
        if text.lines().count() < lines {
            text = read_end(&before)? + &text;
        }
        anyhow::Ok(text)
    })
    .await??;
    Ok(last_lines(&crate::usage::strip_ansi(&text), lines))
}

/// The end of a log. A server that wrote nothing yet has no file, which isn't a failure; anything else is.
pub(super) fn read_end(path: &std::path::Path) -> Result<String> {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(TAIL_BYTES)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    match read() {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

pub(super) fn last_lines(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
