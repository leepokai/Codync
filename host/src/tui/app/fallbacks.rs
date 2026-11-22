//! The bot form's Fallbacks field: the logins that carry on, in order, when the bot's own can't answer
//! (`agent::fallback`). A login is an agent on one of its login profiles or (none) on its default one, the bot's own agent
//! on another login profile included; the chain has each once. ←/→ picks a fallback or the "+ add" slot after them; space
//! adds the next free agent on the slot and cycles a fallback's model; `a` gives a fallback the next agent with a free
//! login; `c` its agent's next free login profile; `[`/`]` move it; `x` removes it. `A` on a failure notice that offers a
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

    /// A key on the Fallbacks field (or Advisor mode's Threads' fallbacks); whether it was one of the field's.
    pub(super) fn fallback_key(&mut self, f: &mut Form, k: KeyEvent) -> bool {
        let len = chain(f).len();
        let slots = len + usize::from(len < MAX_FALLBACKS);
        let at = f.sub.min(slots.saturating_sub(1));
        match k.code {
            KeyCode::Left | KeyCode::Right => {
                f.sub = cycle(at, if k.code == KeyCode::Left { -1 } else { 1 }, slots.max(1));
            }
            KeyCode::Char(' ') if at == len => {
                if let Some(step) = self.step_agents().into_iter().find_map(|a| self.free_step(f, a)) {
                    self.want_models(&step.backend);
                    chain_mut(f).push(step);
                }
            }
            KeyCode::Char(' ') => self.cycle_step_model(&mut chain_mut(f)[at]),
            KeyCode::Char('a') if at < len => {
                // The agents after this step's, around to the ones before it.
                let own = chain(f)[at].backend.clone();
                let mut agents = self.step_agents();
                let i = agents.iter().position(|a| *a == own);
                agents.rotate_left(i.map_or(0, |i| i + 1));
                let other = agents.into_iter().filter(|a| *a != own);
                if let Some(step) = other.into_iter().find_map(|a| self.free_step(f, a)) {
                    self.want_models(&step.backend);
                    chain_mut(f)[at] = step;
                }
            }
            KeyCode::Char('c') if at < len => {
                // Its own login, among the free ones in their order.
                let step = &chain(f)[at];
                let free = self.free_logins(f, &step.backend);
                let logins: Vec<Option<String>> = self
                    .logins(&step.backend)
                    .into_iter()
                    .filter(|l| *l == step.login_profile || free.contains(l))
                    .collect();
                let i = logins.iter().position(|l| *l == step.login_profile).unwrap_or(0);
                chain_mut(f)[at].login_profile.clone_from(&logins[cycle(i, 1, logins.len())]);
            }
            KeyCode::Char('[') if at > 0 && at < len => {
                chain_mut(f).swap(at, at - 1);
                f.sub = at - 1;
            }
            KeyCode::Char(']') if at + 1 < len => {
                chain_mut(f).swap(at, at + 1);
                f.sub = at + 1;
            }
            KeyCode::Char('x') | KeyCode::Backspace | KeyCode::Delete if at < len => {
                chain_mut(f).remove(at);
            }
            _ => return false,
        }
        true
    }

    /// Advisor mode on: its threads start on the bot's own agent, model and fallbacks (on a custom command, which a
    /// step can't be, on the first agent one can); off: no threads' agent or chain.
    pub(super) fn toggle_advisor(&mut self, f: &mut Form) {
        if f.thread_agent.take().is_some() {
            f.thread_fallbacks.clear();
        } else if f.backend != "custom" {
            let model = f.model.text.trim();
            f.thread_agent = Some(Fallback {
                backend: f.backend.clone(),
                login_profile: f.login_profile.clone(),
                model: (!model.is_empty()).then(|| model.to_owned()),
                model_options: f.model_options().cloned().unwrap_or_default(),
                ..Fallback::default()
            });
            f.thread_fallbacks.clone_from(&f.fallbacks);
        } else if let Some(backend) = self.step_agents().into_iter().next() {
            self.want_models(&backend);
            f.thread_agent = Some(Fallback { backend, ..Fallback::default() });
        }
    }

    /// ←/→ on Threads' agent: the next agent a step can be on, on its default login; a threads' fallback there goes.
    pub(super) fn cycle_thread_agent(&mut self, f: &mut Form, d: isize) {
        let agents = self.step_agents();
        let Some(agent) = &f.thread_agent else { return };
        if agents.is_empty() {
            return;
        }
        let i = agents.iter().position(|a| *a == agent.backend).unwrap_or(0);
        let backend = agents[cycle(i, d, agents.len())].clone();
        f.thread_fallbacks.retain(|x| x.backend != backend || x.login_profile.is_some());
        self.want_models(&backend);
        f.thread_agent = Some(Fallback { backend, ..Fallback::default() });
    }

    /// Space on a fallback or on Threads' agent: its next model.
    pub(super) fn cycle_step_model(&mut self, step: &mut Fallback) {
        self.want_models(&step.backend);
        if let Some(model) = self.cycle_model(&step.backend, step.model.as_deref().unwrap_or_default(), 1) {
            step.model = (!model.is_empty()).then_some(model);
            step.model_options.clear();
        }
    }

    /// `c` on Threads' agent: its next login; a threads' fallback on that login goes.
    pub(super) fn cycle_thread_login(&self, f: &mut Form) {
        let Some(agent) = f.thread_agent.as_mut() else { return };
        let logins = self.logins(&agent.backend);
        let i = logins.iter().position(|l| *l == agent.login_profile).unwrap_or(0);
        agent.login_profile.clone_from(&logins[cycle(i, 1, logins.len())]);
        let head = (agent.backend.clone(), agent.login_profile.clone());
        f.thread_fallbacks.retain(|x| (&x.backend, &x.login_profile) != (&head.0, &head.1));
    }

    /// Runnable agents a step can be on, in the pickers' order.
    fn step_agents(&self) -> Vec<String> {
        self.agent_choices().into_iter().filter(|a| a != "custom").collect()
    }

    /// Agent `backend`'s logins: its default one (`None`), then its signed-in login profiles.
    fn logins(&self, backend: &str) -> Vec<Option<String>> {
        let login_profiles = self.login_profiles(backend).unwrap_or_default().iter().filter(|a| a["signedIn"] == true);
        std::iter::once(None)
            .chain(login_profiles.filter_map(|a| a["id"].as_str()).map(|id| Some(id.to_owned())))
            .collect()
    }

    /// The logins of agent `backend` a step can still take: the ones the focused chain, its first agent's included,
    /// doesn't have.
    fn free_logins(&self, f: &Form, backend: &str) -> Vec<Option<String>> {
        let (own, own_login) = head(f);
        let taken = |l: &Option<String>| {
            (own == backend && own_login == l.as_deref())
                || chain(f).iter().any(|x| x.backend == backend && x.login_profile == *l)
        };
        self.logins(backend).into_iter().filter(|l| !taken(l)).collect()
    }

    /// A step on agent `backend`'s first free login; `None` when the chain has them all.
    fn free_step(&self, f: &Form, backend: String) -> Option<Fallback> {
        let login_profile = self.free_logins(f, &backend).into_iter().next()?;
        Some(Fallback { backend, login_profile, ..Fallback::default() })
    }
}

