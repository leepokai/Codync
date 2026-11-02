//! Setup terminals: installing an agent's CLI or signing in to it runs in a
//! real PTY on this computer, streamed to the client that started it. The
//! host picks the command (from `backends::HARNESSES`); clients only type.

use crate::LockExt;
use crate::agent::backends;
use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Output kept for a client that (re)attaches mid-run.
const SCROLLBACK: usize = 256 * 1024;
/// A finished terminal stays readable this long.
const LINGER: Duration = Duration::from_mins(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    Install,
    /// Codync's Node.js, for agents that run on it (one terminal for every agent).
    Node,
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
    pub fn busy(&self) -> bool {
        self.0.locked().values().any(|term| term.output.locked().exit.is_none())
    }

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
            Step::Node => format!("{} node", crate::shell::quote(&crate::service::current_exe()?.to_string_lossy())),
            Step::Install => {
                let h = h.ok_or_else(|| anyhow!("Codync downloads {name} by itself"))?;
                h.install.and_then(backends::Install::command).ok_or_else(|| anyhow!("{}", h.setup))?
            }
            // Some CLIs (codex) delete the current credentials the moment a new sign-in starts.
            Step::Login if backends::signed_in(backend) == Some(true) => bail!("{name} is already signed in"),
            Step::Login => match method {
                Some(m) => crate::agent::auth::terminal_command(backend, m)
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
        let PtyPair { master, slave } = native_pty_system().openpty(size(cols, rows))?;
        let shell = crate::shell::line(command);
        let mut builder = CommandBuilder::new(shell.program);
        builder.args(&shell.args);
        for (k, v) in &shell.env {
            builder.env(k, v);
        }
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.cwd(dirs::home_dir().unwrap_or_else(|| "/".into()));
        let child = slave.spawn_command(builder)?;
        // Only the child may hold the terminal side, or the output never ends.
        drop(slave);
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
        tokio::spawn(run(self.clone(), id.clone(), term, master, child, rx));
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

/// Windows' pseudo console asks where the cursor is (`ESC[6n`) before the program starts,
/// and waits for the answer. The host answers it (the line under the `$ command` header) and
/// keeps the question from clients, which would answer it a second time into the program.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[2;1R";

/// `chunk` without its cursor query, if it has one.
fn without_cursor_query(chunk: &[u8]) -> Option<Vec<u8>> {
    let at = chunk.windows(CURSOR_QUERY.len()).position(|w| w == CURSOR_QUERY)?;
    Some([&chunk[..at], &chunk[at + CURSOR_QUERY.len()..]].concat())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(4), cols: cols.max(20), pixel_width: 0, pixel_height: 0 }
}

async fn run(
    terms: Arc<Terms>,
    id: String,
    term: Arc<Term>,
    master: Box<dyn MasterPty + Send>,
    mut child: Box<dyn Child + Send + Sync>,
    mut rx: mpsc::UnboundedReceiver<Input>,
) {
    let mut killer = child.clone_killer();
    let streams = master.try_clone_reader().and_then(|reader| Ok((reader, master.take_writer()?)));
    let (mut reader, mut writer) = match streams {
        Ok(streams) => streams,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "setup terminal has no streams");
            let _ = killer.kill();
            term.finish(-1);
            return;
        }
    };
    let mut pump = {
        let term = term.clone();
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; 16 * 1024];
            let mut answered = !cfg!(windows);
            // EOF or an error once the child and its children are gone.
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                match without_cursor_query(&buf[..n]).filter(|_| !answered) {
                    Some(rest) => {
                        answered = true;
                        let _ = term.input.send(Input::Data(CURSOR_REPLY.to_vec()));
                        term.push(&rest);
                    }
                    None => term.push(&buf[..n]),
                }
            }
        })
    };
    let mut wait = tokio::task::spawn_blocking(move || child.wait());
    let status = loop {
        tokio::select! {
            status = &mut wait => break status,
            input = rx.recv() => match input {
                // Keystrokes are small; a PTY takes them without blocking.
                Some(Input::Data(bytes)) => {
                    if let Err(error) = writer.write_all(&bytes) {
                        tracing::info!(%error, "setup terminal write failed");
                    }
                }
                Some(Input::Resize(cols, rows)) => {
                    let _ = master.resize(size(cols, rows));
                }
                Some(Input::Kill) | None => {
                    let _ = killer.kill();
                }
            },
        }
    };
    // Let the last output drain; a background grandchild can hold the PTY open forever, and
    // Windows' pseudo console keeps it open until the master side closes.
    let _ = tokio::time::timeout(Duration::from_secs(1), &mut pump).await;
    drop((writer, master));
    let code = match status {
        Ok(Ok(status)) => i32::try_from(status.exit_code()).unwrap_or(-1),
        _ => -1,
    };
    tracing::info!(term = %id, code, "setup terminal finished");
    // A finished sign-in is believed again, over the prompt its agent refused before.
    if term.key.1 == Step::Login && code == 0 {
        backends::clear_prompt_refused(&term.key.0);
    }
    term.finish(code);
    tokio::time::sleep(LINGER).await;
    terms.0.locked().remove(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_out_the_cursor_query() {
        assert_eq!(without_cursor_query(b"a\x1b[6nb").as_deref(), Some(&b"ab"[..]));
        assert_eq!(without_cursor_query(b"plain"), None);
    }

    #[tokio::test]
    async fn runs_in_a_pty_and_takes_input() {
        let terms = Arc::new(Terms::default());
        // `call` expands `%x%` again once `set /p` has read it.
        let (command, enter) = if cfg!(windows) {
            ("echo tty& set /p x=& call echo got-%x%", "\r")
        } else {
            ("[ -t 0 ] && echo tty; read x; echo got-$x", "\n")
        };
        let id = terms.spawn(("test".into(), Step::Login), command, 80, 24).unwrap();
        let term = terms.get(&id).unwrap();
        let (_, mut rx) = term.attach();
        terms.write(&id, &B64.encode(format!("hi{enter}"))).unwrap();
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
