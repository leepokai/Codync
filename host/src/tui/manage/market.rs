//! The marketplace: agents, connectors and skills.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay, fuzzy};
use super::super::l10n::tr;
use super::{Fields, Input, Market, Reply, Row, Submit, TABS, Tab, ctrl, s};

/// Registry rows kept loaded under the cursor on Connectors, about the list's height.
const AHEAD: usize = 20;

impl Market {
    /// The registry page this list waits for: an answer for another search or cursor is a late one.
    pub(super) fn awaits(&self, search: &str, cursor: Option<&str>) -> bool {
        self.searched.as_deref().unwrap_or_default() == search && self.next.as_deref() == cursor
    }
}

impl App {
    pub fn open_market(&mut self) {
        self.call("refreshBackends", json!({}), After::Backends);
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
        self.sheet("marketSkills", json!({}), Reply::Skills);
        self.sheet("marketConnectors", json!({"search": ""}), Reply::Registry { search: String::new(), cursor: None });
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

    fn load_more_connectors(&self, m: &mut Market) {
        if m.loading || m.next.is_none() {
            return;
        }
        m.loading = true;
        let search = m.searched.clone().unwrap_or_default();
        let reply = Reply::Registry { search: search.clone(), cursor: m.next.clone() };
        self.sheet("marketConnectors", json!({"search": search, "cursor": m.next}), reply);
    }

    /// The cursor near the end of the Connectors list asks for the next registry page, and again as pages
    /// arrive while it stays near it (a page can have nothing to install). A page that failed is asked for
    /// again on the next move.
    pub(super) fn load_near_end(&self, m: &mut Market) {
        if m.tab == Tab::Connectors && m.cursor + AHEAD >= self.market_rows(m).len() {
            self.load_more_connectors(m);
        }
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
                self.sheet(
                    "marketConnectors",
                    json!({"search": search}),
                    Reply::Registry { search: search.clone(), cursor: None },
                );
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
                    title: tr("Remove {name}?").replace("{name}", &name),
                    detail: tr(if skill {
                        "Bots stop using this skill."
                    } else {
                        "Bots lose this connector, and the keys saved for it are deleted."
                    })
                    .into_owned(),
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
        self.load_near_end(&mut m);
        Some(Overlay::Market(m))
    }

    fn market_enter(&mut self, m: Box<Market>) -> Option<Overlay> {
        enum Act {
            Node,
            Agent(String),
            SignIn(String),
            Custom,
            Install(Value),
            Skill(Value, bool),
            None,
        }
        let act = match self.market_rows(&m).get(m.cursor) {
            Some(Row::Node) => Act::Node,
            Some(Row::Agent(b)) => Act::Agent(s(b, "id")),
            Some(Row::Installed(c)) if c["auth"] == "signedOut" => Act::SignIn(s(c, "id")),
            Some(Row::Custom) => Act::Custom,
            Some(Row::Registry(c)) => Act::Install((*c).clone()),
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
                self.open_agent(&id, false);
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
            // The connector's sheet, as on the apps: nothing installs until ↵ on it, even with nothing to fill in.
            Act::Install(item) => {
                let options = item["options"].as_array().cloned().unwrap_or_default();
                let host = self.host.clone();
                let title = s(&item, "title");
                let choices = options
                    .iter()
                    .map(|o| {
                        if o["kind"] == "remote" {
                            tr("Hosted by {name}").replace("{name}", &title)
                        } else {
                            tr("On {name} ({kind})").replace("{name}", &host).replace("{kind}", &s(o, "kind"))
                        }
                    })
                    .collect();
                let bare = options.len() == 1 && options[0]["inputs"].as_array().is_none_or(Vec::is_empty);
                let note = match (bare, options.first().is_some_and(|o| o["kind"] == "remote")) {
                    (true, true) => {
                        tr("No keys needed here. If {name} wants you to sign in, its sign-in page opens next.")
                            .replace("{name}", &title)
                    }
                    (true, false) => tr("No setup needed.").into_owned(),
                    (false, _) => tr("Keys are saved on {name} only.").replace("{name}", &host),
                };
                let mut f = Fields {
                    title: tr("Add connector").into_owned(),
                    note,
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

#[cfg(test)]
mod tests {
    use super::super::super::app::{App, Editor, Overlay};
    use super::super::super::net::Client;
    use super::super::{Market, Reply, Tab};
    use crossterm::event::{KeyCode, KeyEvent};
    use serde_json::{Value, json};

    fn registry(range: std::ops::Range<usize>, next: Option<&str>) -> Value {
        let items: Vec<Value> = range.map(|i| json!({"name": format!("c{i}"), "title": format!("C{i}")})).collect();
        json!({"items": items, "nextCursor": next})
    }

    fn open() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.open_market();
        app
    }

    fn market(app: &App) -> &Market {
        match app.overlays.last() {
            Some(Overlay::Market(m)) => m,
            _ => panic!("the marketplace is open"),
        }
    }

    /// The registry's answer for `search` at `cursor`.
    fn answer(app: &mut App, search: &str, cursor: Option<&str>, res: Result<Value, String>) {
        app.on_sheet_reply(Reply::Registry { search: search.into(), cursor: cursor.map(str::to_owned) }, res);
    }

    /// The cursor on Connectors row `row`, as arrow keys leave it.
    fn move_to(app: &mut App, row: usize) {
        let Some(Overlay::Market(mut m)) = app.overlays.pop() else { panic!("the marketplace is open") };
        m.tab = Tab::Connectors;
        m.cursor = row;
        app.load_near_end(&mut m);
        app.overlays.push(Overlay::Market(m));
    }

    #[tokio::test]
    async fn connectors_load_more_as_the_cursor_nears_the_end() {
        let mut app = open();
        answer(&mut app, "", None, Ok(registry(0..30, Some("a"))));
        // Custom connector, then 30 registry rows: a screenful still lies under row 0.
        move_to(&mut app, 0);
        assert!(!market(&app).loading);

        move_to(&mut app, 12);
        assert!(market(&app).loading, "near the end, the next page is asked for");
        answer(&mut app, "", Some("a"), Ok(registry(30..30, Some("b"))));
        assert!(market(&app).loading, "a page with nothing to install reads on");
        answer(&mut app, "", Some("b"), Ok(registry(30..40, Some("c"))));
        assert!(!market(&app).loading);
        assert_eq!(app.market_rows(market(&app)).len(), 41);

        // A failed page keeps the rows; the next move asks again, until the cursor ends.
        move_to(&mut app, 25);
        answer(&mut app, "", Some("c"), Err("offline".into()));
        assert_eq!((market(&app).loading, market(&app).error.as_deref()), (false, Some("offline")));
        move_to(&mut app, 26);
        assert!(market(&app).loading);
        answer(&mut app, "", Some("c"), Ok(registry(40..41, None)));
        move_to(&mut app, 41);
        assert!(!market(&app).loading && market(&app).next.is_none());
    }

    #[tokio::test]
    async fn answers_for_an_older_search_are_dropped() {
        let mut app = open();
        move_to(&mut app, 0);
        answer(&mut app, "", None, Ok(registry(0..5, Some("a"))));
        assert!(market(&app).loading, "page a is on its way");

        let Some(Overlay::Market(mut m)) = app.overlays.pop() else { panic!("the marketplace is open") };
        m.query = Editor::with("linear");
        let m = app.market_key(m, KeyEvent::from(KeyCode::Enter)).expect("the marketplace stays open");
        app.overlays.push(m);
        answer(&mut app, "", Some("a"), Ok(registry(5..10, Some("b"))));
        answer(&mut app, "", Some("a"), Err("offline".into()));
        let m = market(&app);
        assert!(m.registry.is_empty() && m.next.is_none() && m.loading && m.error.is_none());

        answer(&mut app, "linear", None, Ok(registry(0..2, None)));
        assert_eq!(market(&app).registry.len(), 2);
    }

    #[tokio::test]
    async fn opening_a_connector_or_a_new_agent_adds_nothing() {
        let mut app = open();
        let bare = json!({"name": "c0", "title": "C0", "options": [{"id": "npm", "kind": "npm", "inputs": []}]});
        answer(&mut app, "", None, Ok(json!({"items": [bare], "nextCursor": null})));
        // Custom connector, then the registry row.
        move_to(&mut app, 1);
        let Some(Overlay::Market(m)) = app.overlays.pop() else { panic!("the marketplace is open") };
        assert!(app.market_key(m, KeyEvent::from(KeyCode::Enter)).is_none());
        assert!(matches!(app.overlays.last(), Some(Overlay::Fields(_))), "its sheet opens; ↵ there adds it");

        app.backends =
            vec![json!({"id": "kimi", "name": "Kimi", "installed": false, "curated": false, "signedIn": null})];
        app.open_agent("kimi", false);
        let Some(Overlay::Agent(a)) = app.overlays.last() else { panic!("the agent sheet is open") };
        assert!(!a.busy && !app.agent_set_up(a), "a registry agent that never ran waits for Set up");
    }
}
