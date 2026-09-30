//! Connection requests remain actionable in a terminal. Secrets stay in masked setup fields.
use super::app::{After, App, Editor, Entry, Overlay};
use super::manage::{Fields, Input, Submit};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum Step {
    Credentials,
    Catalog,
    Install,
    Installed,
    Authorize,
    SignIn,
    AppStart,
    AppCheck,
    Finish,
}

pub struct Connection {
    pub entry: String,
    pub request: Value,
    pub step: Step,
    pub metadata: Value,
}

fn input(name: &str, label: &str, secret: bool, required: bool) -> Input {
    Input {
        name: name.into(),
        label: label.into(),
        secret,
        required,
        multiline: false,
        placeholder: String::new(),
        ed: Editor::default(),
    }
}

pub fn fields(entry: &Entry) -> Fields {
    let request = &entry.data["connectionRequest"];
    let (step, inputs) = match request["kind"].as_str() {
        Some("login") => (
            Step::Credentials,
            vec![input("username", "Username / email", false, true), input("value", "Password / op://", true, true)],
        ),
        Some("secret") => (
            Step::Credentials,
            vec![input("value", request["field"].as_str().unwrap_or("Credential / op://"), true, true)],
        ),
        Some("app") => (Step::AppStart, vec![]),
        _ if request["connectorId"].is_string() => (Step::Installed, vec![]),
        _ => (Step::Catalog, vec![]),
    };
    Fields {
        title: request["title"].as_str().unwrap_or("Connect service").into(),
        note: "Saved securely; never added to chat. Ctrl+X cancels the request; Esc closes.".into(),
        choices: vec![],
        choice: 0,
        inputs,
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::Connection(Box::new(Connection {
            entry: entry.id.clone(),
            request: request.clone(),
            step,
            metadata: Value::Null,
        })),
    }
}

pub fn option_fields(f: &mut Fields) {
    let Submit::Connection(c) = &f.submit else { return };
    if !matches!(c.step, Step::Install) {
        return;
    }
    f.inputs = c.metadata["options"][f.choice]["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let name = v["name"].as_str().unwrap_or("");
            let mut i = input(name, name, v["secret"] == true, v["required"] == true);
            i.ed = Editor::with(v["default"].as_str().unwrap_or(""));
            i.placeholder = v["description"].as_str().unwrap_or("").into();
            i
        })
        .collect();
    f.cursor = 0;
}

impl App {
    pub(super) fn open_connection(&mut self) {
        let Some(bot) = &self.selected else { return };
        let entry = self
            .lane(bot)
            .into_iter()
            .find(|e| {
                e.data["connectionRequest"]["status"] == "pending" && self.pick.as_ref().is_none_or(|id| *id == e.id)
            })
            .cloned();
        if let Some(entry) = entry {
            self.overlays.push(Overlay::Fields(Box::new(fields(&entry))));
        } else {
            self.flash("No pending connection request here");
        }
    }

    pub(super) fn submit_connection(&self, f: &Fields, values: &serde_json::Map<String, Value>) {
        let Submit::Connection(c) = &f.submit else { return };
        let (method, args, reply) = match c.step {
            Step::Credentials => (
                "connectorRequestFinish",
                json!({"entryId":c.entry,"value":values.get("value"),"username":values.get("username")}),
                Step::Finish,
            ),
            Step::Catalog => ("connectorInfo", json!({"registryName":c.request["registryName"]}), Step::Catalog),
            Step::Install => (
                "installConnector",
                json!({"registryName":c.metadata["name"],"option":c.metadata["options"][f.choice]["id"],"inputs":values}),
                Step::Install,
            ),
            Step::Installed => ("connectors", json!({}), Step::Installed),
            Step::Authorize if f.choice == 0 => {
                ("connectorSignIn", json!({"id":c.request["connectorId"]}), Step::SignIn)
            }
            Step::AppStart => ("composioConnect", json!({"toolkit":c.request["toolkit"]}), Step::AppStart),
            Step::AppCheck if c.metadata["status"] == "needsFields" => (
                "composioConnectFields",
                json!({"toolkit":c.request["toolkit"],"mode":c.metadata["mode"],"fields":values}),
                Step::AppCheck,
            ),
            Step::AppCheck => ("composioConnection", json!({"id":c.metadata["connection"]}), Step::AppCheck),
            _ => (
                "connectorRequestFinish",
                json!({"entryId":c.entry,"connectorId":c.request["connectorId"]}),
                Step::Finish,
            ),
        };
        self.call(method, args, After::Connection(c.entry.clone(), reply));
    }