/// Whether the focused field is Advisor mode's Threads' fallbacks, not the bot's own.
fn threads(f: &Form) -> bool {
    f.current() == Field::ThreadFallbacks
}

/// The chain the focused field edits.
fn chain(f: &Form) -> &[Fallback] {
    if threads(f) { &f.thread_fallbacks } else { &f.fallbacks }
}

fn chain_mut(f: &mut Form) -> &mut Vec<Fallback> {
    if threads(f) { &mut f.thread_fallbacks } else { &mut f.fallbacks }
}

/// The agent and login that chain starts at: the bot's own, or its threads'.
fn head(f: &Form) -> (&str, Option<&str>) {
    match f.thread_agent.as_ref().filter(|_| threads(f)) {
        Some(agent) => (&agent.backend, agent.login_profile.as_deref()),
        None => (&f.backend, f.login_profile.as_deref()),
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
        let login_profiles = json!([
            {"id": "a1", "name": "Work", "signedIn": true},
            {"id": "a2", "signedIn": true},
            {"id": "a3", "signedIn": false},
        ]);
        app.backends = vec![
            json!({"id": "claude", "name": "Claude Code", "available": true, "installed": true, "loginProfiles": login_profiles}),
            json!({"id": "codex", "name": "Codex", "available": true, "installed": true}),
        ];
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "loginProfile": "a1",
            "agentStep": {"agent": "claude", "loginProfile": "a2"},
            "resting": [{"agent": "claude", "loginProfile": "a1", "until": i64::MAX, "reason": "usageLimit"}]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        let key = |c: char| KeyEvent::from(KeyCode::Char(c));
        let steps =
            |f: &Form| f.fallbacks.iter().map(|x| (x.backend.clone(), x.login_profile.clone())).collect::<Vec<_>>();
        let (claude, codex) =
            (|a: Option<&str>| ("claude".to_owned(), a.map(str::to_owned)), ("codex".to_owned(), None));
        app.fallback_key(&mut f, key(' '));
        assert_eq!(steps(&f), [claude(None)], "its own agent, on the first login it isn't on");
        app.fallback_key(&mut f, key('c'));
        assert_eq!(steps(&f), [claude(Some("a2"))], "then the signed-in login profiles the chain doesn't have");
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
        assert_eq!(steps(&f)[0], claude(None), "no other agent or login profile is free");

        let bot = app.bots.get("b");
        let shown = |i: usize| crate::tui::view::step_item(&app, bot, i, &f.fallbacks[i], true);
        assert_eq!(shown(0), "1 Claude Code · default");
        assert_eq!(shown(1), "2 Claude Code (New login profile) · default (in use)", "in use by login, not by agent");
        assert!(crate::tui::view::rest_note(bot.expect("the bot"), "claude", Some("a1")).is_some());
        assert!(crate::tui::view::rest_note(bot.expect("the bot"), "claude", None).is_none(), "resting by login");
    }

    #[tokio::test]
    async fn advisor_mode_gives_the_threads_an_agent_and_fallbacks_of_their_own() {
        let mut app = test_app();
        app.backends = ["claude", "codex", "gemini"]
            .iter()
            .map(|id| json!({"id": id, "name": id, "available": true, "installed": true}))
            .collect();
        let bot = json!({"id": "b", "name": "Lead", "backend": "claude", "model": "opus",
            "fallbacks": [{"backend": "codex", "model": null}], "threadAgent": null, "threadFallbacks": []});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        let at = |field: Field| FIELDS.iter().position(|x| *x == field).expect("the field");
        assert!(app.field_shown(&f, Field::Advisor) && !app.field_shown(&f, Field::ThreadAgent), "off");
        assert_eq!(f.changes().get("threadAgent"), None, "nothing changed yet");

        app.toggle_advisor(&mut f);
        let agent = f.thread_agent.clone().expect("on");
        assert_eq!(
            (agent.backend.as_str(), agent.model.as_deref()),
            ("claude", Some("opus")),
            "the bot's own to start"
        );
        assert_eq!(f.thread_fallbacks, f.fallbacks, "and its chain");
        app.cycle_thread_agent(&mut f, 1);
        let agents = |list: &[Fallback]| list.iter().map(|x| x.backend.clone()).collect::<Vec<_>>();
        assert_eq!(f.thread_agent.as_ref().map(|a| a.backend.as_str()), Some("codex"));
        assert_eq!(agents(&f.thread_fallbacks), [] as [String; 0], "the threads' own agent leaves their chain");
        f.field = at(Field::ThreadFallbacks);
        app.fallback_key(&mut f, KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(agents(&f.thread_fallbacks), ["claude"], "the next agent the threads' chain doesn't have");
        assert_eq!(agents(&f.fallbacks), ["codex"], "the chat's chain is its own");
        assert_eq!(f.changes()["threadAgent"]["backend"], "codex");

        app.toggle_advisor(&mut f);
        assert!(f.thread_agent.is_none() && f.thread_fallbacks.is_empty());
        let older = json!({"id": "o", "name": "Old", "backend": "claude"});
        app.bots.insert("o".into(), Bot::parse(&older).expect("a bot"));
        app.edit_bot_of("o");
        let Some(Overlay::Form(f)) = app.overlays.pop() else { panic!("the editor opens") };
        assert!(!app.field_shown(&f, Field::Advisor), "an older host has no Advisor mode");
        assert!(!f.dirty(), "nor sends it");
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
