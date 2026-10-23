//! The bot form's Fallbacks field: the agents that carry on, in order, when the bot's own can't answer
//! (`agent::fallback`). ←/→ picks a fallback or the "+ add" slot after them; space adds the next free agent on the
//! slot and cycles a fallback's model; `e` cycles its Effort level through Default and the values its agent lists for
//! its model; `a` gives a fallback the next free agent; `[`/`]` move it; `x` removes it. `A`
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
            KeyCode::Char('e') if at < f.fallbacks.len() => {
                // Its agent's list loads on the first press; nothing starts that agent before.
                self.want_models(&f.fallbacks[at].backend);
                let settings = self.step_settings(f, Some(at));
                let setting = super::effort_setting(settings).cloned();
                let ids: Vec<String> = settings.iter().map(|o| o.id.clone()).collect();
                let loaded = matches!(self.models.get(&f.fallbacks[at].backend), Some(Some(_)));
                let step = &mut f.fallbacks[at];
                if setting.is_none() && loaded {
                    // No effort level listed for this model: a saved one nothing lists goes.
                    step.model_options.retain(|id, _| ids.contains(id));
                } else if let Some(o) = setting {
                    // Default (the agent's own), then each value.
                    let ids: Vec<&str> =
                        std::iter::once("").chain(o.values.iter().map(|(id, _)| id.as_str())).collect();
                    let now = step.model_options.get(&o.id).map_or("", String::as_str);
                    let next = ids[cycle(ids.iter().position(|i| *i == now).unwrap_or(0), 1, ids.len())];
                    if next.is_empty() {
                        step.model_options.remove(&o.id);
                    } else {
                        step.model_options.insert(o.id.clone(), next.to_owned());
                    }
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
    async fn each_fallback_has_its_own_effort_level() {
        let mut app = test_app();
        let level = |id: &str| {
            json!({"id": id, "name": "Effort", "category": "thought_level",
                "values": [{"id": "low", "name": "Low"}, {"id": "high", "name": "High"}]})
        };
        let catalog = |options: serde_json::Value| json!({"currentModelId": "m", "models": [{"id": "m", "name": "M", "options": options}]});
        app.models.insert("claude".into(), Some(super::super::AgentModel::list(&catalog(json!([level("effort")])))));
        app.models
            .insert("codex".into(), Some(super::super::AgentModel::list(&catalog(json!([level("reasoning_effort")])))));
        app.models.insert("gemini".into(), Some(super::super::AgentModel::list(&catalog(json!([])))));
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "modelOptions": {"effort": "high"},
            "fallbacks": [{"backend": "codex", "model": null}, {"backend": "gemini", "model": null}],
            "agentStep": {"step": 1, "agent": "codex"}, "sessionLive": true,
            "sessionOptions": {"reasoning_effort": "low"}, "sessionSettings": [level("reasoning_effort")]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        let key = |c: KeyCode| KeyEvent::from(c);
        let item =
            |app: &App, f: &Form, i: usize| crate::tui::view::fallbacks::step_item(app, None, i, &f.fallbacks[i]);
        assert_eq!(
            item(&app, &f, 0),
            "1 codex · default · effort ‹default›",
            "its own agent's list, not the bot's value"
        );
        assert_eq!(item(&app, &f, 1), "2 gemini · default", "an agent without an effort level shows none");
        let caption = |app: &App, f: &Form, step| match app.form_effort(f, step) {
            super::super::Effort::Row { caption, .. } => caption,
            other => panic!("no row: {other:?}"),
        };
        assert_eq!(caption(&app, &f, Some(0)).as_deref(), Some("In use: Low (agent default)"), "the bot runs on it");
        assert_eq!(
            caption(&app, &f, None).as_deref(),
            Some("Not running · Saved: High"),
            "the bot's own isn't running"
        );

        assert!(app.fallback_key(&mut f, key(KeyCode::Char('e'))));
        assert_eq!(f.fallbacks[0].model_options, [("reasoning_effort".to_owned(), "low".to_owned())].into());
        assert_eq!(caption(&app, &f, Some(0)), None, "nothing to report for a value not saved yet");
        app.fallback_key(&mut f, key(KeyCode::Char('e')));
        app.fallback_key(&mut f, key(KeyCode::Char('e')));
        assert!(f.fallbacks[0].model_options.is_empty(), "Default drops the entry's own value");
        app.fallback_key(&mut f, key(KeyCode::Char('e')));
        f.sub = 1;
        assert!(app.fallback_key(&mut f, key(KeyCode::Char('e'))), "the key is the field's");
        assert!(f.fallbacks[1].model_options.is_empty(), "nothing to pick where the agent lists no effort level");
        app.fallback_key(&mut f, key(KeyCode::Char('[')));
        assert_eq!(
            f.fallbacks[1].model_options,
            [("reasoning_effort".to_owned(), "low".to_owned())].into(),
            "moves with it"
        );
        assert_eq!(
            f.model_options().and_then(|o| o.get("effort")).map(String::as_str),
            Some("high"),
            "the bot's own stays"
        );
    }

    #[tokio::test]
    async fn a_running_fallbacks_own_list_lets_its_effort_level_be_picked() {
        // Reviewer 5997efbb: the fallback's agent lists its effort level only on the running session.
        let mut app = test_app();
        let catalog = json!({"currentModelId": "m", "models": [{"id": "m", "name": "M", "options": []}]});
        app.models.insert("codex".into(), Some(super::super::AgentModel::list(&catalog)));
        let level = json!({"id": "reasoning_effort", "name": "Effort", "category": "thought_level",
            "values": [{"id": "low", "name": "Low"}, {"id": "high", "name": "High"}]});
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "fallbacks": [{"backend": "codex", "model": null}],
            "agentStep": {"step": 1, "agent": "codex"}, "sessionLive": true,
            "sessionOptions": {"reasoning_effort": "high"}, "sessionSettings": [level]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        assert!(app.fallback_key(&mut f, KeyEvent::from(KeyCode::Char('e'))));
        assert_eq!(f.fallbacks[0].model_options, [("reasoning_effort".to_owned(), "low".to_owned())].into());
        let item = crate::tui::view::fallbacks::step_item(&app, app.bots.get("b"), 0, &f.fallbacks[0]);
        assert_eq!(item, "1 codex · default · effort ‹Low› (in use)");
    }

    #[tokio::test]
    async fn a_fallbacks_saved_effort_shows_without_its_agents_list_and_stays() {
        // Greptile on #204: the chain showed a saved fallback effort only where its agent listed one.
        let mut app = test_app();
        let catalog = json!({"currentModelId": "m", "models": [{"id": "m", "name": "M", "options": []}]});
        app.models.insert("gemini".into(), Some(super::super::AgentModel::list(&catalog)));
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude",
            "fallbacks": [{"backend": "gemini", "model": null,
                "modelOptions": {"reasoning_effort": "high", "verbosity": "terse"}}]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        let item = |app: &App, f: &Fallback| crate::tui::view::fallbacks::step_item(app, app.bots.get("b"), 0, f);
        let saved = app.bots["b"].fallbacks[0].clone();
        assert_eq!(item(&app, &saved), "1 gemini · default · saved ‹high, terse›");
        app.models.clear();
        assert_eq!(item(&app, &saved), "1 gemini · default · saved ‹high, terse›", "no list loaded");
        app.models.insert("gemini".into(), Some(super::super::AgentModel::list(&catalog)));
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        assert!(app.fallback_key(&mut f, KeyEvent::from(KeyCode::Char('e'))));
        // Reviewer on e3d88788: discovery lists no settings for a model it doesn't run, so absence proves nothing.
        assert_eq!(f.fallbacks[0].model_options, saved.model_options, "e never drops what its agent's list leaves out");
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
