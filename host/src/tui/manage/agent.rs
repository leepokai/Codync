//! Agent sign-in and its setup terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::app::{After, App, Editor, Overlay};
use super::{AgentSetup, Fields, Input, Reply, SetupRow, Submit, ctrl, s};

impl App {
    pub fn open_agent(&mut self, backend: &str) {
        let b = self.backends.iter().find(|b| b["id"] == backend);
        // An agent without its Node.js can't start, so there's nothing to ask it yet.
        let known = b.is_some_and(|b| b["signedIn"] == true || b["node"] == "missing");
        let mut a = AgentSetup { backend: backend.to_owned(), auth: None, busy: false, cursor: 0, error: None };
        if !known {
            a.busy = true;
            self.sheet("agentAuth", json!({"backend": backend}), Reply::Auth(backend.to_owned()));
        }
        self.overlays.push(Overlay::Agent(a));
    }

    pub fn agent_rows(&self, a: &AgentSetup) -> Vec<SetupRow> {
        let b = self.backends.iter().find(|b| b["id"] == a.backend.as_str());
        let mut rows = vec![];
        if b.is_some_and(|b| b["curated"] == true && b["installed"] != true && b["canInstall"] == true) {
            rows.push(SetupRow::Install);
        }
        if b.is_some_and(|b| b["node"] == "missing") {
            rows.push(SetupRow::Node);
        }
        let signed_in =
            a.auth.as_ref().map_or_else(|| b.map(|b| &b["signedIn"]) == Some(&json!(true)), |v| v["signedIn"] == true);
        if !signed_in && !a.busy {
            if a.auth.as_ref().is_some_and(|v| v["login"] == true) {
                rows.push(SetupRow::Login);
            }
            for m in a.auth.as_ref().and_then(|v| v["methods"].as_array()).into_iter().flatten() {
                rows.push(SetupRow::Method(m.clone()));
            }
        }
        rows.push(SetupRow::Check);
        rows
    }

    pub(in crate::tui) fn agent_key(&mut self, mut a: AgentSetup, k: KeyEvent) -> Option<Overlay> {
        let rows = self.agent_rows(&a);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => a.cursor = (a.cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => a.cursor = a.cursor.saturating_sub(1),
            KeyCode::Enter if !a.busy => {
                let backend = a.backend.clone();
                let (cols, rows_n) = crossterm::terminal::size().unwrap_or((80, 24));
                let size = json!({"cols": cols, "rows": rows_n});
                match rows.get(a.cursor) {
                    Some(SetupRow::Install) => self.start_setup(&backend, "install", None, &size),
                    Some(SetupRow::Node) => self.start_setup(&backend, "node", None, &size),
                    Some(SetupRow::Login) => self.start_setup(&backend, "login", None, &size),
                    Some(SetupRow::Method(m)) => match m["kind"].as_str() {
                        Some("terminal") => self.start_setup(&backend, "login", m["id"].as_str(), &size),
                        Some("envVar") => {
                            let saved: Vec<String> = a
                                .auth
                                .as_ref()
                                .and_then(|v| v["savedEnv"].as_array())
                                .into_iter()
                                .flatten()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect();
                            let f = env_fields(&backend, m, &saved, &self.host);
                            self.overlays.push(Overlay::Agent(a));
                            self.overlays.push(Overlay::Fields(Box::new(f)));
                            return None;
                        }
                        _ => {
                            a.busy = true;
                            a.error = None;
                            self.flash("Finish signing in in the browser on this computer");
                            self.sheet(
                                "agentAuthenticate",
                                json!({"backend": backend, "method": m["id"]}),
                                Reply::Auth(backend.clone()),
                            );
                        }
                    },
                    Some(SetupRow::Check) | None => {
                        a.busy = true;
                        a.error = None;
                        self.sheet("agentAuth", json!({"backend": backend}), Reply::Auth(backend.clone()));
                    }
                }
            }
            _ => {}
        }
        Some(Overlay::Agent(a))
    }

    pub(super) fn start_setup(&mut self, backend: &str, step: &str, method: Option<&str>, size: &Value) {
        let mut body = json!({"backend": backend, "step": step, "cols": size["cols"], "rows": size["rows"]});
        if let Some(m) = method {
            body["method"] = m.into();
        }
        self.sheet("agentSetup", body, Reply::Setup);
        self.flash("Opening the terminal…");
    }

