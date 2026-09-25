//! Setup terminals: installing an agent's CLI or signing in to it runs in a
//! real PTY on this computer, streamed to the client that started it. The
//! host picks the command (from `backends::HARNESSES`); clients only type.

use crate::LockExt;
use crate::backends;
use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};

/// Output kept for a client that (re)attaches mid-run.
const SCROLLBACK: usize = 256 * 1024;
/// A finished terminal stays readable this long.
const LINGER: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    Install,
    Login,
}

enum Input {
    Data(Vec<u8>),
    Resize(u16, u16),
    Kill,
}

struct Output {
    scrollback: Vec<u8>,
    exit: Option<i32>,
}

pub struct Term {
    key: (String, Step),
    input: mpsc::UnboundedSender<Input>,
    output: Mutex<Output>,
    /// `{"type":"output","data":<base64>}` and `{"type":"exit","code":n}`.
    events: broadcast::Sender<Value>,
}

impl Term {
    /// The scrollback so far plus a live feed, taken atomically so nothing is lost or doubled.
    pub fn attach(&self) -> (Vec<Value>, broadcast::Receiver<Value>) {
        let out = self.output.locked();
        let rx = self.events.subscribe();
        let mut head = vec![];
        if !out.scrollback.is_empty() {
            head.push(json!({"type": "output", "data": B64.encode(&out.scrollback)}));
        }
        if let Some(code) = out.exit {
            head.push(json!({"type": "exit", "code": code}));
        }
        (head, rx)
    }

    fn push(&self, bytes: &[u8]) {
        let mut out = self.output.locked();
        out.scrollback.extend_from_slice(bytes);
        if out.scrollback.len() > SCROLLBACK {
            let cut = out.scrollback.len() - SCROLLBACK;
            out.scrollback.drain(..cut);
        }
        let _ = self.events.send(json!({"type": "output", "data": B64.encode(bytes)}));
    }

    fn finish(&self, code: i32) {
        self.output.locked().exit = Some(code);
        let _ = self.events.send(json!({"type": "exit", "code": code}));
    }
}

#[derive(Default)]
pub struct Terms(Mutex<HashMap<String, Arc<Term>>>);

impl Terms {
    pub fn get(&self, id: &str) -> Option<Arc<Term>> {
        self.0.locked().get(id).cloned()
    }

    /// Starts `step` for `backend` (a sign-in `method` the agent advertised, or
    /// Codync's own command for it), or returns the one already running.
    pub async fn start(
        self: &Arc<Self>,
        backend: &str,
        step: Step,
        method: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<String> {
        let h = backends::harness(backend);
        let name = h.map_or(backend, |h| h.name);
        let command = match step {
            Step::Install => {
                let h = h.ok_or_else(|| anyhow!("Codync downloads {name} by itself"))?;
                h.install.ok_or_else(|| anyhow!("{}", h.setup))?.command()
            }
            // Some CLIs (codex) delete the current credentials the moment a new sign-in starts.
            Step::Login if backends::signed_in(backend) == Some(true) => bail!("{name} is already signed in"),
            Step::Login => match method {
                Some(m) => crate::auth::terminal_command(backend, m)
                    .ok_or_else(|| anyhow!("That sign-in option is gone; check {name} again"))?,
                None => backends::login_command(backend).await?,
            },
        };
        self.spawn((backend.to_owned(), step), &command, cols, rows)
    }

    fn spawn(self: &Arc<Self>, key: (String, Step), command: &str, cols: u16, rows: u16) -> Result<String> {
        let mut map = self.0.locked();
        if let Some((id, _)) = map.iter().find(|(_, t)| t.key == key && t.output.locked().exit.is_none()) {
            return Ok(id.clone());
        }
        let (pty, pts) = pty_process::open()?;
        pty.resize(pty_process::Size::new(rows.max(4), cols.max(20)))?;
        let child = pty_process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .current_dir(dirs::home_dir().unwrap_or_else(|| "/".into()))
            .kill_on_drop(true)
            .spawn(pts)?;
        let (tx, rx) = mpsc::unbounded_channel();
        let term = Arc::new(Term {
            key,
            input: tx,
            output: Mutex::new(Output { scrollback: vec![], exit: None }),
            events: broadcast::channel(256).0,
        });
        term.push(format!("\x1b[2m$ {command}\x1b[0m\r\n").as_bytes());
        let id = uuid::Uuid::new_v4().to_string();
        map.insert(id.clone(), term.clone());
        tracing::info!(backend = term.key.0, step = ?term.key.1, term = %id, "setup terminal started");
        tokio::spawn(run(self.clone(), id.clone(), term, pty, child, rx));
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let bytes = B64.decode(data)?;
        self.send(id, Input::Data(bytes))
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        self.send(id, Input::Resize(cols, rows))
    }

    pub fn close(&self, id: &str) {
        if let Some(t) = self.0.locked().remove(id) {
            let _ = t.input.send(Input::Kill);
        }
    }

    fn send(&self, id: &str, input: Input) -> Result<()> {
        let t = self.get(id).ok_or_else(|| anyhow!("that terminal is gone"))?;
        // Input after exit has nowhere to go; not an error for the typist.
        let _ = t.input.send(input);
        Ok(())
    }
}

async fn run(
    terms: Arc<Terms>,
    id: String,
    term: Arc<Term>,
    pty: pty_process::Pty,
    mut child: tokio::process::Child,
    mut rx: mpsc::UnboundedReceiver<Input>,
) {
    let (mut reader, mut writer) = pty.into_split();
    let pump = {
        let term = term.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            // EOF or EIO once the child and its children are gone.
            while let Ok(n @ 1..) = reader.read(&mut buf).await {
                term.push(&buf[..n]);
            }
        })
    };
    let status = loop {
        tokio::select! {
            status = child.wait() => break status,
            input = rx.recv() => match input {
                Some(Input::Data(bytes)) => {
                    if let Err(error) = writer.write_all(&bytes).await {
                        tracing::info!(%error, "setup terminal write failed");
                    }
                }
                Some(Input::Resize(cols, rows)) => {
                    let _ = writer.resize(pty_process::Size::new(rows.max(4), cols.max(20)));
                }
                Some(Input::Kill) | None => {
                    let _ = child.start_kill();
                }
            },
        }
    };
    // Let the last output drain; a background grandchild can hold the PTY open forever.
    let _ = tokio::time::timeout(Duration::from_secs(1), pump).await;
    let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
    tracing::info!(term = %id, code, "setup terminal finished");
    term.finish(code);
    tokio::time::sleep(LINGER).await;
    terms.0.locked().remove(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_in_a_pty_and_takes_input() {
        let terms = Arc::new(Terms::default());
        let id =
            terms.spawn(("test".into(), Step::Login), "[ -t 0 ] && echo tty; read x; echo got-$x", 80, 24).unwrap();
        let term = terms.get(&id).unwrap();
        let (_, mut rx) = term.attach();
        terms.write(&id, &B64.encode("hi\n")).unwrap();
        let code = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let v = rx.recv().await.unwrap();
                if v["type"] == "exit" {
                    return v["code"].as_i64().unwrap();
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(code, 0);
        let (head, _) = term.attach();
        let text = String::from_utf8(B64.decode(head[0]["data"].as_str().unwrap()).unwrap()).unwrap();
        assert!(text.contains("tty") && text.contains("got-hi"), "{text}");
    }
}
