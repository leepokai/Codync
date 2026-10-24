//! The bot form's Fallbacks field: the agents that carry on, in order, when the bot's own can't answer
//! (`agent::fallback`). ←/→ picks a fallback or the "+ add" slot after them; space adds the next free agent on the
//! slot and cycles a fallback's model; `a` gives a fallback the next free agent; `[`/`]` move it; `x` removes it. `A`
//! on a failure notice that offers a fallback opens the field.

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Bot, Entry, FIELDS, Field, Form, Overlay, cycle};
use crate::agent::fallback::MAX_FALLBACKS;
use crate::store::Fallback;

impl App {
    /// The agent bot a failure notice in chat `chat` offers a fallback for: in a group, the member whose agent failed.
    pub(in crate::tui) fn fallback_owner(&self, chat: &str, e: &Entry) -> Option<&Bot> {
        let id = e.data["author"].as_str().unwrap_or(chat);
        self.bots.get(id).filter(|b| e.adds_fallback() && !b.group)
    }

    /// `A` on such a notice: that bot's settings, on its Fallbacks field.
    pub(super) fn add_fallback(&mut self, chat: &str, e: &Entry) {
        let Some(id) = self.fallback_owner(chat, e).map(|b| b.id.clone()) else { return };
        self.edit_bot_of(&id);
        if let Some(Overlay::Form(f)) = self.overlays.last_mut() {
            f.field = FIELDS.iter().position(|x| *x == Field::Fallbacks).unwrap_or(0);
        }
    }

    /// A key on the Fallbacks field; whether it was one of the field's.
    pub(super) fn fallback_key(&mut self, f: &mut Form, k: KeyEvent) -> bool {
        let slots = f.fallbacks.len() + usize::from(f.fallbacks.len() < MAX_FALLBACKS);
        let at = f.sub.min(slots.saturating_sub(1));
        match k.code {
            KeyCode::Left | KeyCode::Right => {
                f.sub = cycle(at, if k.code == KeyCode::Left { -1 } else { 1 }, slots.max(1));
            }
            KeyCode::Char(' ') if at == f.fallbacks.len() => {
                if let Some(agent) = self.free_agents(f).into_iter().next() {
                    self.want_models(&agent);
                    f.fallbacks.push(Fallback { backend: agent, ..Fallback::default() });
                }
            }
            KeyCode::Char(' ') => {
                let step = &mut f.fallbacks[at];
                self.want_models(&step.backend);
                if let Some(model) = self.cycle_model(&step.backend, step.model.as_deref().unwrap_or_default(), 1) {
                    step.model = (!model.is_empty()).then_some(model);
                    step.model_options.clear();
                }
            }
            KeyCode::Char('a') if at < f.fallbacks.len() => {
                let free = self.free_agents(f);
                let agents: Vec<&String> = std::iter::once(&f.fallbacks[at].backend).chain(&free).collect();
                if agents.len() > 1 {
                    let next = agents[1].clone();
                    self.want_models(&next);
                    f.fallbacks[at] = Fallback { backend: next, ..Fallback::default() };
                }
            }
            KeyCode::Char('[') if at > 0 && at < f.fallbacks.len() => {
                f.fallbacks.swap(at, at - 1);
                f.sub = at - 1;
            }
            KeyCode::Char(']') if at + 1 < f.fallbacks.len() => {
                f.fallbacks.swap(at, at + 1);
                f.sub = at + 1;
            }
            KeyCode::Char('x') | KeyCode::Backspace | KeyCode::Delete if at < f.fallbacks.len() => {
                f.fallbacks.remove(at);
            }
            _ => return false,
        }
        true
    }

    /// Agents that can join the chain: runnable ones not already in it, the bot's own included.
    fn free_agents(&self, f: &Form) -> Vec<String> {
        self.agent_choices()
            .into_iter()
            .filter(|a| *a != f.backend && !f.fallbacks.iter().any(|x| x.backend == *a) && a != "custom")
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app;
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn the_fallbacks_field_adds_moves_and_removes_agents() {
        let mut app = test_app();
        app.backends = ["claude", "codex", "gemini"]
            .iter()
            .map(|id| json!({"id": id, "name": id, "available": true, "installed": true}))
            .collect();
        let bot =
            json!({"id": "b", "name": "Lead", "backend": "claude", "fallbacks": [{"backend": "codex", "model": null}]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        f.field = FIELDS.iter().position(|x| *x == Field::Fallbacks).expect("a fallbacks field");
        let key = |c: KeyCode| KeyEvent::from(c);
        assert!(app.fallback_key(&mut f, key(KeyCode::Right)), "to the add slot");
        assert!(app.fallback_key(&mut f, key(KeyCode::Char(' '))));
        let agents = |f: &Form| f.fallbacks.iter().map(|x| x.backend.clone()).collect::<Vec<_>>();
        assert_eq!(agents(&f), ["codex", "gemini"], "the next free agent, never the bot's own");
        assert!(f.dirty());
        f.sub = 1;
        app.fallback_key(&mut f, key(KeyCode::Char('[')));
        assert_eq!((agents(&f), f.sub), (vec!["gemini".to_owned(), "codex".to_owned()], 0));
        app.fallback_key(&mut f, key(KeyCode::Char('x')));
        assert_eq!(agents(&f), ["codex"]);
        assert!(!app.fallback_key(&mut f, key(KeyCode::Char('q'))), "other keys aren't the field's");
        assert!(!f.dirty(), "back to the list it opened with");
    }

    #[tokio::test]
    async fn a_members_failure_in_a_group_opens_that_members_fallbacks() {
        let mut app = test_app();
        for bot in [
            json!({"id": "ada", "name": "Ada", "backend": "claude"}),
            json!({"id": "g", "name": "Room", "kind": "group", "members": ["ada"]}),
        ] {
            app.bots.insert(bot["id"].as_str().expect("an id").into(), Bot::parse(&bot).expect("a bot"));
        }
        let notice = |data: serde_json::Value| {
            Entry::parse(&json!({"id": "n", "botId": "g", "seq": 1, "kind": "notice", "data": data}))
                .expect("an entry")
                .1
        };
        let failed = notice(json!({"text": "The agent failed", "addFallback": true, "author": "ada"}));
        assert_eq!(app.fallback_owner("g", &failed).map(|b| b.id.as_str()), Some("ada"));
        assert!(app.fallback_owner("g", &notice(json!({"addFallback": true}))).is_none(), "never the group");
        app.add_fallback("g", &failed);
        let Some(Overlay::Form(f)) = app.overlays.last() else { panic!("Ada's editor opens") };
        assert_eq!((f.bot_id.as_str(), FIELDS[f.field]), ("ada", Field::Fallbacks));
    }
}