    pub(super) fn connection_reply(&mut self, id: &str, step: Step, result: Result<Value, String>) {
        let Some(index) = self.overlays.iter().position(
            |o| matches!(o, Overlay::Fields(f) if matches!(&f.submit, Submit::Connection(c) if c.entry == id)),
        ) else {
            return;
        };
        let Overlay::Fields(f) = &mut self.overlays[index] else { return };
        f.saving = false;
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                f.error = Some(e);
                return;
            }
        };
        f.error = None;
        let Submit::Connection(c) = &mut f.submit else { return };
        let mut finish = false;
        match step {
            Step::Finish => {
                self.overlays.remove(index);
                self.flash("Connection request updated");
                return;
            }
            Step::Catalog => {
                f.choices = value["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| o["id"].as_str().unwrap_or("Setup").to_owned())
                    .collect();
                if f.choices.is_empty() {
                    f.error = Some("No supported setup options".into());
                    return;
                }
                c.metadata = value;
                c.step = Step::Install;
                f.choice = 0;
                option_fields(f);
            }
            Step::Install | Step::Installed => {
                let connector = if matches!(step, Step::Install) {
                    value["connector"].clone()
                } else {
                    value["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|v| v["id"] == c.request["connectorId"])
                        .cloned()
                        .unwrap_or(Value::Null)
                };
                if connector.is_null() {
                    f.error = Some("Connector is no longer installed".into());
                    return;
                }
                c.request["connectorId"] = connector["id"].clone();
                for i in &mut f.inputs {
                    i.ed.clear();
                }
                f.inputs.clear();
                f.cursor = 0;
                if connector["auth"] == "signedOut" {
                    c.step = Step::Authorize;
                    f.choices = vec!["Sign in".into(), "Check connection".into()];
                    f.choice = 0;
                } else {
                    c.step = Step::Finish;
                    finish = true;
                }
            }
            Step::SignIn => {
                c.step = Step::Authorize;
                f.choice = 1;
                f.note = "Sign-in link copied. Finish in browser, then Enter checks. Ctrl+X cancels.".into();
                if let Some(url) = value["url"].as_str() {
                    super::app::copy(url);
                    super::manage::open_browser(url, &self.url);
                }
            }
            Step::AppStart | Step::AppCheck => {
                if value["status"] == "active" {
                    c.request["connectorId"] =
                        format!("composio-{}", c.request["toolkit"].as_str().unwrap_or("")).into();
                    c.step = Step::Finish;
                    f.inputs.clear();
                    finish = true;
                } else {
                    if matches!(step, Step::AppStart) || value["status"] == "needsFields" {
                        c.metadata = value.clone();
                        f.inputs = value["fields"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| {
                                input(
                                    v["name"].as_str().unwrap_or(""),
                                    v["label"].as_str().unwrap_or("Credential"),
                                    v["secret"] != false,
                                    v["required"] != false,
                                )
                            })
                            .collect();
                    }
                    c.step = Step::AppCheck;
                    f.note = "Complete sign-in, then Enter checks. Ctrl+X cancels.".into();
                    if let Some(url) = value["url"].as_str() {
                        super::app::copy(url);
                        super::manage::open_browser(url, &self.url);
                    }
                    if matches!(step, Step::AppCheck) {
                        f.error = Some(format!(
                            "Connection status: {}. Finish signing in and check again.",
                            value["status"].as_str().unwrap_or("pending")
                        ));
                    }
                }
            }
            Step::Credentials | Step::Authorize => {}
        }
        if finish {
            f.saving = true;
            let Submit::Connection(c) = &f.submit else { return };
            let args = json!({"entryId":c.entry,"connectorId":c.request["connectorId"]});
            self.call("connectorRequestFinish", args, After::Connection(id.into(), Step::Finish));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::app::Kind;
    use super::*;
    async fn fixture() -> (
        App,
        tokio::sync::mpsc::UnboundedReceiver<super::super::app::Msg>,
        std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        use axum::{Json, Router, extract::Path, routing::post};
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received = calls.clone();
        let router = Router::new().route("/api/{method}", post(move |Path(method): Path<String>, Json(body): Json<Value>| {
            let received = received.clone();
            async move {
                received.lock().unwrap().push(json!({"method":method,"body":body}));
                Json(match method.as_str() {
                    "connectorInfo" => json!({"name":"fixture","options":[{"id":"local","inputs":[{"name":"key","secret":true,"required":true}]}]}),
                    "installConnector" => json!({"connector":{"id":"connector","auth":"signedOut"}}),
                    "composioConnect" => json!({"status":"needsFields","mode":"apiKey","fields":[{"name":"token","label":"Token","secret":true}]}),
                    "composioConnectFields" => json!({"status":"active"}),
                    _ => json!({}),
                })
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (App::new(super::super::net::Client::new(&url, Some("fixture")), tx, url), rx, calls, task)
    }

    fn open(app: &mut App, request: &Value) {
        let e = Entry {
            id: "request".into(),
            seq: 1,
            turn: 1,
            kind: Kind::Notice,
            data: json!({"connectionRequest":request}),
            created_at: 0,
            thread_id: None,
        };
        app.overlays.push(Overlay::Fields(Box::new(fields(&e))));
    }

    fn key(app: &mut App, code: crossterm::event::KeyCode, modifiers: crossterm::event::KeyModifiers) {
        app.on_key(crossterm::event::KeyEvent::new(code, modifiers));
    }

    async fn reply(app: &mut App, rx: &mut tokio::sync::mpsc::UnboundedReceiver<super::super::app::Msg>) {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
        app.on_msg(msg);
    }

    #[tokio::test]
    async fn credentials_submit_exact_secret_and_cancel_is_explicit() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let (mut app, mut rx, calls, server) = fixture().await;
        open(&mut app, &json!({"kind":"login","status":"pending"}));
        let Some(Overlay::Fields(f)) = app.overlays.last_mut() else { panic!("fields") };
        f.inputs[0].ed = Editor::with("name");
        f.inputs[1].ed = Editor::with(" secret with spaces ");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        assert!(app.overlays.is_empty());
        assert_eq!(
            calls.lock().unwrap()[0]["body"],
            json!({"entryId":"request","username":"name","value":" secret with spaces "})
        );
        open(&mut app, &json!({"kind":"secret","status":"pending"}));
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(calls.lock().unwrap().len(), 1);
        open(&mut app, &json!({"kind":"secret","status":"pending"}));
        key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        reply(&mut app, &mut rx).await;
        assert_eq!(calls.lock().unwrap()[1]["body"], json!({"entryId":"request","cancel":true}));
        server.abort();
    }

    #[tokio::test]
    async fn registry_request_installs_signs_in_and_finishes() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let (mut app, mut rx, calls, server) = fixture().await;
        open(&mut app, &json!({"kind":"connector","status":"pending","registryName":"fixture"}));
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        let Some(Overlay::Fields(f)) = app.overlays.last_mut() else { panic!("fields") };
        assert!(f.inputs[0].secret);
        f.inputs[0].ed = Editor::with("fixture-key");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        let calls = calls.lock().unwrap();
        let methods: Vec<_> = calls.iter().map(|c| c["method"].as_str().unwrap()).collect();
        assert_eq!(methods, ["connectorInfo", "installConnector", "connectorSignIn", "connectorRequestFinish"]);
        assert_eq!(calls[1]["body"]["inputs"]["key"], "fixture-key");
        assert_eq!(calls[3]["body"]["connectorId"], "connector");
        assert!(app.overlays.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn hosted_app_fields_finish_only_after_active_status() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let (mut app, mut rx, calls, server) = fixture().await;
        open(&mut app, &json!({"kind":"app","status":"pending","toolkit":"fixture"}));
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        let Some(Overlay::Fields(f)) = app.overlays.last_mut() else { panic!("fields") };
        assert!(f.inputs[0].secret);
        f.inputs[0].ed = Editor::with("fixture-token");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        reply(&mut app, &mut rx).await;
        reply(&mut app, &mut rx).await;
        assert!(app.overlays.is_empty());
        assert_eq!(calls.lock().unwrap()[2]["body"]["connectorId"], "composio-fixture");
        server.abort();
    }

    #[test]
    fn login_fields_mask_the_password_and_keep_it_out_of_chat() {
        let e = Entry {
            id: "r".into(),
            seq: 1,
            turn: 1,
            kind: Kind::Notice,
            data: json!({"connectionRequest":{"kind":"login","status":"pending"}}),
            created_at: 0,
            thread_id: None,
        };
        let f = fields(&e);
        assert!(!f.inputs[0].secret);
        assert!(f.inputs[1].secret);
        assert!(f.inputs.iter().all(|i| i.ed.text.is_empty()));
        assert!(matches!(f.submit, Submit::Connection(c) if matches!(c.step, Step::Credentials)));
    }
}
