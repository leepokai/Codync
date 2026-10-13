//! The new/edit bot and group chat forms.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::{
    After, AgentPicker, App, Bot, COLORS, Editor, FIELDS, Field, Form, GroupField, GroupForm, Overlay, SHAPES, Toggle,
    cycle, merge_toggles,
};

impl App {
    pub(super) fn open_pair(&mut self) {
        self.overlays.push(Overlay::Pair(None));
        self.call("pairing", json!({}), After::Pairing);
    }

    pub(super) fn new_bot(&mut self) {
        if self.backends.is_empty() {
            self.call("hello", json!({}), After::Hello);
        }
        self.overlays.push(Overlay::Agents(AgentPicker { query: Editor::default(), cursor: 0 }));
    }

    pub(super) fn new_group(&mut self) {
        self.overlays.push(Overlay::Group(GroupForm {
            group_id: None,
            name: Editor::default(),
            about: Editor::default(),
            members: vec![],
            field: GroupField::Bots,
            cursor: 0,
            error: None,
            saving: false,
        }));
    }

    /// Bots a group can hold: the roster's agents, plus hidden ones already in it.
    pub fn group_candidates(&self, members: &[String]) -> Vec<&Bot> {
        let mut v: Vec<&Bot> = self.roster().into_iter().filter(|b| !b.group).collect();
        v.extend(self.bots.values().filter(|b| b.hidden && !b.group && members.contains(&b.id)));
        v
    }

    /// "Alice, Bob" when no name is typed.
    pub fn group_default_name(&self, members: &[String]) -> String {
        let names: Vec<&str> = members.iter().filter_map(|m| self.bots.get(m)).map(|b| b.name.as_str()).collect();
        names.join(", ")
    }