    /// Keys while the setup terminal has the screen: everything goes to it; ^] leaves.
    pub(in crate::tui) fn shell_key(&mut self, k: KeyEvent) {
        let Some(sh) = &self.shell else { return };
        let id = sh.id.clone();
        if sh.exited.is_some() {
            self.close_shell();
            return;
        }
        if ctrl(k) && matches!(k.code, KeyCode::Char(']' | '5')) {
            self.call("termClose", json!({"term": id}), After::Nothing);
            self.close_shell();
            return;
        }
        let bytes = key_bytes(k);
        if !bytes.is_empty() {
            self.term_input(&bytes);
        }
    }

    pub(in crate::tui) fn term_input(&self, bytes: &[u8]) {
        if let Some(sh) = &self.shell {
            let _ = sh.input.send(bytes.to_vec());
        }
    }

    pub fn on_resize(&self, cols: u16, rows: u16) {
        if let Some(sh) = self.shell.as_ref().filter(|s| s.exited.is_none()) {
            self.call("termResize", json!({"term": sh.id, "cols": cols, "rows": rows}), After::Nothing);
        }
    }

    pub(in crate::tui) fn on_term(&mut self, id: &str, v: &Value) {
        let Some(sh) = self.shell.as_mut().filter(|s| s.id == id) else { return };
        match v["type"].as_str() {
            Some("output") => {
                use base64::Engine as _;
                if let Ok(bytes) =
                    base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap_or_default())
                {
                    sh.out.extend_from_slice(&bytes);
                }
            }
            Some("exit") => {
                let code = v["code"].as_i64().unwrap_or(-1);
                sh.exited = Some(code);
                let how = if code == 0 { "Done" } else { "Stopped" };
                sh.out.extend_from_slice(
                    format!("\r\n\x1b[2m{how}. Press any key to go back to Codync.\x1b[0m\r\n").as_bytes(),
                );
            }
            _ => {}
        }
    }

    fn close_shell(&mut self) {
        let Some(sh) = self.shell.take() else { return };
        self.call("refreshBackends", json!({}), After::Backends);
        if let Some(Overlay::Agent(a)) = self.overlays.last_mut() {
            a.busy = true;
            self.sheet("agentAuth", json!({"backend": sh.backend}), Reply::Auth(sh.backend.clone()));
        }
    }
}

fn env_fields(backend: &str, method: &Value, saved: &[String], host: &str) -> Fields {
    let inputs = method["vars"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let name = s(v, "name");
            let has = saved.contains(&name);
            Input {
                label: s(v, "label"),
                secret: v["secret"] != false,
                required: v["optional"] != true && !has,
                multiline: false,
                placeholder: if has { "saved · type to replace".into() } else { name.clone() },
                name,
                ed: Editor::default(),
            }
        })
        .collect();
    let link = s(method, "link");
    Fields {
        title: s(method, "name"),
        note: if link.is_empty() {
            format!("Saved on {host} only.")
        } else {
            format!("Get one at {link} · saved on {host} only.")
        },
        choices: vec![],
        choice: 0,
        inputs,
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::AgentEnv(backend.to_owned()),
    }
}

/// Bytes a terminal sends for a key.
pub fn key_bytes(k: KeyEvent) -> Vec<u8> {
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let mut out: Vec<u8> = match k.code {
        KeyCode::Char(c) if ctrl(k) && c.is_ascii_alphabetic() => vec![(c.to_ascii_lowercase() as u8) & 0x1f],
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => vec![],
    };
    if alt && !out.is_empty() {
        out.insert(0, 0x1b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_become_terminal_bytes() {
        let k = |code, m| KeyEvent::new(code, m);
        assert_eq!(key_bytes(k(KeyCode::Char('c'), KeyModifiers::CONTROL)), [3]);
        assert_eq!(key_bytes(k(KeyCode::Char('é'), KeyModifiers::NONE)), "é".as_bytes());
        assert_eq!(key_bytes(k(KeyCode::Enter, KeyModifiers::NONE)), b"\r");
        assert_eq!(key_bytes(k(KeyCode::Char('b'), KeyModifiers::ALT)), b"\x1bb");
        assert_eq!(key_bytes(k(KeyCode::Up, KeyModifiers::NONE)), b"\x1b[A");
    }
}
