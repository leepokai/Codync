//! The bot settings form.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    After, App, COLORS, Editor, FIELDS, Field, Form, GroupField, GroupForm, Overlay, PairCode, SHAPES, Toggle, cycle,
    merge_toggles,
};
use crate::tui::manage::Page;

impl App {
    pub(super) fn open_pair(&mut self) {
        self.overlays.push(Overlay::Pair(PairCode::Loading));
        self.call("pairing", json!({}), After::Pairing);
    }

    pub(in crate::tui) fn edit_bot(&mut self) {
        if let Some(id) = self.selected.clone() {
            self.edit_bot_of(&id);
        }
    }

    /// Opens bot `id`'s settings (a group's: its name, about and members).
    pub(in crate::tui) fn edit_bot_of(&mut self, id: &str) {
        let Some(b) = self.bots.get(id).cloned() else { return };
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
            bot_id: b.id.clone(),
            name: Editor::with(&b.name),
            instructions: Editor::with(&b.description),
            backend: b.backend.clone(),
            model: Editor::with(b.model.as_deref().unwrap_or_default()),
            model_options: (b.model.clone().unwrap_or_default(), b.model_options.clone()),
            fast: b.fast_mode,
            fallbacks: b.fallbacks.clone(),
            self_fallbacks: b.self_fallbacks,
            cwd: if b.managed_workspace { String::new() } else { b.cwd.clone() },
            auto: b.auto,
            mode: b.mode.clone(),
            notify: b.notify,
            computer: b.computer,
            manage_team: b.manage_team,
            offered_to_network: b.offered_to_network,
            reachable_by_assistants: b.reachable_by_assistants,
            color: COLORS.iter().position(|c| *c == b.color).unwrap_or(7),
            shape: SHAPES.iter().position(|s| *s == b.shape).unwrap_or(0),
            connectors: b.connectors.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            skills: b.skills.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            field: 0,
            sub: 0,
            error: None,
            saving: false,
            opened: Value::Null,
        };
        merge_toggles(&mut f.connectors, &self.market.0);
        merge_toggles(&mut f.skills, &self.market.1);
        f.opened = f.body();
        self.want_models(&b.backend);
        self.overlays.push(Overlay::Form(Box::new(f)));
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
    }

    pub(super) fn form_key(&mut self, mut f: Box<Form>, k: KeyEvent) -> Option<Overlay> {
        if let Some(page) = Page::Settings.step(k) {
            if f.dirty() {
                f.error = Some("Save with ^S first, or Esc to drop the changes.".into());
                return Some(Overlay::Form(f));
            }
            self.open_page(page);
            return None;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('o') && !f.saving && self.custom_icons {
            let target = format!("bot:{}", f.bot_id);
            self.overlays.push(Overlay::Form(f));
            self.overlays.push(Overlay::Fields(Box::new(crate::tui::manage::icon_fields(target))));
            return None;
        }
        let field = f.current();
        if field == Field::Fallbacks && self.fallback_key(&mut f, k) {
            return Some(Overlay::Form(f));
        }
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
                self.open_agent(&backend, false);
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
                        | Field::Fast
                        | Field::SelfFallbacks
                        | Field::Approvals
                        | Field::Notify
                        | Field::Computer
                        | Field::Team
                        | Field::Network
                        | Field::Assistants
                        | Field::Color
                        | Field::Shape
                        | Field::ModelOptions
                        | Field::Connectors
                        | Field::Skills
                ) =>
            {
                let d: isize = if k.code == KeyCode::Left { -1 } else { 1 };
                let space = k.code == KeyCode::Char(' ');
                match field {
                    Field::Agent if !space => {
                        let ids = self.agent_choices();
                        if !ids.is_empty() {
                            let i = ids.iter().position(|x| *x == f.backend).unwrap_or(0);
                            f.backend.clone_from(&ids[cycle(i, d, ids.len())]);
                            // Another agent's model ids mean nothing here, and it can't be its own fallback.
                            f.model.clear();
                            f.model_options = Default::default();
                            let own = f.backend.clone();
                            f.fallbacks.retain(|x| x.backend != own);
                            // Nor its mode ids: a picked mode becomes the new agent's Auto mode (or, without one, asking).
                            f.mode = f.mode.take().map(|_| crate::agent::modes::AUTO.to_owned());
                            self.want_models(&f.backend);
                        }
                    }
                    Field::Fast => f.fast = !f.fast,
                    Field::SelfFallbacks => f.self_fallbacks = !f.self_fallbacks,
                    Field::Approvals => self.cycle_approval(&mut f, d),
                    Field::Notify => f.notify = !f.notify,
                    Field::Computer => f.computer = !f.computer,
                    Field::Team => f.manage_team = f.manage_team.map(|on| !on),
                    Field::Network => f.offered_to_network = f.offered_to_network.map(|on| !on),
                    Field::Assistants => f.reachable_by_assistants = f.reachable_by_assistants.map(|on| !on),
                    Field::Color if !space => f.color = cycle(f.color, d, COLORS.len()),
                    Field::Shape if !space => f.shape = cycle(f.shape, d, SHAPES.len()),
                    Field::ModelOptions => {
                        let model = f.model.text.trim().to_owned();
                        let offered = self.model_settings(&f.backend, &model);
                        if !space {
                            f.sub = cycle(f.sub, d, offered.len().max(1));
                        } else if let Some(o) = offered.get(f.sub) {
                            if f.model_options.0 != model {
                                f.model_options = (model, BTreeMap::new());
                            }
                            // Default, then each value.
                            let ids: Vec<&str> =
                                std::iter::once("").chain(o.values.iter().map(|(id, _)| id.as_str())).collect();
                            let now = f.model_options.1.get(&o.id).map_or("", String::as_str);
                            let next = ids[cycle(ids.iter().position(|i| *i == now).unwrap_or(0), 1, ids.len())];
                            if next.is_empty() {
                                f.model_options.1.remove(&o.id);
                            } else {
                                f.model_options.1.insert(o.id.clone(), next.to_owned());
                            }
                        }
                    }
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
        if f.name.text.trim().is_empty() {
            f.error = Some("Give the bot a name.".into());
            return;
        }
        let mut body = f.changes();
        body["id"] = f.bot_id.clone().into();
        f.saving = true;
        f.error = None;
        self.call("updateBot", body, After::Saved);
    }

    /// Agents a bot can switch to: runnable ones on this computer first.
    pub(super) fn agent_choices(&self) -> Vec<String> {
        let mut v: Vec<&Value> = self
            .backends
            .iter()
            .filter(|b| b["available"].as_bool().unwrap_or(false) || b["registry"].is_string())
            .collect();
        v.sort_by_key(|b| !b["installed"].as_bool().unwrap_or(false));
        v.iter().filter_map(|b| b["id"].as_str().map(str::to_owned)).collect()
    }
}

impl Form {
    /// The bot as this form would save it.
    fn body(&self) -> Value {
        let model = self.model.text.trim();
        json!({
            "name": self.name.text.trim(),
            "description": self.instructions.text.trim(),
            "backend": self.backend,
            "cwd": self.cwd,
            "permission": if self.auto { "auto" } else { "ask" },
            "mode": self.mode,
            "model": if model.is_empty() { Value::Null } else { model.into() },
            "modelOptions": self.model_options().cloned().unwrap_or_default(),
            "fastMode": self.fast,
            "fallbacks": self.fallbacks,
            "selfFallbacks": self.self_fallbacks,
            "notify": self.notify,
            "computer": self.computer,
            "manageTeam": self.manage_team.unwrap_or_default(),
            "offeredToNetwork": self.offered_to_network.unwrap_or(true),
            "reachableByAssistants": self.reachable_by_assistants.unwrap_or(true),
            "avatarColor": COLORS[self.color],
            "avatarShape": SHAPES[self.shape],
            "connectors": self.connectors.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
            "skills": self.skills.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
        })
    }

    /// Whether anything changed since the form opened.
    pub fn dirty(&self) -> bool {
        self.body() != self.opened
    }

    /// What changed since the form opened, as `updateBot` takes it: a change made elsewhere meanwhile (a bot that
    /// manages the team, another device) stays.
    pub(super) fn changes(&self) -> Value {
        let mut body = self.body();
        if let (Some(now), Some(opened)) = (body.as_object_mut(), self.opened.as_object()) {
            now.retain(|key, value| opened.get(key) != Some(value));
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AgentModel, Bot, test_app};
    use super::*;

    #[tokio::test]
    async fn changing_agent_clears_default_model_effort() {
        let mut app = test_app();
        app.backends = vec![json!({"id": "claude", "available": true}), json!({"id": "codex", "available": true})];
        let bot = json!({"id": "b", "backend": "claude", "modelOptions": {"effort": "high"}});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("editor opens") };
        f.field = FIELDS.iter().position(|x| *x == Field::Agent).expect("agent field");
        let Some(Overlay::Form(f)) = app.form_key(f, KeyEvent::from(KeyCode::Right)) else { panic!("editor stays") };
        assert_ne!(f.backend, "claude");
        assert!(f.model_options.1.is_empty());
    }

    #[tokio::test]
    async fn model_options_cycle_and_belong_to_their_model() {
        let mut app = test_app();
        let grok = json!({"id": "grok", "name": "Grok", "options": [
            {"id": "fast", "name": "Fast", "values": [{"id": "false", "name": "Off"}, {"id": "true", "name": "On"}]}]});
        app.models.insert("cursor".into(), Some(AgentModel::parse(&grok).into_iter().collect()));
        let bot = json!({"id": "b", "backend": "cursor", "model": "grok", "modelOptions": {"fast": "false"}});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        f.field = FIELDS.iter().position(|x| *x == Field::ModelOptions).expect("an options field");
        let space = KeyEvent::from(KeyCode::Char(' '));
        let Some(Overlay::Form(f)) = app.form_key(f, space) else { panic!("the editor stays") };
        assert_eq!(f.model_options().and_then(|o| o.get("fast")).map(String::as_str), Some("true"));
        let Some(Overlay::Form(mut f)) = app.form_key(f, space) else { panic!("the editor stays") };
        assert!(f.model_options().is_some_and(BTreeMap::is_empty), "back to the agent's default");
        f.model = Editor::with("other");
        assert!(f.model_options().is_none(), "another model starts without settings");
    }

    #[tokio::test]
    async fn a_save_sends_only_what_the_form_changed() {
        let mut app = test_app();
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "model": "sonnet"});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        assert_eq!(f.changes(), json!({}));
        f.notify = false;
        assert_eq!(f.changes(), json!({"notify": false}), "a model changed elsewhere meanwhile stays");
    }

    #[tokio::test]
    async fn the_network_switches_need_a_host_that_has_them() {
        let mut app = test_app();
        let old = json!({"id": "a", "name": "Old", "backend": "claude"});
        let new = json!({"id": "b", "name": "New", "backend": "claude", "offeredToNetwork": true, "reachableByAssistants": true});
        for bot in [&old, &new] {
            app.bots.insert(bot["id"].as_str().unwrap().into(), Bot::parse(bot).expect("a bot"));
        }
        for (id, offered) in [("a", None), ("b", Some(false))] {
            app.selected = Some(id.into());
            app.edit_bot();
            let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
            f.field = FIELDS.iter().position(|x| *x == Field::Network).unwrap();
            let keep = app.form_key(f, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
            let Some(Overlay::Form(f)) = keep else { panic!("the form stays") };
            assert_eq!(f.offered_to_network, offered);
            let changed = f.changes();
            assert_eq!(changed.get("offeredToNetwork").cloned(), offered.map(Value::from), "{changed}");
        }
    }
}
