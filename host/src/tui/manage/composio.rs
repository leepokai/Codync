//! Composio's apps in the terminal: the key once, then its sign-in page (or the app's own key
//! fields) per app. The host does the talking; this only collects what it asks for.

use super::super::app::{After, App, Editor, Overlay};
use super::super::l10n::tr;
use super::{Fields, Input, Reply, Submit, s};
use serde_json::{Value, json};

/// Where connecting an app stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::tui) enum Step {
    /// Asking Composio what connecting the app needs.
    Start,
    /// The key fields a key-based app declares.
    Fields,
    /// The sign-in page is open; ↵ checks again.
    Check,
    /// Connected: ↵ closes.
    Done,
}

/// One app being connected, and how far it got.
pub(in crate::tui) struct Composio {
    pub app: Value,
    pub step: Step,
    /// Composio's connection, once it has one.
    pub connection: Option<String>,
    /// The scheme a key-based app wants its fields in.
    pub mode: String,
}

fn input(name: &str, label: &str, secret: bool, required: bool, placeholder: &str) -> Input {
    Input {
        name: name.into(),
        label: label.into(),
        secret,
        required,
        multiline: false,
        placeholder: placeholder.into(),
        ed: Editor::default(),
    }
}

/// The one key that makes Composio's apps available on this computer.
pub(super) fn key_fields(status: Option<&Value>, host: &str) -> Fields {
    let configured = status.is_some_and(|v| v["configured"] == true);
    let url = status.and_then(|v| v["keyUrl"].as_str()).unwrap_or("https://platform.composio.dev");
    let mut note = tr(
        "Composio signs you in to apps like Gmail and Slack and runs their tools for your bots. The key stays on {name}.",
    )
    .replace("{name}", host);
    note.push_str("\n\n");
    note.push_str(&tr("Get a key from Composio"));
    note.push_str(": ");
    note.push_str(url);
    if configured {
        note.push_str("\n\n");
        note.push_str(&tr("Remove key"));
        note.push_str(": ↵");
    }
    Fields {
        title: tr("Composio").into_owned(),
        note,
        choices: vec![],
        choice: 0,
        inputs: vec![input("key", &tr("Composio API key"), true, !configured, &tr("API key"))],
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::ComposioKey,
    }
}

/// Connecting one app: Composio's page, or the fields a key-based app declares.
pub(super) fn app_fields(app: &Value) -> Fields {
    Fields {
        title: format!("{} {}", tr("Connect"), s(app, "name")),
        note: s(app, "description"),
        choices: vec![],
        choice: 0,
        inputs: vec![],
        cursor: 0,
        error: None,
        saving: true,
        submit: Submit::Composio(Box::new(Composio {
            app: app.clone(),
            step: Step::Start,
            connection: None,
            mode: String::new(),
        })),
    }
}

impl App {
    /// Opens the app's sheet and asks Composio what it needs.
    pub(super) fn connect_composio(&mut self, app: Value) {
        let fields = app_fields(&app);
        self.call("composioConnect", json!({"toolkit": s(&app, "slug")}), After::Composio(Step::Start));
        self.overlays.push(Overlay::Fields(Box::new(fields)));
    }

    pub(super) fn submit_composio(&mut self, f: &Fields, values: &serde_json::Map<String, Value>) {
        let Submit::Composio(c) = &f.submit else { return };
        let toolkit = s(&c.app, "slug");
        match (c.step, &c.connection) {
            (Step::Fields, _) => self.call(
                "composioConnectFields",
                json!({"toolkit": toolkit, "mode": c.mode, "fields": values}),
                After::Composio(Step::Fields),
            ),
            (Step::Check, Some(id)) => {
                self.call("composioConnection", json!({"id": id}), After::Composio(Step::Check));
            }
            (Step::Done, _) => {}
            (Step::Start | Step::Check, _) => {
                self.call("composioConnect", json!({"toolkit": toolkit}), After::Composio(Step::Start));
            }
        }
    }

