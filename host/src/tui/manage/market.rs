//! The marketplace: agents, connectors and skills.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay, fuzzy};
use super::super::l10n::tr;
use super::{Fields, Input, Market, MarketPage, Reply, Row, Submit, TABS, Tab, ctrl, s};

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
            pages: [0; 3],
            page_size: PAGE_SIZE,
        })));
    }

    /// The open tab's page: the rows every page starts with, then this page's catalog rows.
    pub fn market_page<'a>(&'a self, m: &'a Market) -> MarketPage<'a> {
        let (mut rows, catalog) = self.market_catalog(m, m.tab);
        let more = m.tab == Tab::Connectors && m.next.is_some();
        let count = catalog.len();
        let page = clamp_page(m.pages[m.tab.index()], count, m.page_size, more);
        rows.extend(catalog.into_iter().skip(page * m.page_size).take(m.page_size));
        MarketPage { rows, count, page, pages: page_count(count, m.page_size, more) }
    }

    fn connector_count(&self, m: &Market) -> usize {
        self.market_catalog(m, Tab::Connectors).1.len()
    }

    fn load_more_connectors(&self, m: &mut Market) {
        if m.loading || m.next.is_none() {
            return;
        }
        m.loading = true;
        let search = m.searched.clone().unwrap_or_default();
        self.sheet("marketConnectors", json!({"search": search, "cursor": m.next}), Reply::Registry { more: true });
    }

    /// After a connector page turns or a registry page arrives, keep fetching until that page is full or the cursor ends.
    /// The Connectors page fills even while another tab is open.
    pub(super) fn continue_market_page(&self, m: &mut Market) {
        let count = self.connector_count(m);
        let i = Tab::Connectors.index();
        if m.next.is_some() && needs_fetch(count, m.pages[i], m.page_size) {
            self.load_more_connectors(m);
        } else {
            m.pages[i] = clamp_page(m.pages[i], count, m.page_size, m.next.is_some());
        }
    }

    /// A registry page that didn't come: back to the last Connectors page with rows.
    pub(super) fn settle_connector_page(&self, m: &mut Market) {
        let i = Tab::Connectors.index();
        m.pages[i] = clamp_page(m.pages[i], self.connector_count(m), m.page_size, false);
    }

    fn turn_market_page(&self, m: &mut Market, forward: bool) {
        let MarketPage { page, pages, .. } = self.market_page(m);
        let next = if forward { page + 1 } else { page.saturating_sub(1) };
        if next == page || next >= pages {
            return;
        }
        m.pages[m.tab.index()] = next;
        m.cursor = 0;
        if m.tab == Tab::Connectors {
            self.continue_market_page(m);
        }
    }

    /// A tab's rows, split into the ones every page starts with and the catalog rows that are paged.
    fn market_catalog<'a>(&'a self, m: &'a Market, tab: Tab) -> (Vec<Row<'a>>, Vec<Row<'a>>) {
        let q = m.query.text.trim();
        let hit = |v: &Value, keys: &[&str]| {
            let fields: Vec<&str> = keys.iter().map(|k| v[*k].as_str().unwrap_or_default()).collect();
            fuzzy(q, &fields)
        };
        match tab {
            Tab::Agents => {
                let agents = self
                    .backends
                    .iter()
                    .filter(|b| b["available"] == true || b["installed"] == true || b["curated"] == true);
                let node = agents.clone().any(|b| b["node"] == "missing").then_some(Row::Node);
                (node.into_iter().collect(), agents.filter(|b| hit(b, &["name", "id"])).map(Row::Agent).collect())
            }
            Tab::Connectors => {
                let installed: Vec<&str> = self.plugins.0.iter().filter_map(|c| c["registryName"].as_str()).collect();
                let mut pinned: Vec<Row> =
                    self.plugins.0.iter().filter(|c| hit(c, &["name", "description"])).map(Row::Installed).collect();
                pinned.push(Row::Custom);
                let registry = m
                    .registry
                    .iter()
                    .filter(|c| !installed.contains(&c["name"].as_str().unwrap_or_default()))
                    .map(Row::Registry)
                    .collect();
                (pinned, registry)
            }
            Tab::Skills => {
                let installed: Vec<String> = self.plugins.1.iter().map(|v| s(v, "id").to_lowercase()).collect();
                let own =
                    self.plugins.1.iter().filter(|v| hit(v, &["name", "description"])).map(Row::OwnSkill).collect();
                let catalog = m
                    .skills
                    .iter()
                    .flatten()
                    .filter(|v| !installed.contains(&s(v, "source").to_lowercase()))
                    .filter(|v| hit(v, &["name", "description"]))
                    .map(Row::Skill)
                    .collect();
                (own, catalog)
            }
        }
    }

    /// The query no longer matches what the registry list shows: ↵ searches first.
    pub fn market_stale(m: &Market) -> bool {
        m.tab == Tab::Connectors && m.searched.as_deref().is_some_and(|s| s != m.query.text.trim())
    }

    pub(in crate::tui) fn market_key(&mut self, mut m: Box<Market>, k: KeyEvent) -> Option<Overlay> {
        let n = self.market_page(&m).rows.len();
        let alt = k.modifiers.contains(KeyModifiers::ALT);
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
            KeyCode::PageDown => self.turn_market_page(&mut m, true),
            KeyCode::PageUp => self.turn_market_page(&mut m, false),
            KeyCode::Char('s') if alt => {
                let i = PAGE_SIZES.iter().position(|s| *s == m.page_size).unwrap_or(0);
                m.page_size = PAGE_SIZES[(i + 1) % PAGE_SIZES.len()];
                m.pages = [0; 3];
                m.cursor = 0;
            }
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
                m.pages[Tab::Connectors.index()] = 0;
                self.sheet("marketConnectors", json!({"search": search}), Reply::Registry { more: false });
                m.searched = Some(search);
            }
            KeyCode::Enter => return self.market_enter(m),
            KeyCode::Char('x') if ctrl(k) => {
                let (id, name, skill) = match self.market_page(&m).rows.get(m.cursor) {
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
                    m.pages[m.tab.index()] = 0;
                }
            }
        }
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
        let act = match self.market_page(&m).rows.get(m.cursor) {
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
            Act::Install(item) => {
                let options = item["options"].as_array().cloned().unwrap_or_default();
                if options.len() == 1 && options[0]["inputs"].as_array().is_none_or(Vec::is_empty) {
                    self.flash(&tr("Adding {name}…").replace("{name}", &s(&item, "title")));
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
                                tr("Hosted by {name}").replace("{name}", &s(&item, "title"))
                            } else {
                                tr("On {name} ({kind})").replace("{name}", &host).replace("{kind}", &s(o, "kind"))
                            }
                        })
                        .collect();
                    let mut f = Fields {
                        title: tr("Add connector").into_owned(),
                        note: tr("Keys are saved on {name} only.").replace("{name}", &host),
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

const PAGE_SIZE: usize = 10;
const PAGE_SIZES: [usize; 4] = [6, 10, 20, 50];

impl MarketPage<'_> {
    /// The pager, in the same order as the other clients: count, size, previous, current / total, next.
    pub fn pager_line(&self, page_size: usize) -> String {
        format!(
            "{}   {} {}   {}   {} / {}   {}",
            tr("{count} total").replace("{count}", &self.count.to_string()),
            tr("Per page"),
            tr(&format!("{page_size} / page")),
            tr("Previous page"),
            self.page + 1,
            self.pages,
            tr("Next page"),
        )
    }
}

