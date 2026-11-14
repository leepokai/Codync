//! Agent sign-in and its setup terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::app::{After, App, Editor, Overlay};
use super::{AgentSetup, Fields, Input, Reply, SetupRow, Submit, ctrl, s};

impl App {
    /// What the user calls agent `id` on this computer ("Claude Code").
    pub fn agent_name(&self, id: &str) -> String {
        self.backends.iter().find(|b| b["id"] == id).and_then(|b| b["name"].as_str()).unwrap_or(id).to_owned()
    }

    /// `recheck`: opened from a chat's sign-in request, whose agent refused a turn, so the agent
    /// is asked again however signed in it looked.
    pub fn open_agent(&mut self, backend: &str, recheck: bool) {
        let b = self.backends.iter().find(|b| b["id"] == backend);
        // An agent without its Node.js can't start, so there's nothing to ask it yet.
        let known = b.is_some_and(|b| (b["signedIn"] == true && !recheck) || b["node"] == "missing");
        let mut a = AgentSetup {
            backend: backend.to_owned(),
            auth: None,
            busy: false,
            signing_in: None,
            cursor: 0,
            error: None,
        };
        // Opening never downloads an agent: one not set up waits for Set up.
        if !known && (recheck || self.agent_set_up(&a)) {
            a.busy = true;
            self.sheet("agentAuth", json!({"backend": backend}), Reply::Auth(backend.to_owned()));
        }
        self.overlays.push(Overlay::Agent(a));
    }

    /// It has run on this computer: its CLI is there, or an earlier check started it. Until then, a
    /// check can download it, so only Set up starts one.
    pub fn agent_set_up(&self, a: &AgentSetup) -> bool {
        a.auth.is_some()
            || self
                .backends
                .iter()
                .find(|b| b["id"] == a.backend.as_str())
                .is_some_and(|b| b["installed"] == true || !b["signedIn"].is_null())
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
        rows.push(SetupRow::Brings);
        if let Some(login_profiles) = self.login_profiles(&a.backend) {
            rows.push(SetupRow::DefaultLoginProfile);
            rows.extend(login_profiles.iter().cloned().map(SetupRow::LoginProfile));
            // An account signs in with the agent's own CLI, so it needs that installed; an agent that keeps one
            // sign-in of its own takes only API key profiles, which any agent can have.
            if b.is_some_and(|b| b["installed"] == true && !b["loginProfileSignIn"].is_null()) {
                rows.push(SetupRow::AddAccount);
            }
            rows.push(SetupRow::AddApiKey);
        }
        rows
    }

