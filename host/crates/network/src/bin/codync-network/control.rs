//! The running node's local door: `codync-network <command>` from any terminal or agent on this
//! computer sends its words over a socket only this user can open, and prints the answer.

#[cfg(not(unix))]
pub use other::{ask, serve};
#[cfg(unix)]
pub use unix::{ask, serve};

#[cfg(unix)]
mod unix {
    use super::super::Daemon;
    use anyhow::{Context, Result, bail};
    use serde_json::{Value, json};
    use std::path::Path;
    use std::sync::Arc;

    const SOCKET: &str = "control.sock";
    /// What a socket path may hold, with room to spare (`sun_path` is 104 bytes on macOS).
    const MAX_SOCKET_PATH: usize = 100;

    /// In the state folder, or, when that path is too long for a socket, a name for it in this
    /// user's temporary folder.
    fn socket_path(dir: &Path) -> std::path::PathBuf {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let path = dir.join(SOCKET);
        if path.as_os_str().len() <= MAX_SOCKET_PATH {
            return path;
        }
        let mut hasher = DefaultHasher::new();
        dir.hash(&mut hasher);
        std::env::temp_dir().join(format!("codync-network-{:016x}.sock", hasher.finish()))
    }

    pub fn serve(dir: &Path, daemon: Arc<Daemon>) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let path = socket_path(dir);
        // A socket left by a node that stopped.
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).context("couldn't open the control socket")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        tokio::spawn(async move {
            while let Ok((conn, _)) = listener.accept().await {
                let daemon = daemon.clone();
                tokio::spawn(async move {
                    let (read, mut write) = conn.into_split();
                    let Ok(Some(line)) = BufReader::new(read).lines().next_line().await else { return };
                    let answer = match serde_json::from_str::<Vec<String>>(&line) {
                        Ok(words) => match daemon.command(&words).await {
                            Ok(out) => json!({"ok": out}),
                            Err(e) => json!({"error": format!("{e:#}")}),
                        },
                        Err(_) => json!({"error": "unreadable request"}),
                    };
                    let _ = write.write_all(format!("{answer}\n").as_bytes()).await;
                });
            }
        });
        Ok(())
    }

    pub async fn ask(dir: &Path, words: &[String]) -> Result<String> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let conn = tokio::net::UnixStream::connect(socket_path(dir))
            .await
            .context("codync-network isn't running on this computer; start it with `codync-network up`")?;
        let (read, mut write) = conn.into_split();
        write.write_all(format!("{}\n", serde_json::to_string(words)?).as_bytes()).await?;
        let line = BufReader::new(read).lines().next_line().await?.unwrap_or_default();
        let answer: Value = serde_json::from_str(&line).context("the node gave an unreadable answer")?;
        match (answer["ok"].as_str(), answer["error"].as_str()) {
            (Some(out), _) => Ok(out.into()),
            (_, Some(error)) => bail!("{error}"),
            _ => bail!("the node gave an unreadable answer"),
        }
    }
}

// ponytail: Windows gets the control door (a named pipe) with Windows support; until then its node
// takes commands typed where it runs.
#[cfg(not(unix))]
mod other {
    use super::super::Daemon;
    use anyhow::{Result, bail};
    use std::path::Path;
    use std::sync::Arc;

    #[allow(clippy::unnecessary_wraps, reason = "the same signature as the Unix version")]
    pub fn serve(_dir: &Path, _daemon: Arc<Daemon>) -> Result<()> {
        Ok(())
    }

    #[allow(clippy::unused_async, reason = "the same signature as the Unix version")]
    pub async fn ask(_dir: &Path, _words: &[String]) -> Result<String> {
        bail!("on Windows, type commands where codync-network runs")
    }
}