    pub(super) fn group_key(&mut self, mut g: GroupForm, k: KeyEvent) -> Option<Overlay> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.group_candidates(&g.members).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => self.save_group(&mut g),
            KeyCode::Char('s') if ctrl => self.save_group(&mut g),
            KeyCode::Tab => g.field = g.field.next(),
            KeyCode::BackTab => g.field = g.field.prev(),
            KeyCode::Down if g.field != GroupField::Bots => g.field = g.field.next(),
            KeyCode::Down => g.cursor = (g.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up if g.field == GroupField::About => g.field = GroupField::Name,
            KeyCode::Up if g.field == GroupField::Bots && g.cursor == 0 => g.field = GroupField::About,
            KeyCode::Up if g.field == GroupField::Bots => g.cursor -= 1,
            KeyCode::Char(' ') if g.field == GroupField::Bots => {
                let id = self.group_candidates(&g.members).get(g.cursor).map(|b| b.id.clone());
                if let Some(id) = id {
                    g.error = None;
                    if let Some(i) = g.members.iter().position(|m| *m == id) {
                        g.members.remove(i);
                    } else {
                        g.members.push(id);
                    }
                }
            }
            _ if g.field == GroupField::Name => {
                g.name.key(k);
            }
            _ if g.field == GroupField::About => {
                g.about.key(k);
            }
            _ => {}
        }
        Some(Overlay::Group(g))
    }

    fn save_group(&mut self, g: &mut GroupForm) {
        if g.saving {
            return;
        }
        if g.members.is_empty() {
            g.error = Some("Pick at least one bot.".into());
            return;
        }
        let typed = g.name.text.trim();
        let name = if typed.is_empty() { self.group_default_name(&g.members) } else { typed.to_owned() };
        let mut body = json!({"name": name, "members": g.members, "description": g.about.text.trim()});
        g.saving = true;
        g.error = None;
        if let Some(id) = &g.group_id {
            body["id"] = id.clone().into();
            self.call("updateBot", body, After::Saved);
        } else {
            body["id"] = "".into();
            body["kind"] = "group".into();
            self.call("createBot", body, After::Saved);
        }
    }

    pub(super) fn edit_bot(&mut self) {
        let Some(b) = self.bot().cloned() else { return };
        if b.group {
            self.overlays.push(Overlay::Group(GroupForm {
                group_id: Some(b.id.clone()),
                name: Editor::with(&b.name),
                about: Editor::with(&b.description),
                members: b.members.clone(),
                field: GroupField::Name,
                cursor: 0,
                error: None,
                saving: false,
            }));
            return;
        }
        let mut f = Form {
            bot_id: Some(b.id.clone()),
            name: Editor::with(&b.name),
            instructions: Editor::with(&b.description),
            backend: b.backend.clone(),
            model: Editor::with(b.model.as_deref().unwrap_or_default()),
            cwd: if b.managed_workspace { String::new() } else { b.cwd.clone() },
            auto: b.auto,
            notify: b.notify,
            computer: b.computer,
            color: COLORS.iter().position(|c| *c == b.color).unwrap_or(7),
            shape: SHAPES.iter().position(|s| *s == b.shape).unwrap_or(0),
            connectors: b.connectors.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            skills: b.skills.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            field: 0,
            sub: 0,
            error: None,
            saving: false,
        };
        merge_toggles(&mut f.connectors, &self.market.0, false);
        merge_toggles(&mut f.skills, &self.market.1, false);
        self.want_models(&b.backend);
        self.overlays.push(Overlay::Form(Box::new(f)));
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
    }

    /// Step 2 of a new bot: name and looks, prefilled, in a personal workspace.
    pub(super) fn new_bot_form(&mut self, backend: String) {
        if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Agents(_))) {
            self.overlays.truncate(i);
        }
        let n = self.bots.len() + usize::try_from(crate::store::now_ms() % 97).unwrap_or(0);
        let name = NAMES[n % NAMES.len()].to_owned();
        let f = Form {
            bot_id: None,
            name: Editor::with(&name),
            instructions: Editor::default(),
            backend,
            model: Editor::default(),
            cwd: String::new(),
            auto: true,
            notify: true,
            computer: false,
            color: (n * 7) % COLORS.len(),
            shape: n % SHAPES.len(),
            connectors: self
                .market
                .0
                .iter()
                .map(|t| Toggle { id: t.id.clone(), name: t.name.clone(), on: true })
                .collect(),
            skills: self.market.1.iter().map(|t| Toggle { id: t.id.clone(), name: t.name.clone(), on: true }).collect(),
            field: 0,
            sub: 0,
            error: None,
            saving: false,
        };
        self.want_models(&f.backend);
        self.overlays.push(Overlay::Form(Box::new(f)));
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
    }

    pub(super) fn form_key(&mut self, mut f: Box<Form>, k: KeyEvent) -> Option<Overlay> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let field = f.current();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Char('s') if ctrl => {
                self.save_form(&mut f);
            }
            KeyCode::Tab | KeyCode::Down => {
                f.field = (f.field + 1) % FIELDS.len();
                f.sub = 0;
            }
            KeyCode::BackTab | KeyCode::Up => {
                f.field = (f.field + FIELDS.len() - 1) % FIELDS.len();
                f.sub = 0;
            }
            KeyCode::Enter
                if field == Field::Instructions && k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) =>
            {
                f.instructions.insert("\n");
            }
            KeyCode::Char('j') if ctrl && field == Field::Instructions => f.instructions.insert("\n"),
            KeyCode::Backspace | KeyCode::Delete if field == Field::Folder => f.cwd.clear(),
            KeyCode::Enter if field == Field::Folder => {
                let start = f.cwd.clone();
                self.overlays.push(Overlay::Form(f));
                self.open_folder(&start);
                return None;
            }
            KeyCode::Enter if field == Field::Agent => {
                let backend = f.backend.clone();
                self.overlays.push(Overlay::Form(f));
                self.open_agent(&backend);
                return None;
            }
            KeyCode::Char('i') if field == Field::Skills => {
                let id = f.skills.get(f.sub).map(|t| t.id.clone());
                let skill = self.plugins.1.iter().find(|v| v["id"].as_str() == id.as_deref()).cloned();
                self.overlays.push(Overlay::Form(f));
                if let Some(v) = skill {
                    self.open_skill(&v, false);
                }
                return None;
            }
            KeyCode::Left | KeyCode::Right if field == Field::Model => {
                let d: isize = if k.code == KeyCode::Left { -1 } else { 1 };
                self.want_models(&f.backend);
                if let Some(m) = self.cycle_model(&f.backend, f.model.text.trim(), d) {
                    f.model = Editor::with(&m);
                }
            }
            KeyCode::Enter => self.save_form(&mut f),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if matches!(
                    field,
                    Field::Agent
                        | Field::Approvals
                        | Field::Notify
                        | Field::Computer
                        | Field::Color
                        | Field::Shape
                        | Field::Connectors
                        | Field::Skills
                ) =>
            {
                let d: isize = if k.code == KeyCode::Left { -1 } else { 1 };
                let space = k.code == KeyCode::Char(' ');
                match field {
                    Field::Agent if !space => {
                        let ids: Vec<String> =
                            self.agent_choices("").iter().filter_map(|b| b["id"].as_str().map(str::to_owned)).collect();
                        if !ids.is_empty() {
                            let i = ids.iter().position(|x| *x == f.backend).unwrap_or(0);
                            f.backend.clone_from(&ids[cycle(i, d, ids.len())]);
                            // Another agent's model ids mean nothing here.
                            f.model.clear();
                            self.want_models(&f.backend);
                        }
                    }
                    Field::Approvals => f.auto = !f.auto,
                    Field::Notify => f.notify = !f.notify,
                    Field::Computer => f.computer = !f.computer,
                    Field::Color if !space => f.color = cycle(f.color, d, COLORS.len()),
                    Field::Shape if !space => f.shape = cycle(f.shape, d, SHAPES.len()),
                    Field::Connectors | Field::Skills => {
                        let list = if field == Field::Connectors { &mut f.connectors } else { &mut f.skills };
                        if space {
                            if let Some(t) = list.get_mut(f.sub) {
                                t.on = !t.on;
                            }
                        } else if !list.is_empty() {
                            f.sub = cycle(f.sub, d, list.len());
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                let ed = match field {
                    Field::Name => Some(&mut f.name),
                    Field::Instructions => Some(&mut f.instructions),
                    Field::Model => Some(&mut f.model),
                    _ => None,
                };
                if let Some(ed) = ed {
                    ed.key(k);
                }
            }
        }
        Some(Overlay::Form(f))
    }

    fn save_form(&mut self, f: &mut Form) {
        if f.saving {
            return;
        }
        if f.bot_id.is_some() && f.name.text.trim().is_empty() {
            f.error = Some("Give the bot a name.".into());
            return;
        }
        let model = f.model.text.trim();
        let mut body = json!({
            "name": f.name.text.trim(),
            "description": f.instructions.text.trim(),
            "backend": f.backend,
            "cwd": f.cwd,
            "permission": if f.auto { "auto" } else { "ask" },
            "model": if model.is_empty() { Value::Null } else { model.into() },
            "notify": f.notify,
            "computer": f.computer,
            "avatarColor": COLORS[f.color],
            "avatarShape": SHAPES[f.shape],
            "connectors": f.connectors.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
            "skills": f.skills.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
        });
        f.saving = true;
        f.error = None;
        match &f.bot_id {
            Some(id) => {
                body["id"] = id.clone().into();
                self.call("updateBot", body, After::Saved);
            }
            None => self.call("createBot", body, After::Saved),
        }
    }
}

const NAMES: [&str; 24] = [
    "Ada", "Rex", "Mia", "Sol", "Kit", "Oli", "Ivy", "Max", "Zoe", "Leo", "Ari", "Bea", "Cal", "Dot", "Eli", "Fay",
    "Gus", "Hal", "Ida", "Jin", "Kai", "Lux", "Nia", "Otto",
];
