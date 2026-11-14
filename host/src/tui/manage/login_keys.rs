//! An API key login profile's keys: its key form (a `Fields` sheet: one secret value per variable, then a new
//! variable's name and value; ^x removes a saved variable, ^l imports) and the list of the login shell's variables
//! to import, ticked with space; importing saves what the form holds too. Values only go to the host: nothing shows
//! them again.

use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Editor, Overlay};
use super::super::l10n::tr;
use super::{Fields, Input, Reply, Submit};

/// The key form's inputs for a variable the user names: no variable is called that.
pub(super) const NEW_NAME: &str = "+name";
pub(super) const NEW_VALUE: &str = "+value";

/// The login shell's variables, to pick the ones login profile `login_profile` of agent `backend` copies.
pub struct KeyImport {
    pub backend: String,
    pub login_profile: String,
    /// What the key form holds, saved along with the import.
    pub vars: serde_json::Map<String, Value>,
    /// `None` while the host lists them.
    pub names: Option<Vec<String>>,
    pub picked: BTreeSet<String>,
    pub cursor: usize,
    pub importing: bool,
    pub error: Option<String>,
}

/// The key form's values as the host takes them: the new variable under the name typed for it. Also what a new
/// variable still lacks (its name or its value), which leaves it out.
fn key_vars(mut vars: serde_json::Map<String, Value>) -> (serde_json::Map<String, Value>, Option<String>) {
    let missing = match (vars.remove(NEW_NAME), vars.remove(NEW_VALUE)) {
        (Some(Value::String(name)), Some(value)) => {
            vars.insert(name, value);
            None
        }
        (None, None) => None,
        (name, _) => Some(format!("{} is required.", tr(if name.is_some() { "Value" } else { "Variable name" }))),
    };
    (vars, missing)
}

