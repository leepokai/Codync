//! The new/edit group chat form.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

use super::{After, App, Bot, Editor, GroupField, GroupForm, Overlay};

impl App {
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
        let about = g.group_id.is_some();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => self.save_group(&mut g),
            KeyCode::Char('s') if ctrl => self.save_group(&mut g),
            KeyCode::Tab => g.field = g.field.next(about),
            KeyCode::BackTab => g.field = g.field.prev(about),
            KeyCode::Down if g.field != GroupField::Bots => g.field = g.field.next(about),
            KeyCode::Down => g.cursor = (g.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up if g.field == GroupField::About => g.field = GroupField::Name,
            KeyCode::Up if g.field == GroupField::Bots && g.cursor == 0 => g.field = g.field.prev(about),
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
}

#[cfg(test)]
mod tests {
    use super::super::test_app;
    use super::*;

    #[test]
    fn a_new_group_has_no_about_and_an_edited_one_keeps_it() {
        let mut app = test_app();
        app.bots.insert("a".into(), Bot::parse(&json!({"id": "a", "name": "Ada"})).expect("a bot"));
        app.new_group();
        let Some(Overlay::Group(mut g)) = app.overlays.pop() else { panic!("the group form opens") };
        let mut fields = vec![];
        for code in [KeyCode::Tab, KeyCode::Tab, KeyCode::BackTab, KeyCode::Down, KeyCode::Up] {
            let Some(Overlay::Group(next)) = app.group_key(g, KeyEvent::from(code)) else { panic!("stays open") };
            g = next;
            fields.push(g.field);
        }
        let (n, b) = (GroupField::Name, GroupField::Bots);
        assert_eq!(fields, [n, b, n, b, n], "creating skips About");
        let Some(Overlay::Group(g)) = app.group_key(g, KeyEvent::from(KeyCode::Char('x'))) else { panic!("stays") };
        assert_eq!(g.name.text, "x");
        let group = json!({"id": "g", "kind": "group", "name": "Team", "members": ["a"]});
        app.bots.insert("g".into(), Bot::parse(&group).expect("a group"));
        app.selected = Some("g".into());
        app.edit_bot();
        let Some(Overlay::Group(mut g)) = app.overlays.pop() else { panic!("the group editor opens") };
        let mut fields = vec![];
        for _ in 0..3 {
            let Some(Overlay::Group(next)) = app.group_key(g, KeyEvent::from(KeyCode::Tab)) else { panic!("stays") };
            g = next;
            fields.push(g.field);
        }
        assert_eq!(fields, [GroupField::About, b, n]);
    }
}