/// Catalog rows split into pages. While the registry still has a cursor, one more page is offered.
fn page_count(count: usize, size: usize, has_more: bool) -> usize {
    let loaded = if count == 0 { 1 } else { count.div_ceil(size) };
    if has_more { loaded + 1 } else { loaded }
}

fn clamp_page(page: usize, count: usize, size: usize, has_more: bool) -> usize {
    page.min(page_count(count, size, has_more).saturating_sub(1))
}

/// A later page that isn't full yet: the registry has to be asked. The first page is whatever the first answer held.
fn needs_fetch(count: usize, page: usize, size: usize) -> bool {
    let start = page * size;
    start > 0 && start + size > count
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
    use super::super::super::app::{App, Overlay};
    use super::super::super::net::Client;
    use super::super::{Market, Reply, Tab};
    use super::{clamp_page, needs_fetch, page_count};
    use serde_json::{Value, json};

    #[test]
    fn forty_seven_agents_are_five_pages() {
        assert_eq!(page_count(47, 10, false), 5);
        assert_eq!(page_count(19, 10, false), 2);
        assert_eq!(page_count(0, 10, false), 1);
        assert_eq!(page_count(72, 10, true), 9);
        assert_eq!(clamp_page(9, 47, 10, false), 4);
        assert_eq!(clamp_page(9, 72, 10, true), 8);
        assert!(!needs_fetch(60, 5, 10));
        assert!(needs_fetch(60, 6, 10));
        assert!(needs_fetch(72, 7, 10));
        assert!(!needs_fetch(80, 7, 10));
        assert!(!needs_fetch(0, 0, 10));
        assert!(!needs_fetch(3, 0, 10));
        assert!(needs_fetch(0, 1, 10));
    }

    fn registry(range: std::ops::Range<usize>, next: Option<&str>) -> Value {
        let items: Vec<Value> = range.map(|i| json!({"name": format!("c{i}"), "title": format!("C{i}")})).collect();
        json!({"items": items, "nextCursor": next})
    }

    fn market(app: &App) -> &Market {
        match app.overlays.last() {
            Some(Overlay::Market(m)) => m,
            _ => panic!("the marketplace is open"),
        }
    }

    fn turn_to(app: &mut App, page: usize) {
        let Some(Overlay::Market(mut m)) = app.overlays.pop() else { panic!("the marketplace is open") };
        m.tab = Tab::Connectors;
        m.pages[Tab::Connectors.index()] = page;
        app.continue_market_page(&mut m);
        app.overlays.push(Overlay::Market(m));
    }

    #[tokio::test]
    async fn a_connector_page_fills_from_the_registry_and_falls_back_when_it_cannot() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.open_market();
        app.on_sheet_reply(Reply::Registry { more: false }, Ok(registry(0..15, Some("a"))));
        turn_to(&mut app, 0);
        let page = app.market_page(market(&app));
        assert_eq!((page.count, page.pages), (15, 3));

        // Page 2 starts at row 10 but only 15 are loaded: the next registry page is asked for.
        turn_to(&mut app, 1);
        assert!(market(&app).loading);
        app.on_sheet_reply(Reply::Registry { more: true }, Ok(registry(15..18, Some("b"))));
        assert!(market(&app).loading, "18 rows still leave page 2 short");
        app.on_sheet_reply(Reply::Registry { more: true }, Ok(registry(18..25, Some("c"))));
        assert!(!market(&app).loading);
        assert_eq!(market(&app).pages[Tab::Connectors.index()], 1);

        // The page past the loaded rows fails: back to the last page with rows.
        turn_to(&mut app, 3);
        assert!(market(&app).loading);
        app.on_sheet_reply(Reply::Registry { more: true }, Err("offline".into()));
        let m = market(&app);
        assert!(!m.loading);
        assert_eq!(m.error.as_deref(), Some("offline"));
        assert_eq!(m.pages[Tab::Connectors.index()], 2);

        // The cursor ends: no page past the rows.
        app.on_sheet_reply(Reply::Registry { more: true }, Ok(registry(25..26, None)));
        assert_eq!(app.market_page(market(&app)).pages, 3);
    }
}