    pub(super) fn submit_composio_key(&mut self, values: &serde_json::Map<String, Value>) {
        let key = values.get("key").cloned().unwrap_or_default();
        self.sheet("setComposioKey", json!({"key": key}), Reply::ComposioKey);
    }

    pub(in crate::tui) fn composio_reply(&mut self, step: Step, result: Result<Value, String>) {
        let Some(index) = self
            .overlays
            .iter()
            .rposition(|o| matches!(o, Overlay::Fields(f) if matches!(&f.submit, Submit::Composio(_))))
        else {
            return;
        };
        let Some((_slug, name)) = self.overlays.get(index).and_then(|o| match o {
            Overlay::Fields(f) => match &f.submit {
                Submit::Composio(c) => Some((s(&c.app, "slug"), s(&c.app, "name"))),
                _ => None,
            },
            _ => None,
        }) else {
            return;
        };
        let value = match result {
            Ok(v) => v,
            Err(error) => {
                if let Some(Overlay::Fields(f)) = self.overlays.get_mut(index) {
                    f.saving = false;
                    f.error = Some(error);
                }
                return;
            }
        };
        let status = value["status"].as_str().unwrap_or_default();
        if status == "active" {
            self.flash(&format!("{name}: {}", tr("Connected")));
            self.call("connectors", json!({}), After::Connectors);
            // The status also re-reads the list, where the app now shows as connected.
            self.reload_composio();
            if let Some(Overlay::Fields(f)) = self.overlays.get_mut(index) {
                f.saving = false;
                f.error = None;
                f.inputs.clear();
                f.cursor = 0;
                let Submit::Composio(c) = &mut f.submit else { return };
                c.step = Step::Done;
                f.note = tr("Turn {name} on for a bot in the bot's settings, under Connectors.").replace("{name}", &name);
            }
            return;
        }
        let mut url = None;
        if let Some(Overlay::Fields(f)) = self.overlays.get_mut(index) {
            f.saving = false;
            f.error = None;
            let host = self.host.clone();
            let Submit::Composio(c) = &mut f.submit else { return };
            if let Some(id) = value["connection"].as_str().or_else(|| value["id"].as_str()) {
                c.connection = Some(id.to_owned());
            }
            match status {
                "needsFields" => {
                    c.step = Step::Fields;
                    if let Some(mode) = value["mode"].as_str() {
                        mode.clone_into(&mut c.mode);
                    }
                    f.inputs = value["fields"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|v| {
                            input(
                                v["name"].as_str().unwrap_or_default(),
                                &tr(v["label"].as_str().unwrap_or("API key")),
                                v["secret"] != false,
                                v["required"] != false,
                                v["description"].as_str().unwrap_or_default(),
                            )
                        })
                        .collect();
                    f.cursor = 0;
                    f.note = tr("Keys are saved on {name} only.").replace("{name}", &host);
                    f.note.push_str("\n");
                    f.note.push_str(&tr("Save"));
                    f.note.push_str(": ↵");
                }
                "redirect" => {
                    c.step = Step::Check;
                    f.inputs.clear();
                    f.cursor = 0;
                    url = value["url"].as_str().map(str::to_owned);
                    f.note = tr("Sign-in link copied: open it in a browser, then ↵ checks.").into_owned();
                    if let Some(url) = &url {
                        f.note.push_str("\n\n");
                        f.note.push_str(url);
                    }
                }
                _ if step == Step::Check => {
                    if matches!(status, "failed" | "expired") {
                        c.connection = None;
                        f.note = tr("That sign-in didn't finish. ↵ starts it again.").into_owned();
                    } else {
                        f.note = tr("Waiting for you to finish signing in…").into_owned();
                        f.error = Some(format!("{}: {status}", tr("Status")));
                    }
                    f.note.push_str(" · ↵");
                }
                _ => {}
            }
        }
        if let Some(url) = url {
            super::super::app::copy(&url);
            super::open_browser(&url, &self.url);
        }
    }

    /// Asks the host for the key's state and, when there is one, the apps again.
    pub(super) fn reload_composio(&self) {
        self.sheet("composioStatus", json!({}), Reply::ComposioStatus);
    }
}
