//! The labeled text fields sheet: keys, connector setup, a pasted MCP config, a handoff.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Editor, Entry, Overlay};
use super::market::connector_inputs;
use super::{Fields, Input, Reply, Submit, ctrl, newline};

impl App {
    pub(in crate::tui) fn fields_key(&mut self, mut f: Box<Fields>, k: KeyEvent) -> Option<Overlay> {
        if f.saving {
            return if k.code == KeyCode::Esc { None } else { Some(Overlay::Fields(f)) };
        }
        let rows = f.offset() + f.inputs.len();
        let on_choice = f.offset() == 1 && f.cursor == 0;
        let multiline = f.current().is_some_and(|i| i.multiline);
        match k.code {
            KeyCode::Esc => return None,
            _ if newline(k) && multiline => {
                if let Some(i) = f.current() {
                    i.ed.insert("\n");
                }
            }
            KeyCode::Enter => self.save_fields(&mut f),
            KeyCode::Char('s') if ctrl(k) => self.save_fields(&mut f),
            KeyCode::Tab | KeyCode::Down => f.cursor = (f.cursor + 1) % rows.max(1),
            KeyCode::BackTab | KeyCode::Up => f.cursor = (f.cursor + rows.max(1) - 1) % rows.max(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if on_choice => {
                f.choice = (f.choice + 1) % f.choices.len();
                connector_inputs(&mut f);
                super::super::connections::option_fields(&mut f);
            }
            _ => {
                if let Some(i) = f.current() {
                    i.ed.key(k);
                }
            }
        }
        Some(Overlay::Fields(f))
    }

    fn save_fields(&mut self, f: &mut Fields) {
        if f.saving {
            return;
        }
        if let Some(i) = f.inputs.iter().find(|i| i.required && i.ed.text.trim().is_empty()) {
            f.error = Some(format!("{} is required.", i.label));
            return;
        }
        let values: serde_json::Map<String, Value> = f
            .inputs
            .iter()
            .filter(|i| !i.ed.text.trim().is_empty())
            .map(|i| (i.name.clone(), if i.secret { i.ed.text.as_str() } else { i.ed.text.trim() }.into()))
            .collect();
        f.saving = true;
        f.error = None;
        match &f.submit {
            Submit::Icon(target) => {
                let mut body = json!({"target": target, "tinted": f.choice == 1});
                if f.choice == 2 {
                    body["remove"] = true.into();
                } else if let Some(source) = values.get("source").and_then(Value::as_str) {
                    let key = if source.starts_with("https://") { "url" } else { "localFile" };
                    body[key] = source.into();
                }
                self.sheet("setIcon", body, Reply::FieldsSaved);
            }
            Submit::Connection(_) => self.submit_connection(f, &values),
            Submit::AgentEnv(backend) => {
                if values.is_empty() {
                    f.saving = false;
                    f.error = Some("Type at least one key.".into());
                    return;
                }
                self.sheet("setAgentEnv", json!({"backend": backend, "vars": values}), Reply::FieldsSaved);
            }
            Submit::Connector(item) => {
                let option = item["options"][f.choice]["id"].clone();
                self.sheet(
                    "installConnector",
                    json!({"registryName": item["name"], "option": option, "inputs": values}),
                    Reply::FieldsSaved,
                );
            }
            Submit::Import => self.sheet("importConnectors", json!({"config": values["config"]}), Reply::FieldsSaved),
            Submit::Handoff(entry) => {
                self.sheet("editHandoff", json!({"entryId": entry, "text": values["text"]}), Reply::FieldsSaved);
            }
            Submit::Section(bot) => {
                self.sheet("updateBot", json!({"id": bot, "section": values.get("name")}), Reply::FieldsSaved);
            }
            Submit::RenameSection(from) => {
                self.section_order = self.shown_section_names();
                let to = values.get("name").and_then(Value::as_str).map(str::to_owned);
                let r = Reply::SectionRenamed { from: from.clone(), to: to.clone() };
                self.sheet("renameSection", json!({"from": from, "to": to}), r);
            }
            Submit::RenameArtifact { bot, artifact } => {
                let body = json!({"botId": bot, "artifactId": artifact, "title": values["title"]});
                self.sheet("renameArtifact", body, Reply::FieldsSaved);
            }
        }
    }
}

/// Edits what a handoff card carries into the next conversation.
pub fn handoff_fields(entry: &Entry) -> Fields {
    Fields {
        title: "Edit the handoff".into(),
        note: "Your next message starts a fresh conversation that begins with this. ⇧↵ new line, ↵ saves.".into(),
        choices: vec![],
        choice: 0,
        inputs: vec![Input {
            name: "text".into(),
            label: "What it carries".into(),
            secret: false,
            required: true,
            multiline: true,
            placeholder: String::new(),
            ed: Editor::with(entry.data["handoff"]["text"].as_str().unwrap_or_default()),
        }],
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::Handoff(entry.id.clone()),
    }
}

/// Asks for a section name: where a bot goes, or a section's new name. Empty is allowed.
pub fn section_fields(title: &str, note: String, current: Option<&str>, submit: Submit) -> Fields {
    Fields {
        title: title.into(),
        note,
        choices: vec![],
        choice: 0,
        inputs: vec![Input {
            name: "name".into(),
            label: "Section".into(),
            secret: false,
            required: false,
            multiline: false,
            placeholder: "Section name".into(),
            ed: Editor::with(current.unwrap_or_default()),
        }],
        cursor: 0,
        error: None,
        saving: false,
        submit,
    }
}
