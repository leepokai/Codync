//! Replies to the sheets' commands.

use serde_json::{Value, json};

use super::super::app::{After, AgentModel, App, Entry, Overlay, Toggle};
use super::{Market, MemoryMode, Reply, Shell, open_browser, s};

impl App {
    pub(in crate::tui) fn on_sheet_reply(&mut self, r: Reply, res: Result<Value, String>) {
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                self.sheet_error(&r, e);
                return;
            }
        };
        match r {
            Reply::Memory(bot) => {
                for o in &mut self.overlays {
                    if let Overlay::Memory(m) = o
                        && m.bot == bot
                    {
                        m.show(v.clone());
                        m.busy = false;
                        m.error = None;
                    }
                }
            }
            Reply::MemorySaved(bot) => {
                for o in &mut self.overlays {
                    if let Overlay::Memory(m) = o
                        && m.bot == bot
                    {
                        m.busy = false;
                    }
                }
                self.refresh_sheets();
            }
            Reply::MemoryFile(bot) => {
                for o in &mut self.overlays {
                    if let Overlay::Memory(m) = o
                        && m.bot == bot
                        && let MemoryMode::File { detail, .. } = &mut m.mode
                    {
                        *detail = Some(v.clone());
                        m.busy = false;
                    }
                }
            }
            Reply::SkillDetails(id) => {
                for o in &mut self.overlays {
                    if let Overlay::Skill(s) = o
                        && s.id == id
                    {
                        s.markdown = Some(v["markdown"].as_str().unwrap_or_default().to_owned());
                    }
                }
            }
            Reply::CallEntries(call) => {
                self.call_entries.insert(call, entries_of(&v));
            }
            Reply::BotChat { bot, peer } => {
                let entries = entries_of(&v);
                for o in &mut self.overlays {
                    if let Overlay::BotChat(c) = o
                        && c.bot == bot
                        && c.peer == peer
                    {
                        c.fetched = Some(entries.clone());
                    }
                }
            }
            Reply::Routines(bot) => {
                if v["routines"].is_array() {
                    for o in &mut self.overlays {
                        if let Overlay::Routines(l) = o
                            && l.bot == bot
                        {
                            let items = v["routines"].as_array().cloned().unwrap_or_default();
                            l.cursor = l.cursor.min(items.len().saturating_sub(1));
                            l.items = Some(items);
                            l.runs = v["runs"].as_array().cloned().unwrap_or_default();
                        }
                    }
                } else {
                    // A change: the list comes again.
                    self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.clone()));
                }
            }
            Reply::Library(bot) => self.library_loaded(&bot, v["items"].as_array().map_or(&[], Vec::as_slice)),
            Reply::Gear(backend) => {
                let lists = super::gear::GearLists {
                    skills: super::gear::items(&v["skills"]),
                    connectors: super::gear::items(&v["connectors"]),
                    rules: super::gear::items(&v["rules"]),
                    plugins: super::gear::items(&v["plugins"]),
                };
                for overlay in &mut self.overlays {
                    if let Overlay::Gear(gear) = overlay
                        && gear.backend == backend
                    {
                        gear.lists = Some(lists.clone());
                        gear.failed = false;
                    }
                }
            }
            Reply::RoutineSaved => {
                if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Routine(_))) {
                    self.overlays.truncate(i);
                }
                self.refresh_sheets();
            }
            Reply::Webhook => {
                let url = v["url"].as_str().or_else(|| v["localUrl"].as_str()).unwrap_or_default();
                let text = format!(
                    "curl -X POST '{url}' -H 'Authorization: Bearer {}' -H 'Content-Type: application/json' -d '{{}}'",
                    s(&v, "key")
                );
                super::super::app::copy(&text);
                self.flash(if v["url"].is_string() {
                    "Copied a curl command for the public URL (with its key)"
                } else {
                    "Copied a curl command (the cloud is off: this computer only)"
                });
            }
            Reply::Registry { search, cursor } => self.with_market(|app, m| {
                if !m.awaits(&search, cursor.as_deref()) {
                    return;
                }
                let items = v["items"].as_array().cloned().unwrap_or_default();
                if cursor.is_some() {
                    m.registry.extend(items);
                } else {
                    m.registry = items;
                }
                m.next = v["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_owned);
                m.searched.get_or_insert_with(String::new);
                m.loading = false;
                m.error = None;
                app.load_near_end(m);
            }),
            Reply::Skills => {
                if let Some(Overlay::Market(m)) =
                    self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Market(_)))
                {
                    m.skills = Some(v["items"].as_array().cloned().unwrap_or_default());
                }
            }
            Reply::Auth(backend) => {
                for o in &mut self.overlays {
                    if let Overlay::Agent(a) = o
                        && a.backend == backend
                    {
                        a.auth = Some(v.clone());
                        a.busy = false;
                        a.error = None;
                        a.cursor = 0;
                    }
                }
                self.call("refreshBackends", json!({}), After::Nothing);
            }
            Reply::Setup => {
                let Some(id) = v["term"].as_str() else { return };
                let backend = match self.overlays.last() {
                    Some(Overlay::Agent(a)) => a.backend.clone(),
                    _ => String::new(),
                };
                let input = self.client.spawn_term(id.to_owned(), self.tx.clone());
                self.shell = Some(Shell { id: id.to_owned(), input, backend, exited: None, out: vec![] });
            }
            Reply::FieldsSaved => {
                if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Fields(_))) {
                    self.overlays.truncate(i);
                }
                if let Some(Overlay::Agent(a)) = self.overlays.last_mut()
                    && v.get("signedIn").is_some()
                {
                    a.auth = Some(v);
                    a.busy = false;
                }
                self.flash("Saved");
                self.refresh_sheets();
            }
            Reply::SignIn => {
                let url = s(&v, "url");
                super::super::app::copy(&url);
                if open_browser(&url, &self.url) {
                    self.flash("Finish in the browser, then ^r refreshes");
                } else {
                    self.flash("Sign-in link copied: open it on this computer, then ^r refreshes");
                }
            }
            Reply::Models(backend) => {
                let mut list: Vec<_> =
                    v["models"].as_array().into_iter().flatten().filter_map(AgentModel::parse).collect();
                if v["defaultModelOptions"] == true
                    && let Some(default) = v["models"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|m| m["id"] == v["currentModelId"])
                        .and_then(AgentModel::parse)
                {
                    list.insert(
                        0,
                        AgentModel {
                            id: String::new(),
                            name: "Default".into(),
                            options: default.options,
                            options_error: default.options_error,
                        },
                    );
                }
                if v["fastMode"] == true {
                    self.fast_mode.insert(backend.clone());
                }
                self.models.insert(backend, Some(list));
            }
            Reply::Downloaded => {
                let paths: Vec<String> =
                    v["paths"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_owned)).collect();
                let names: Vec<&str> =
                    paths.iter().filter_map(|p| std::path::Path::new(p).file_name()?.to_str()).collect();
                let dir = paths.first().and_then(|p| std::path::Path::new(p).parent()).map(|d| d.to_string_lossy());
                let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = dir.map(|d| super::super::app::tilde(&d, &home)).unwrap_or_default();
                self.flash(&format!("Saved {} to {dir}", names.join(", ")));
            }
            Reply::Changed => self.refresh_sheets(),
        }
    }

    fn sheet_error(&mut self, r: &Reply, e: String) {
        match r {
            Reply::Models(backend) => {
                self.models.insert(backend.clone(), Some(vec![]));
            }
            Reply::RoutineSaved => {
                if let Some(Overlay::Routine(f)) = self.overlays.last_mut() {
                    f.saving = false;
                    f.error = Some(e);
                }
            }
            Reply::FieldsSaved => match self.overlays.last_mut() {
                Some(Overlay::Fields(f)) => {
                    f.saving = false;
                    f.error = Some(e);
                }
                _ => self.flash(&e),
            },
            Reply::Auth(backend) => {
                for o in &mut self.overlays {
                    if let Overlay::Agent(a) = o
                        && a.backend == *backend
                    {
                        a.busy = false;
                        a.error = Some(e.clone());
                    }
                }
            }
            Reply::Registry { search, cursor } => self.with_market(|_, m| {
                if m.awaits(search, cursor.as_deref()) {
                    m.loading = false;
                    m.error = Some(e);
                }
            }),
            Reply::Skills => self.with_market(|_, m| {
                m.skills.get_or_insert_with(Vec::new);
                m.error = Some(e);
            }),
            Reply::CallEntries(call) => {
                // The call's line folds again with nothing kept, so the next `o` asks again.
                let lines: Vec<String> = self
                    .entries
                    .values()
                    .flat_map(|m| m.values())
                    .filter(|l| l.unfolds() && l.call() == Some(call.as_str()))
                    .map(|l| l.id.clone())
                    .collect();
                self.unfolded.retain(|id| !lines.contains(id));
                self.flash(&e);
            }
            Reply::BotChat { bot, peer } => {
                for o in &mut self.overlays {
                    if let Overlay::BotChat(c) = o
                        && c.bot == *bot
                        && c.peer == *peer
                    {
                        c.fetched.get_or_insert_with(Vec::new);
                    }
                }
                self.flash(&e);
            }
            Reply::SkillDetails(id) => {
                for o in &mut self.overlays {
                    if let Overlay::Skill(s) = o
                        && s.id == *id
                    {
                        s.error = Some(e.clone());
                    }
                }
            }
            Reply::Library(bot) => {
                for o in &mut self.overlays {
                    if let Overlay::Library(l) = o
                        && l.bot == *bot
                    {
                        l.items.get_or_insert_with(Vec::new);
                    }
                }
                self.flash(&e);
            }
            Reply::Gear(backend) => {
                for o in &mut self.overlays {
                    if let Overlay::Gear(gear) = o
                        && gear.backend == *backend
                    {
                        gear.lists.get_or_insert_with(|| super::gear::GearLists {
                            skills: Vec::new(),
                            connectors: Vec::new(),
                            rules: Vec::new(),
                            plugins: Vec::new(),
                        });
                        gear.failed = true;
                    }
                }
                self.flash(&e);
            }
            Reply::Memory(_) | Reply::MemorySaved(_) | Reply::MemoryFile(_) | Reply::Routines(_) => {
                for o in &mut self.overlays {
                    match o {
                        Overlay::Memory(m) => {
                            m.busy = false;
                            m.error = Some(e.clone());
                        }
                        Overlay::Routines(l) => {
                            l.items.get_or_insert_with(Vec::new);
                        }
                        _ => {}
                    }
                }
                self.flash(&e);
            }
            _ => self.flash(&e),
        }
    }

    /// Runs `f` on the open marketplace, with the rest of the app readable beside it.
    fn with_market(&mut self, f: impl FnOnce(&Self, &mut Market)) {
        let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Market(_))) else { return };
        let overlay = match self.overlays.remove(i) {
            Overlay::Market(mut m) => {
                f(self, &mut m);
                Overlay::Market(m)
            }
            other => other,
        };
        self.overlays.insert(i, overlay);
    }

    /// Loads what every open sheet shows again, after a change.
    pub(in crate::tui) fn refresh_sheets(&mut self) {
        let mut calls: Vec<(&'static str, Value, After)> = vec![];
        let mut gear_ids = Vec::new();
        for o in &self.overlays {
            match o {
                Overlay::Memory(m) => {
                    calls.push(("memory", m.body(), After::Sheet(Reply::Memory(m.bot.clone()))));
                    if let Some(body) = m.file_body() {
                        calls.push(("memoryFile", body, After::Sheet(Reply::MemoryFile(m.bot.clone()))));
                    }
                }
                Overlay::Routines(l) => {
                    calls.push(("routines", json!({"botId": l.bot}), After::Sheet(Reply::Routines(l.bot.clone()))));
                }
                Overlay::Library(l) => {
                    calls.push(("library", json!({"botId": l.bot}), After::Sheet(Reply::Library(l.bot.clone()))));
                }
                Overlay::Gear(gear) => gear_ids.push(gear.backend.clone()),
                Overlay::Market(_) | Overlay::Form(_) => {
                    calls.push(("connectors", json!({}), After::Connectors));
                    calls.push(("skills", json!({}), After::Skills));
                }
                _ => {}
            }
        }
        for backend in gear_ids {
            calls.push(("agentGear", json!({"backend": backend}), After::Sheet(Reply::Gear(backend))));
        }
        calls.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        for (m, body, after) in calls {
            self.call(m, body, after);
        }
    }

    /// Keeps the installed lists the marketplace and the bot form show.
    pub(in crate::tui) fn set_plugins(&mut self, connectors: bool, items: Vec<Value>) -> Vec<Toggle> {
        let list = items
            .iter()
            .filter_map(|c| {
                let id = c["id"].as_str()?.to_owned();
                let name = c["name"].as_str().unwrap_or(&id).to_owned();
                Some(Toggle { id, name, on: false })
            })
            .collect();
        if connectors {
            self.plugins.0 = items;
        } else {
            self.plugins.1 = items;
        }
        list
    }
}

/// The entries in a host reply (`callEntries`, `botConversation`).
fn entries_of(v: &Value) -> Vec<Entry> {
    v["entries"].as_array().into_iter().flatten().filter_map(Entry::parse).map(|(_, e)| e).collect()
}