impl App {
    /// Opens the key form of agent `backend`'s API key login profile `id`.
    pub(in crate::tui) fn open_login_keys(&mut self, backend: &str, id: &str) {
        let agent = self.backends.iter().find(|b| b["id"] == backend);
        let Some(profile) = self.login_profiles(backend).unwrap_or_default().iter().find(|p| p["id"] == id) else {
            return;
        };
        let names = |v: &Value| -> Vec<String> {
            v.as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_owned)).collect()
        };
        let saved = names(&profile["vars"]);
        // A profile without keys starts on the variables its agent's key usually goes in.
        let usual = if saved.is_empty() { agent.map(|b| names(&b["loginKeys"])).unwrap_or_default() } else { vec![] };
        let input = |name: &str, label: &str, placeholder: &str, secret: bool| Input {
            name: name.to_owned(),
            label: label.to_owned(),
            secret,
            required: false,
            multiline: false,
            placeholder: tr(placeholder).into_owned(),
            ed: Editor::default(),
        };
        let mut inputs: Vec<Input> =
            saved.iter().map(|name| input(name, name, "Saved (type to replace)", true)).collect();
        inputs.extend(usual.iter().map(|name| input(name, name, "Value", true)));
        inputs.push(input(NEW_NAME, "Add variable", "Variable name", false));
        inputs.push(input(NEW_VALUE, "Value", "Value", true));
        let f = Fields {
            title: profile["name"].as_str().unwrap_or("API key").to_owned(),
            note: format!("Values are kept on {} only and never shown again.", self.host),
            choices: vec![],
            choice: 0,
            inputs,
            cursor: 0,
            error: None,
            saving: false,
            submit: Submit::LoginProfileKeys { backend: backend.to_owned(), login_profile: id.to_owned(), saved },
        };
        self.overlays.push(Overlay::Fields(Box::new(f)));
    }

    /// ↵ in the key form: what the user typed, the new variable under its name.
    pub(super) fn save_login_keys(&self, f: &mut Fields, vars: serde_json::Map<String, Value>) {
        let Submit::LoginProfileKeys { backend, login_profile, .. } = &f.submit else { return };
        let (vars, missing) = key_vars(vars);
        let missing = missing.or_else(|| vars.is_empty().then(|| "Type at least one key.".to_owned()));
        if missing.is_some() {
            f.saving = false;
            f.error = missing;
            return;
        }
        let body = json!({"loginProfile": login_profile, "vars": vars, "import": []});
        self.sheet("setLoginProfileKeys", body, Reply::LoginKeys(backend.clone()));
    }

    /// ^x in the key form: the saved variable under the cursor goes (the host takes an empty value as that).
    pub(super) fn remove_login_key(&self, f: &mut Fields) {
        let Some(name) = f.current().map(|i| i.name.clone()) else { return };
        let Submit::LoginProfileKeys { backend, login_profile, saved } = &f.submit else { return };
        if !saved.contains(&name) {
            return;
        }
        let body = json!({"loginProfile": login_profile, "vars": {name: ""}, "import": []});
        self.sheet("setLoginProfileKeys", body, Reply::LoginKeys(backend.clone()));
        f.saving = true;
        f.error = None;
    }

    /// ^l in the key form: the login shell's variables, over it.
    pub(super) fn import_login_keys(&mut self, f: &Fields) {
        let Submit::LoginProfileKeys { backend, login_profile, .. } = &f.submit else { return };
        self.sheet("loginShellVariables", json!({}), Reply::ShellVariables(login_profile.clone()));
        self.overlays.push(Overlay::KeyImport(KeyImport {
            backend: backend.clone(),
            login_profile: login_profile.clone(),
            vars: key_vars(f.typed()).0,
            names: None,
            picked: BTreeSet::new(),
            cursor: 0,
            importing: false,
            error: None,
        }));
    }

    pub(in crate::tui) fn key_import_key(&self, mut i: KeyImport, k: KeyEvent) -> Option<Overlay> {
        let names = i.names.as_deref().unwrap_or_default();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => i.cursor = (i.cursor + 1).min(names.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => i.cursor = i.cursor.saturating_sub(1),
            KeyCode::Char(' ') if !i.importing => {
                if let Some(name) = names.get(i.cursor)
                    && !i.picked.remove(name)
                {
                    i.picked.insert(name.clone());
                }
            }
            KeyCode::Enter if !i.importing && !i.picked.is_empty() => {
                let body = json!({"loginProfile": i.login_profile, "vars": i.vars, "import": i.picked});
                self.sheet("setLoginProfileKeys", body, Reply::LoginKeys(i.backend.clone()));
                i.importing = true;
                i.error = None;
            }
            _ => {}
        }
        Some(Overlay::KeyImport(i))
    }

    /// The host listed the login shell's variables (or couldn't), for the import list of `login_profile`.
    pub(super) fn shell_variables(&mut self, login_profile: &str, res: Result<&Value, String>) {
        let Some(Overlay::KeyImport(i)) =
            self.overlays.last_mut().filter(|o| matches!(o, Overlay::KeyImport(i) if i.login_profile == login_profile))
        else {
            return;
        };
        let names = res.as_ref().ok().and_then(|v| v["names"].as_array());
        i.names = Some(names.into_iter().flatten().filter_map(|n| n.as_str().map(str::to_owned)).collect());
        i.error = res.err();
    }

    /// `setLoginProfileKeys` answered: the key form and its import list close, the agent's sheet shows the profile.
    pub(super) fn login_keys_saved(&mut self, backend: &str, v: &Value) {
        let form = |o: &Overlay| matches!(o, Overlay::Fields(f) if matches!(f.submit, Submit::LoginProfileKeys { .. }));
        if let Some(at) = self.overlays.iter().rposition(form) {
            self.overlays.truncate(at);
        }
        self.login_profiles_changed(backend, v);
        self.flash("Saved");
    }

    /// `setLoginProfileKeys` was refused: the sheet that asked says why.
    pub(super) fn login_keys_failed(&mut self, e: String) {
        match self.overlays.last_mut() {
            Some(Overlay::Fields(f)) => {
                f.saving = false;
                f.error = Some(e);
            }
            Some(Overlay::KeyImport(i)) => {
                i.importing = false;
                i.error = Some(e);
            }
            _ => self.flash(&e),
        }
    }
}