    /// The login profile `r` renames on `row` (none: the default login profile), and the name the user gave it.
    fn rename_target<'a>(
        &'a self,
        backend: &str,
        row: Option<&'a SetupRow>,
    ) -> Option<(Option<String>, Option<&'a str>)> {
        match row? {
            SetupRow::DefaultLoginProfile => {
                let own = self.backends.iter().find(|b| b["id"] == backend).map(|b| &b["defaultLoginProfile"]);
                Some((None, own.and_then(|o| o["label"].as_str())))
            }
            SetupRow::LoginProfile(login_profile) => {
                Some((Some(login_profile["id"].as_str()?.to_owned()), login_profile["label"].as_str()))
            }
            _ => None,
        }
    }

    pub(in crate::tui) fn agent_key(&mut self, mut a: AgentSetup, k: KeyEvent) -> Option<Overlay> {
        let rows = self.agent_rows(&a);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => a.cursor = (a.cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => a.cursor = a.cursor.saturating_sub(1),
            KeyCode::Char('x') if !a.busy => {
                if let Some(SetupRow::LoginProfile(login_profile)) = rows.get(a.cursor) {
                    a.busy = true;
                    a.error = None;
                    let body = json!({"loginProfile": login_profile["id"]});
                    self.sheet("removeLoginProfile", body, Reply::LoginProfiles(a.backend.clone()));
                }
            }
            KeyCode::Char('r') if !a.busy => {
                if let Some((login_profile, label)) = self.rename_target(&a.backend, rows.get(a.cursor)) {
                    let f = rename_fields(&a.backend, login_profile, label);
                    self.overlays.push(Overlay::Agent(a));
                    self.overlays.push(Overlay::Fields(Box::new(f)));
                    return None;
                }
            }
            KeyCode::Enter if !a.busy => {
                let backend = a.backend.clone();
                let (cols, rows_n) = crossterm::terminal::size().unwrap_or((80, 24));
                let size = json!({"cols": cols, "rows": rows_n});
                match rows.get(a.cursor) {
                    Some(SetupRow::Install) => self.start_setup(&backend, "install", None, &size),
                    Some(SetupRow::Node) => self.start_setup(&backend, "node", None, &size),
                    Some(SetupRow::Login) => self.start_setup(&backend, "login", None, &size),
                    // An API key's keys are edited any time; an account has nothing to run once signed in.
                    Some(SetupRow::LoginProfile(p)) => {
                        if let Some(id) = p["id"].as_str().filter(|_| p["kind"] == "apiKey" || p["signedIn"] != true) {
                            self.overlays.push(Overlay::Agent(a));
                            self.sign_in_login_profile(&backend, id);
                            return None;
                        }
                    }
                    Some(row @ (SetupRow::AddAccount | SetupRow::AddApiKey)) => {
                        a.busy = true;
                        a.error = None;
                        let kind = if matches!(row, SetupRow::AddApiKey) { "apiKey" } else { "account" };
                        self.sheet(
                            "addLoginProfile",
                            json!({"backend": backend, "kind": kind}),
                            Reply::LoginProfileAdded(backend.clone()),
                        );
                    }
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
                    Some(SetupRow::Brings) => {
                        self.overlays.push(Overlay::Agent(a));
                        self.open_agent_gear(&backend);
                        return None;
                    }
                    Some(SetupRow::Check | SetupRow::DefaultLoginProfile) | None => {
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
        self.sheet("agentSetup", body, Reply::Setup(None));
        self.flash("Opening the terminal…");
    }

    /// Signs agent `backend`'s login profile `login_profile` in, over the agent's sheet: an API key one in its key
    /// form; an account by the agent itself in a browser on the computer (its sheet shows that until the host
    /// answers, once it's done), or in the setup terminal. One this app hasn't heard of yet (added from another
    /// app) opens nothing, since its kind isn't known: the agents are asked for again.
    pub(in crate::tui) fn sign_in_login_profile(&mut self, backend: &str, login_profile: &str) {
        let profiles = self.login_profiles(backend).unwrap_or_default();
        let Some(profile) = profiles.iter().find(|p| p["id"] == login_profile) else {
            self.call("refreshBackends", json!({}), After::Backends);
            return;
        };
        if profile["kind"] == "apiKey" {
            self.open_login_keys(backend, login_profile);
            return;
        }
        if self.backends.iter().any(|b| b["id"] == backend && b["loginProfileSignIn"] == "browser") {
            for o in &mut self.overlays {
                if let Overlay::Agent(a) = o
                    && a.backend == backend
                {
                    a.busy = true;
                    a.error = None;
                    a.signing_in = Some(login_profile.to_owned());
                }
            }
            let body = json!({"backend": backend, "loginProfile": login_profile});
            self.sheet("agentAuthenticate", body, Reply::LoginProfiles(backend.to_owned()));
            return;
        }
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let body =
            json!({"backend": backend, "step": "login", "loginProfile": login_profile, "cols": cols, "rows": rows});
        self.sheet("agentSetup", body, Reply::Setup(Some(login_profile.to_owned())));
        self.flash("Opening the terminal…");
    }

    /// An answer that carries agent `backend`'s login profiles (`backends`): its sheet shows them.
    pub(super) fn login_profiles_changed(&mut self, backend: &str, v: &Value) {
        if let Some(agents) = v["backends"].as_array() {
            self.backends.clone_from(agents);
        }
        for o in &mut self.overlays {
            if let Overlay::Agent(a) = o
                && a.backend == backend
            {
                a.busy = false;
                a.signing_in = None;
                a.error = None;
            }
        }
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
            match sh.login_profile {
                Some(login_profile) => {
                    let body = json!({"backend": sh.backend, "loginProfile": login_profile});
                    self.sheet("agentAuth", body, Reply::LoginProfiles(sh.backend));
                }
                None => self.sheet("agentAuth", json!({"backend": sh.backend}), Reply::Auth(sh.backend.clone())),
            }
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

/// Asks for a login profile's name (`login_profile`; none: the default one), from the one the user gave it.
fn rename_fields(backend: &str, login_profile: Option<String>, label: Option<&str>) -> Fields {
    Fields {
        title: "Rename login profile".into(),
        note: String::new(),
        choices: vec![],
        choice: 0,
        inputs: vec![Input {
            name: "name".into(),
            label: "Login profile name".into(),
            secret: false,
            required: false,
            multiline: false,
            placeholder: "Login profile name".into(),
            ed: Editor::with(label.unwrap_or_default()),
        }],
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::RenameLoginProfile { backend: backend.to_owned(), login_profile },
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

    #[tokio::test]
    async fn the_agent_sheet_adds_signs_in_and_removes_login_profiles() {
        let mut app = crate::tui::app::test_app();
        let login_profiles = json!([{"id": "a1", "name": "ada@example.com", "signedIn": true}, {"id": "a2", "name": null, "signedIn": false}]);
        app.backends = vec![
            json!({"id": "claude", "installed": true, "signedIn": true, "loginProfileSignIn": "terminal",
                "loginProfiles": login_profiles}),
            json!({"id": "gemini", "installed": true, "signedIn": true}),
            json!({"id": "codex", "installed": false, "loginProfiles": []}),
        ];
        let sheet = |backend: &str| AgentSetup {
            backend: backend.into(),
            auth: None,
            busy: false,
            signing_in: None,
            cursor: 0,
            error: None,
        };
        assert!(
            !app.agent_rows(&sheet("gemini"))
                .iter()
                .any(|r| matches!(r, SetupRow::LoginProfile(_) | SetupRow::AddAccount | SetupRow::AddApiKey)),
            "none from a host without login profiles"
        );
        let not_installed = app.agent_rows(&sheet("codex"));
        assert!(matches!(not_installed.last(), Some(SetupRow::AddApiKey)), "any agent takes an API key");
        assert!(!not_installed.iter().any(|r| matches!(r, SetupRow::AddAccount)), "an account needs the agent's CLI");
        let rows = app.agent_rows(&sheet("claude"));
        assert!(matches!(
            &rows[rows.len() - 5..],
            [
                SetupRow::DefaultLoginProfile,
                SetupRow::LoginProfile(_),
                SetupRow::LoginProfile(_),
                SetupRow::AddAccount,
                SetupRow::AddApiKey
            ]
        ));
        let mut press = |cursor: usize, code: KeyCode| {
            let a = AgentSetup { cursor, ..sheet("claude") };
            let Some(Overlay::Agent(a)) = app.agent_key(a, KeyEvent::from(code)) else { panic!("the sheet stays") };
            a.busy
        };
        assert!(!press(rows.len() - 4, KeyCode::Enter), "a signed-in account has nothing to run");
        assert!(press(rows.len() - 4, KeyCode::Char('x')), "x removes the login profile under the cursor");
        assert!(!press(0, KeyCode::Char('x')), "and nothing else");
        assert!(press(rows.len() - 2, KeyCode::Enter), "adding an account asks the host");
        assert!(press(rows.len() - 1, KeyCode::Enter), "and so does adding an API key");

        app.overlays.push(Overlay::Agent(AgentSetup { busy: true, ..sheet("claude") }));
        app.on_sheet_reply(
            Reply::LoginProfiles("claude".into()),
            Ok(json!({"backends": [{"id": "claude", "installed": true, "loginProfiles": []}]})),
        );
        let Some(Overlay::Agent(a)) = app.overlays.last() else { panic!("the sheet stays") };
        assert!(!a.busy);
        assert!(matches!(app.agent_rows(a).last(), Some(SetupRow::AddApiKey)));
        assert!(!app.agent_rows(a).iter().any(|r| matches!(r, SetupRow::AddAccount)), "no sign-in of its own to add");
        assert_eq!(app.agent_rows(a).iter().filter(|r| matches!(r, SetupRow::LoginProfile(_))).count(), 0);
    }

    #[tokio::test]
    async fn every_login_profile_is_renamed_and_a_browser_agent_signs_its_own_in_itself() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = crate::tui::app::test_app();
        let login_profiles =
            json!([{"id": "a1", "name": "Work", "label": "Work", "email": "ada@example.com", "signedIn": false}]);
        app.backends = vec![json!({"id": "agy", "name": "Antigravity", "installed": true, "signedIn": true,
            "loginProfileSignIn": "browser", "loginProfiles": login_profiles, "defaultLoginProfile": {"name": "me@example.com"}})];
        let sheet = |cursor| AgentSetup {
            backend: "agy".into(),
            auth: None,
            busy: false,
            signing_in: None,
            cursor,
            error: None,
        };
        let own =
            app.agent_rows(&sheet(0)).iter().position(|r| matches!(r, SetupRow::DefaultLoginProfile)).expect("its row");

        for (cursor, login_profile, label) in [(own, None, ""), (own + 1, Some("a1"), "Work")] {
            assert!(
                app.agent_key(sheet(cursor), KeyEvent::from(KeyCode::Char('r'))).is_none(),
                "the name is asked for"
            );
            let Some(Overlay::Fields(f)) = app.overlays.pop() else { panic!("the name field") };
            assert_eq!((f.title.as_str(), f.inputs[0].ed.text.as_str()), ("Rename login profile", label));
            assert!(
                matches!(&f.submit, Submit::RenameLoginProfile { backend, login_profile: a } if backend == "agy" && a.as_deref() == login_profile)
            );
            app.overlays.clear();
        }
        assert!(app.agent_key(sheet(0), KeyEvent::from(KeyCode::Char('r'))).is_some(), "only login profiles are named");

        // ↵ on the signed-out login profile: no terminal, the row waits for the browser.
        assert!(app.agent_key(sheet(own + 1), KeyEvent::from(KeyCode::Enter)).is_none(), "the sheet goes back itself");
        let Some(Overlay::Agent(a)) = app.overlays.last() else { panic!("the sheet stays") };
        assert_eq!((a.busy, a.signing_in.as_deref(), app.shell.is_none()), (true, Some("a1"), true));
        app.host = "Mac".into();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw the sheet");
        let text: String = terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for shown in [
            "Login profiles",
            "me@example.com",
            "Default · Signed in",
            "ada@example.com · Finish signing in in the browser on Mac…",
            "r Rename login profile",
        ] {
            assert!(text.contains(shown), "{shown}: {text}");
        }
        app.on_sheet_reply(Reply::LoginProfiles("agy".into()), Ok(json!({"signedIn": true, "backends": []})));
        assert!(matches!(app.overlays.last(), Some(Overlay::Agent(a)) if !a.busy && a.signing_in.is_none()));
    }

    #[tokio::test]
    async fn api_key_login_profiles_show_their_variables_and_open_their_key_form() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = crate::tui::app::test_app();
        app.host = "Mac".into();
        let agent = |profiles: Value| {
            json!({"id": "codex", "name": "Codex", "installed": true, "signedIn": true, "loginProfileSignIn": null,
                "loginKeys": ["OPENAI_API_KEY"], "loginProfiles": profiles})
        };
        let work = json!({"id": "k1", "kind": "apiKey", "name": "Work", "label": "Work", "signedIn": true,
            "vars": ["OPENAI_API_KEY", "OPENAI_BASE_URL"]});
        let new = |id: &str| json!({"id": id, "kind": "apiKey", "name": null, "signedIn": false, "vars": []});
        app.backends = vec![agent(json!([work, new("k2")]))];
        let sheet = |cursor| AgentSetup {
            backend: "codex".into(),
            auth: None,
            busy: false,
            signing_in: None,
            cursor,
            error: None,
        };
        let rows = app.agent_rows(&sheet(0));
        assert!(matches!(rows.last(), Some(SetupRow::AddApiKey)));
        assert!(!rows.iter().any(|r| matches!(r, SetupRow::AddAccount)), "Codex keeps one sign-in of its own");

        app.overlays.push(Overlay::Agent(sheet(rows.len() - 2)));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw the sheet");
        let text: String = terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for shown in [
            "API key · OPENAI_API_KEY, OPENAI_BASE_URL · ↵ Edit keys",
            "API key · No key yet",
            "Add API key",
            "Runs Codex on keys you type or import from your login shell.",
            "x Remove login profile",
        ] {
            assert!(text.contains(shown), "{shown}: {text}");
        }

        // ↵ on the one without keys: its form starts on the variable the agent's key usually goes in.
        app.overlays.clear();
        assert!(app.agent_key(sheet(rows.len() - 2), KeyEvent::from(KeyCode::Enter)).is_none());
        let Some(Overlay::Fields(f)) = app.overlays.pop() else { panic!("the key form") };
        let names: Vec<_> = f.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!((f.title.as_str(), names), ("API key", vec!["OPENAI_API_KEY", "+name", "+value"]));
        assert!(
            matches!(&f.submit, Submit::LoginProfileKeys { login_profile, saved, .. } if login_profile == "k2" && saved.is_empty())
        );

        // A new API key profile gets its key form next, not a sign-in.
        let added = json!({"loginProfile": new("k3"), "backends": [agent(json!([new("k3")]))]});
        app.on_sheet_reply(Reply::LoginProfileAdded("codex".into()), Ok(added));
        assert!(matches!(app.overlays.last(), Some(Overlay::Fields(f))
            if matches!(&f.submit, Submit::LoginProfileKeys { login_profile, .. } if login_profile == "k3")));
        assert!(app.shell.is_none());
    }
}
