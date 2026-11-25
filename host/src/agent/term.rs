//! Terminals: installing an agent's CLI or signing in to it runs in a real PTY on
//! this computer, streamed to the client that started it; the host picks the
//! command (from `backends::HARNESSES`) and clients only type. A bot's own
//! terminal is the user's login shell in the bot's folder.

use crate::LockExt;
use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use codync_agent::backends;
use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
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
    /// A bot's own shell, in its folder (`shell`); never asked for by name.
    #[serde(skip)]
    Shell,
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

/// What a terminal is for: an agent (a bot, for its shell), the step, the account login profile a sign-in is for, and
/// the app's tab a shell is drawn in.
#[derive(PartialEq, Eq)]
struct Key {
    agent: String,
    step: Step,
    profile: Option<String>,
    tab: Option<String>,
}

pub struct Term {
    key: Key,
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
    /// A setup still running: a bot's shell stays open as long as its tab does, and holds nothing up.
    pub fn busy(&self) -> bool {
        self.0.locked().values().any(|term| term.key.step != Step::Shell && term.output.locked().exit.is_none())
    }

    pub fn get(&self, id: &str) -> Option<Arc<Term>> {
        self.0.locked().get(id).cloned()
    }

    /// Whether a sign-in to login profile `id` is open.
    pub fn signing_in(&self, id: &str) -> bool {
        let open = |term: &Arc<Term>| term.key.profile.as_deref() == Some(id) && term.output.locked().exit.is_none();
        self.0.locked().values().any(open)
    }

    /// Runs `command`, which signs `backend`'s account login profile `id` in, or returns the one already running.
    pub fn sign_in_account(
        self: &Arc<Self>,
        backend: &str,
        id: &str,
        command: &str,
        cols: u16,
        rows: u16,
    ) -> Result<String> {
        self.spawn(
            Key { agent: backend.to_owned(), step: Step::Login, profile: Some(id.to_owned()), tab: None },
            command_line(command),
            Some(command),
            home(),
            cols,
            rows,
        )
    }

    /// The user's login shell in bot `bot`'s folder `cwd` for the app's tab `tab` (a bot can have several), or the one
    /// already open for it.
    pub fn shell(self: &Arc<Self>, bot: &str, tab: Option<&str>, cwd: &Path, cols: u16, rows: u16) -> Result<String> {
        if !cwd.is_dir() {
            bail!("{} is not a folder on this computer", cwd.display());
        }
        self.spawn(
            Key { agent: bot.to_owned(), step: Step::Shell, profile: None, tab: tab.map(str::to_owned) },
            login_shell(),
            None,
            cwd.to_owned(),
            cols,
            rows,
        )
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
            Step::Node => {
                format!("{} node", codync_agent::shell::quote(&crate::service::current_exe()?.to_string_lossy()))
            }
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
            Step::Shell => bail!("a shell opens from its bot"),
        };
        let key = Key { agent: backend.to_owned(), step, profile: None, tab: None };
        self.spawn(key, command_line(&command), Some(&command), home(), cols, rows)
    }

    /// Runs `builder` in `cwd` on a new PTY, its `banner` (the command) shown first, or returns the open terminal for
    /// `key`.
    fn spawn(
        self: &Arc<Self>,
        key: Key,
        mut builder: CommandBuilder,
        banner: Option<&str>,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
    ) -> Result<String> {
        let mut map = self.0.locked();
        if let Some((id, _)) = map.iter().find(|(_, t)| t.key == key && t.output.locked().exit.is_none()) {
            return Ok(id.clone());
        }
        let PtyPair { master, slave } = native_pty_system().openpty(size(cols, rows))?;
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.cwd(cwd);
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
        if let Some(command) = banner {
            term.push(format!("\x1b[2m$ {command}\x1b[0m\r\n").as_bytes());
        }
        let id = uuid::Uuid::new_v4().to_string();
        map.insert(id.clone(), term.clone());
        tracing::info!(backend = term.key.agent, step = ?term.key.step, term = %id, "setup terminal started");
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

    /// Ends the shells drawn in the app's tab `tab`, one for each bot it was shown for.
    pub fn close_shells(&self, tab: &str) {
        self.0.locked().retain(|_, t| {
            let shell = t.key.step == Step::Shell && t.key.tab.as_deref() == Some(tab);
            if shell {
                let _ = t.input.send(Input::Kill);
            }
            !shell
        });
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

/// `command` as the platform's shell runs one line.
fn command_line(command: &str) -> CommandBuilder {
    let shell = codync_agent::shell::line(command);
    let mut builder = CommandBuilder::new(shell.program);
    builder.args(&shell.args);
    for (k, v) in &shell.env {
        if v.is_empty() { builder.env_remove(k) } else { builder.env(k, v) }
    }
    builder
}

/// The user's own interactive shell: `$SHELL` as a login shell on Unix, PowerShell on Windows.
fn login_shell() -> CommandBuilder {
    if cfg!(windows) {
        let mut builder = CommandBuilder::new("powershell.exe");
        builder.arg("-NoLogo");
        return builder;
    }
    let fallback = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    let mut builder =
        CommandBuilder::new(std::env::var("SHELL").ok().filter(|s| !s.is_empty()).as_deref().unwrap_or(fallback));
    builder.arg("-l");
    builder
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| "/".into())
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
    // A finished sign-in is believed again, over the prompt that login (the login profile's, or the agent's own)
    // refused before.
    if term.key.step == Step::Login && code == 0 {
        backends::clear_prompt_refused(term.key.profile.as_deref().unwrap_or(&term.key.agent));
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
        let id = terms
            .spawn(
                Key { agent: "test".into(), step: Step::Login, profile: None, tab: None },
                command_line(command),
                Some(command),
                home(),
                80,
                24,
            )
            .unwrap();
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

    /// A bot's shell opens in its folder, a second open for the same tab while it runs is the same terminal, another tab
    /// gets its own, and closing a tab ends its shell on every bot and no other.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_bot_shell_opens_in_its_folder() {
        let terms = Arc::new(Terms::default());
        let dir = std::env::temp_dir().join(format!("codync-term-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = terms.shell("bot", Some("t1"), &dir, 80, 24).unwrap();
        assert_eq!(terms.shell("bot", Some("t1"), &dir, 80, 24).unwrap(), id);
        let other = terms.shell("bot", Some("t2"), &dir, 80, 24).unwrap();
        assert_ne!(other, id, "another tab, another shell");
        let elsewhere = terms.shell("bot2", Some("t2"), &dir, 80, 24).unwrap();
        assert!(!terms.signing_in("t2"), "a tab is no login profile");
        terms.close_shells("t2");
        assert!(terms.get(&other).is_none() && terms.get(&elsewhere).is_none(), "the tab's shells end");
        assert!(!terms.busy(), "an open shell holds nothing up");
        let term = terms.get(&id).unwrap();
        let (_, mut rx) = term.attach();
        terms.write(&id, &B64.encode("pwd; exit\n")).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async { while rx.recv().await.unwrap()["type"] != "exit" {} })
            .await
            .unwrap();
        let (head, _) = term.attach();
        let text = String::from_utf8(B64.decode(head[0]["data"].as_str().unwrap()).unwrap()).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(text.contains(&name), "{text}");
    }
}
