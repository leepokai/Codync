//! The bot form's Fallbacks field: the logins that carry on, in order, when the bot's own can't answer
//! (`agent::fallback`). A login is an agent on one of its accounts or (none) on its default one, the bot's own agent
//! on another account included; the chain has each once. ←/→ picks a fallback or the "+ add" slot after them; space
//! adds the next free agent on the slot and cycles a fallback's model; `a` gives a fallback the next agent with a free
//! login; `c` its agent's next free account; `[`/`]` move it; `x` removes it. `A` on a failure notice that offers a
//! fallback opens the field.

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
                if let Some(step) = self.step_agents().into_iter().find_map(|a| self.free_step(f, a)) {
                    self.want_models(&step.backend);
                    f.fallbacks.push(step);
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
                // The agents after this step's, around to the ones before it.
                let mut agents = self.step_agents();
                let own = agents.iter().position(|a| *a == f.fallbacks[at].backend);
                agents.rotate_left(own.map_or(0, |i| i + 1));
                let other = agents.into_iter().filter(|a| *a != f.fallbacks[at].backend);
                if let Some(step) = other.into_iter().find_map(|a| self.free_step(f, a)) {
                    self.want_models(&step.backend);
                    f.fallbacks[at] = step;
                }
            }
            KeyCode::Char('c') if at < f.fallbacks.len() => {
                // Its own login, among the free ones in their order.
                let step = &f.fallbacks[at];
                let free = self.free_logins(f, &step.backend);
                let logins: Vec<Option<String>> =
                    self.logins(&step.backend).into_iter().filter(|l| *l == step.account || free.contains(l)).collect();
                let i = logins.iter().position(|l| *l == step.account).unwrap_or(0);
                f.fallbacks[at].account.clone_from(&logins[cycle(i, 1, logins.len())]);
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

    /// Runnable agents a step can be on, in the pickers' order.
    fn step_agents(&self) -> Vec<String> {
        self.agent_choices().into_iter().filter(|a| a != "custom").collect()
    }

    /// Agent `backend`'s logins: its default one (`None`), then its signed-in accounts.
    fn logins(&self, backend: &str) -> Vec<Option<String>> {
        let accounts = self.accounts(backend).unwrap_or_default().iter().filter(|a| a["signedIn"] == true);
        std::iter::once(None).chain(accounts.filter_map(|a| a["id"].as_str()).map(|id| Some(id.to_owned()))).collect()
    }

    /// The logins of agent `backend` a step can still take: the ones the chain, the bot's own included, doesn't have.
    fn free_logins(&self, f: &Form, backend: &str) -> Vec<Option<String>> {
        let taken = |l: &Option<String>| {
            (f.backend == backend && f.account == *l)
                || f.fallbacks.iter().any(|x| x.backend == backend && x.account == *l)
        };
        self.logins(backend).into_iter().filter(|l| !taken(l)).collect()
    }

    /// A step on agent `backend`'s first free login; `None` when the chain has them all.
    fn free_step(&self, f: &Form, backend: String) -> Option<Fallback> {
        let account = self.free_logins(f, &backend).into_iter().next()?;
        Some(Fallback { backend, account, ..Fallback::default() })
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
    async fn a_fallback_runs_on_a_free_login_the_bots_own_agent_included() {
        let mut app = test_app();
        let accounts = json!([
            {"id": "a1", "name": "Work", "signedIn": true},
            {"id": "a2", "signedIn": true},
            {"id": "a3", "signedIn": false},
        ]);
        app.backends = vec![
            json!({"id": "claude", "name": "Claude Code", "available": true, "installed": true, "accounts": accounts}),
            json!({"id": "codex", "name": "Codex", "available": true, "installed": true}),
        ];
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "account": "a1",
            "agentStep": {"agent": "claude", "account": "a2"},
            "resting": [{"agent": "claude", "account": "a1", "until": i64::MAX, "reason": "usageLimit"}]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        let key = |c: char| KeyEvent::from(KeyCode::Char(c));
        let steps = |f: &Form| f.fallbacks.iter().map(|x| (x.backend.clone(), x.account.clone())).collect::<Vec<_>>();
        let (claude, codex) =
            (|a: Option<&str>| ("claude".to_owned(), a.map(str::to_owned)), ("codex".to_owned(), None));
        app.fallback_key(&mut f, key(' '));
        assert_eq!(steps(&f), [claude(None)], "its own agent, on the first login it isn't on");
        app.fallback_key(&mut f, key('c'));
        assert_eq!(steps(&f), [claude(Some("a2"))], "then the signed-in accounts the chain doesn't have");
        app.fallback_key(&mut f, key('c'));
        assert_eq!(steps(&f), [claude(None)], "never the bot's own or a signed-out one");
        f.sub = 1;
        app.fallback_key(&mut f, key(' '));
        f.sub = 2;
        app.fallback_key(&mut f, key(' '));
        assert_eq!(steps(&f), [claude(None), claude(Some("a2")), codex.clone()], "each login once");
        f.sub = 0;
        app.fallback_key(&mut f, key('c'));
        app.fallback_key(&mut f, key('a'));
        assert_eq!(steps(&f)[0], claude(None), "no other agent or account is free");

        let bot = app.bots.get("b");
        let shown = |i: usize| crate::tui::view::step_item(&app, bot, i, &f.fallbacks[i]);
        assert_eq!(shown(0), "1 Claude Code · default");
        assert_eq!(shown(1), "2 Claude Code (New account) · default (in use)", "in use by login, not by agent");
        assert!(crate::tui::view::rest_note(bot.expect("the bot"), "claude", Some("a1")).is_some());
        assert!(crate::tui::view::rest_note(bot.expect("the bot"), "claude", None).is_none(), "resting by login");
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
