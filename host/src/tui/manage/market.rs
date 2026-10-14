//! The marketplace: agents, connectors and skills.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay, fuzzy};
use super::{Fields, Input, Market, Reply, Row, Submit, TABS, Tab, ctrl, s};

impl App {
    pub fn open_market(&mut self) {
        self.call("refreshBackends", json!({}), After::Backends);
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
        self.sheet("marketSkills", json!({}), Reply::Skills);
        self.sheet("marketConnectors", json!({"search": ""}), Reply::Registry { more: false });
        self.overlays.push(Overlay::Market(Box::new(Market {
            tab: Tab::Agents,
            query: Editor::default(),
            searched: None,
            registry: vec![],
            next: None,
            skills: None,
            loading: true,
            cursor: 0,
            error: None,
        })));
    }

    /// The marketplace's rows on its current tab, filtered by the query.
    pub fn market_rows<'a>(&'a self, m: &'a Market) -> Vec<Row<'a>> {
        let q = m.query.text.trim();
        let hit = |v: &Value, keys: &[&str]| {
            let fields: Vec<&str> = keys.iter().map(|k| v[*k].as_str().unwrap_or_default()).collect();
            fuzzy(q, &fields)
        };
        match m.tab {
            Tab::Agents => {
                let agents = self
                    .backends
                    .iter()
                    .filter(|b| b["available"] == true || b["installed"] == true || b["curated"] == true);
                let node = agents.clone().any(|b| b["node"] == "missing").then_some(Row::Node);
                node.into_iter().chain(agents.filter(|b| hit(b, &["name", "id"])).map(Row::Agent)).collect()
            }
            Tab::Connectors => {
                let installed: Vec<&str> = self.plugins.0.iter().filter_map(|c| c["registryName"].as_str()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.0.iter().filter(|c| hit(c, &["name", "description"])).map(Row::Installed).collect();
                rows.push(Row::Custom);
                rows.extend(
                    m.registry
                        .iter()
                        .filter(|c| !installed.contains(&c["name"].as_str().unwrap_or_default()))
                        .map(Row::Registry),
                );
                if m.next.is_some() {
                    rows.push(Row::More);
                }
                rows
            }
            Tab::Skills => {
                let installed: Vec<String> = self.plugins.1.iter().map(|v| s(v, "id").to_lowercase()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.1.iter().filter(|v| hit(v, &["name", "description"])).map(Row::OwnSkill).collect();
                rows.extend(
                    m.skills
                        .iter()
                        .flatten()
                        .filter(|v| !installed.contains(&s(v, "source").to_lowercase()))
                        .filter(|v| hit(v, &["name", "description"]))
                        .map(Row::Skill),
                );
                rows
            }
        }
    }

    /// The query no longer matches what the registry list shows: ↵ searches first.
    pub fn market_stale(m: &Market) -> bool {
        m.tab == Tab::Connectors && m.searched.as_deref().is_some_and(|s| s != m.query.text.trim())
    }

    pub(in crate::tui) fn market_key(&mut self, mut m: Box<Market>, k: KeyEvent) -> Option<Overlay> {
        let n = self.market_rows(&m).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Tab | KeyCode::BackTab => {
                let i = TABS.iter().position(|(t, _)| *t == m.tab).unwrap_or(0);
                let d = if k.code == KeyCode::Tab { 1 } else { TABS.len() - 1 };
                m.tab = TABS[(i + d) % TABS.len()].0;
                m.cursor = 0;
            }
            KeyCode::Down => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('n' | 'j') if ctrl(k) => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Char('p') if ctrl(k) => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('r') if ctrl(k) => {
                self.call("connectors", json!({}), After::Connectors);
                self.call("skills", json!({}), After::Skills);
                self.call("refreshBackends", json!({}), After::Backends);
            }
            KeyCode::Enter if Self::market_stale(&m) => {
                let search = m.query.text.trim().to_owned();
                m.loading = true;
                m.cursor = 0;
                m.registry.clear();
                m.next = None;
                self.sheet("marketConnectors", json!({"search": search}), Reply::Registry { more: false });
                m.searched = Some(search);
            }
            KeyCode::Enter => return self.market_enter(m),
            KeyCode::Char('x') if ctrl(k) => {
                let (id, name, skill) = match self.market_rows(&m).get(m.cursor) {
                    Some(Row::Installed(c)) => (s(c, "id"), s(c, "name"), false),
                    Some(Row::OwnSkill(v)) => (s(v, "id"), s(v, "name"), true),
                    _ => return Some(Overlay::Market(m)),
                };
                let c = Confirm {
                    title: format!("Remove {name}?"),
                    detail: if skill {
                        "Bots stop using this skill.".into()
                    } else {
                        "Bots lose it, and the keys saved for it are deleted.".into()
                    },
                    note: String::new(),
                    button: "Remove",
                    act: if skill { ConfirmAct::RemoveSkill(id) } else { ConfirmAct::RemoveConnector(id) },
                };
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {
                if m.query.key(k) {
                    m.cursor = 0;
                }
            }
        }
        Some(Overlay::Market(m))
    }

    fn market_enter(&mut self, mut m: Box<Market>) -> Option<Overlay> {
        enum Act {
            Node,
            Agent(String),
            SignIn(String),
            Custom,
            Install(Value),
            More,
            Skill(Value, bool),
            None,
        }
        let act = match self.market_rows(&m).get(m.cursor) {
            Some(Row::Node) => Act::Node,
            Some(Row::Agent(b)) => Act::Agent(s(b, "id")),
            Some(Row::Installed(c)) if c["auth"] == "signedOut" => Act::SignIn(s(c, "id")),
            Some(Row::Custom) => Act::Custom,
            Some(Row::Registry(c)) => Act::Install((*c).clone()),
            Some(Row::More) => Act::More,
            Some(Row::Skill(v)) => Act::Skill((*v).clone(), true),
            Some(Row::OwnSkill(v)) => Act::Skill((*v).clone(), false),
            _ => Act::None,
        };
        match act {
            Act::Node => {
                let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                self.start_setup("", "node", None, &json!({"cols": cols, "rows": rows}));
            }
            Act::Agent(id) => {
                self.overlays.push(Overlay::Market(m));
                self.open_agent(&id);
                return None;
            }
            Act::SignIn(id) => {
                self.flash("Starting the sign-in…");
                self.sheet("connectorSignIn", json!({"id": id}), Reply::SignIn);
            }
            Act::Custom => {
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Fields(Box::new(Fields {
                    title: "Custom connector".into(),
                    note: "Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added.".into(),
                    choices: vec![],
                    choice: 0,
                    inputs: vec![Input {
                        name: "config".into(),
                        label: "Config".into(),
                        secret: false,
                        required: true,
                        multiline: true,
                        placeholder: r#"{"mcpServers": {"name": {"command": "npx", "args": ["-y", "…"]}}}"#.into(),
                        ed: Editor::default(),
                    }],
                    cursor: 0,
                    error: None,
                    saving: false,
                    submit: Submit::Import,
                })));
                return None;
            }
            Act::Install(item) => {
                let options = item["options"].as_array().cloned().unwrap_or_default();
                if options.len() == 1 && options[0]["inputs"].as_array().is_none_or(Vec::is_empty) {
                    self.flash(&format!("Adding {}…", s(&item, "title")));
                    self.sheet(
                        "installConnector",
                        json!({"registryName": item["name"], "option": options[0]["id"], "inputs": {}}),
                        Reply::FieldsSaved,
                    );
                } else {
                    let host = self.host.clone();
                    let choices = options
                        .iter()
                        .map(|o| {
                            if o["kind"] == "remote" {
                                format!("Hosted by {}", s(&item, "title"))
                            } else {
                                format!("On {host} ({})", s(o, "kind"))
                            }
                        })
                        .collect();
                    let mut f = Fields {
                        title: format!("Add {}", s(&item, "title")),
                        note: format!("Keys are saved on {host} only."),
                        choices,
                        choice: 0,
                        inputs: vec![],
                        cursor: 0,
                        error: None,
                        saving: false,
                        submit: Submit::Connector(item),
                    };
                    connector_inputs(&mut f);
                    super::super::connections::option_fields(&mut f);
                    self.overlays.push(Overlay::Market(m));
                    self.overlays.push(Overlay::Fields(Box::new(f)));
                    return None;
                }
            }
            Act::More => {
                m.loading = true;
                let search = m.searched.clone().unwrap_or_default();
                self.sheet(
                    "marketConnectors",
                    json!({"search": search, "cursor": m.next}),
                    Reply::Registry { more: true },
                );
            }
            Act::Skill(v, catalog) => {
                self.overlays.push(Overlay::Market(m));
                self.open_skill(&v, catalog);
                return None;
            }
            Act::None => {}
        }
        Some(Overlay::Market(m))
    }
}

/// The inputs of a connector's chosen way to run, defaults filled in.
pub(super) fn connector_inputs(f: &mut Fields) {
    let Submit::Connector(item) = &f.submit else { return };
    f.inputs = item["options"][f.choice]["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let name = s(i, "name");
            let description = s(i, "description");
            Input {
                label: name.clone(),
                name,
                secret: i["secret"] == true,
                required: i["required"] == true,
                multiline: false,
                placeholder: if description.is_empty() { s(i, "placeholder") } else { description },
                ed: Editor::with(i["default"].as_str().unwrap_or_default()),
            }
        })
        .collect();
    f.cursor = f.cursor.min(f.offset() + f.inputs.len().saturating_sub(1));
}
