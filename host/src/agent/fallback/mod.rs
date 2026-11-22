//! A bot's fallback chain: when its agent can't answer (out of usage, rate limited, signed out, won't start), the
//! next agent in its list carries on in the same chat, and the bot goes back once its own agent rests enough.
//! Step 0 is the bot's own agent; step `n` is `fallbacks[n - 1]`. What tells the steps apart is `login`; which
//! failures move on is `reason`; how long a login rests is `rest`, shared by every bot on the computer; what the
//! next agent is told is `carry`. The actor's side is `agent::bot::fallback`. Design: `docs/features/agent-fallback.md`.

pub mod carry;
pub mod login;
pub mod reason;
pub mod rest;

pub use reason::Reason;

use crate::hub::Hub;
use crate::store::{BotConfig, Fallback, Store};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::HashSet;

/// Most fallbacks a bot has.
pub const MAX_FALLBACKS: usize = 3;

/// How many steps the bot's chain has, its own agent included.
pub fn steps(cfg: &BotConfig) -> usize {
    1 + cfg.fallbacks.len()
}

/// Step `step`'s fallback; `None` for the bot's own agent (and a step past the end).
fn fallback_at(cfg: &BotConfig, step: usize) -> Option<&Fallback> {
    step.checked_sub(1).and_then(|i| cfg.fallbacks.get(i))
}

/// The agent of step `step`.
pub fn backend(cfg: &BotConfig, step: usize) -> &str {
    fallback_at(cfg, step).map_or(&cfg.backend, |f| &f.backend)
}

/// The bot as step `step` runs it: that step's agent and model, everything else the bot's.
pub fn effective(cfg: &BotConfig, step: usize) -> BotConfig {
    fallback_at(cfg, step).map_or_else(|| cfg.clone(), |f| on(cfg, f))
}

/// The bot on step `f`: its agent, login and model, everything else the bot's.
fn on(cfg: &BotConfig, f: &Fallback) -> BotConfig {
    BotConfig {
        backend: f.backend.clone(),
        command: f.command.clone(),
        login_profile: f.login_profile.clone(),
        model: f.model.clone(),
        model_options: f.model_options.clone(),
        ..cfg.clone()
    }
}

/// The bot as its threads run it: in Advisor mode on its threads' agent (`thread_agent`) and their fallbacks, as a
/// step on that agent runs, everything else the bot's; otherwise the bot as it is. A picked mode of the chat's agent
/// becomes Auto on another, as a client's edit makes it, and the chat's Fast mode (a model option) stays with it.
pub fn threads(cfg: &BotConfig) -> BotConfig {
    let Some(agent) = &cfg.thread_agent else { return cfg.clone() };
    let other = agent.backend != cfg.backend;
    BotConfig {
        fast_mode: cfg.fast_mode && !other,
        mode: if other { codync_agent::modes::on_other_agent(cfg.mode.as_deref()) } else { cfg.mode.clone() },
        fallbacks: cfg.thread_fallbacks.clone(),
        ..on(cfg, agent)
    }
}

/// The first step, in order, whose login `passed` doesn't pass over and that isn't resting at `now`.
pub fn first_step(store: &Store, cfg: &BotConfig, now: i64, passed: impl Fn(&str) -> bool) -> Option<usize> {
    (0..steps(cfg)).find(|&s| {
        let login = login(cfg, s);
        !passed(&login) && rest::get(store, &login, now).is_none()
    })
}

/// The bot as a new turn of its starts at `now`: on the first step whose login isn't resting (its own agent when
/// every one is).
pub fn starting(store: &Store, cfg: &BotConfig, now: i64) -> BotConfig {
    effective(cfg, first_step(store, cfg, now, |_| false).unwrap_or(0))
}

/// The step that runs on login `login`, if the chain has it.
pub fn step_of(cfg: &BotConfig, login: &str) -> Option<usize> {
    (0..steps(cfg)).find(|&s| self::login(cfg, s) == login)
}

/// Drops fallbacks that repeat an earlier login (the bot's own included), as when the user makes a fallback the
/// bot's own agent. The same agent on another of its login profiles is another login.
pub fn tidy(cfg: &mut BotConfig) {
    let mut seen = HashSet::from([login::of(cfg)]);
    cfg.fallbacks.retain(|f| seen.insert(step_login(f)));
    // Advisor mode: the threads' chain starts at their own agent, and there is none without it.
    let Some(threads) = cfg.thread_agent.as_ref().map(step_login) else {
        cfg.thread_fallbacks.clear();
        return;
    };
    let mut seen = HashSet::from([threads]);
    cfg.thread_fallbacks.retain(|f| seen.insert(step_login(f)));
}

fn step_login(f: &Fallback) -> String {
    login::key(&f.backend, f.command.as_deref(), f.login_profile.as_deref())
}

/// The login step `step` runs on (`login`): what tells the chain's steps apart, and whose usage a rest is about.
pub fn login(cfg: &BotConfig, step: usize) -> String {
    fallback_at(cfg, step).map_or_else(|| login::of(cfg), step_login)
}

/// For each agent of a list that came in order without what it can't show (a bot's own tools), the saved step it stands for: the n-th step of an agent is the n-th saved one of it.
pub fn saved_steps<'a>(saved: &'a [Fallback], agents: &[&str]) -> Vec<Option<&'a Fallback>> {
    let nth = |i: usize| agents[..i].iter().filter(|a| **a == agents[i]).count();
    (0..agents.len()).map(|i| saved.iter().filter(|f| f.backend == agents[i]).nth(nth(i))).collect()
}

/// The agents of `cfg`'s chain that are resting at `now` (`resting` on the bot's wire shape).
pub fn resting(store: &Store, cfg: &BotConfig, now: i64) -> Vec<Value> {
    let mut out = Vec::new();
    // Advisor mode's threads' chain as well, so its rows say so too.
    for cfg in chains(cfg) {
        for s in 0..steps(&cfg) {
            let Some(r) = rest::get(store, &login(&cfg, s), now) else { continue };
            let profile = fallback_at(&cfg, s).map_or(cfg.login_profile.as_ref(), |f| f.login_profile.as_ref());
            let rests =
                json!({"agent": backend(&cfg, s), "loginProfile": profile, "until": r.until, "reason": r.reason});
            if !out.contains(&rests) {
                out.push(rests);
            }
        }
    }
    out
}

/// `cfg`'s chain, and in Advisor mode its threads' too.
fn chains(cfg: &BotConfig) -> impl Iterator<Item = Cow<'_, BotConfig>> {
    std::iter::once(Cow::Borrowed(cfg)).chain(cfg.thread_agent.is_some().then(|| Cow::Owned(threads(cfg))))
}

/// Login `rested` started or stopped resting: every bot that uses it shows it.
pub fn tell_bots(hub: &Hub, rested: &str) {
    let Ok(rows) = hub.store.bots() else { return };
    for row in rows.iter().filter(|r| !r.deleted && !r.config.is_group()) {
        if chains(&row.config).any(|cfg| (0..steps(&cfg)).any(|s| login(&cfg, s) == rested)) {
            hub.emit_bot(&row.config.id);
        }
    }
}

/// `login`'s rest is over (it answered, the user asked for it again, it's signed in): bots that use it show it.
pub fn end_rest(hub: &Hub, login: &str) {
    if rest::clear(&hub.store, login) {
        tell_bots(hub, login);
    }
}

/// Agent `backend` is signed in now on `login_profile`, or on the computer's own login: that login's rests for
/// being signed out are over (one for usage stays), so bots that moved on to a fallback come back on their next
/// message.
pub fn signed_in(hub: &Hub, backend: &str, login_profile: Option<&str>) {
    for login in rest::signed_out(&hub.store, backend, login_profile) {
        end_rest(hub, &login);
    }
}

/// Saved fallbacks are at most `MAX_FALLBACKS`, each a known agent or a custom command, and no login twice
/// (the bot's own included): an agent appears again only on another of its login profiles. Falling back between one
/// agent's models is that agent's own setting.
pub fn check(cfg: &BotConfig) -> Result<()> {
    if cfg.fallbacks.len() > MAX_FALLBACKS {
        bail!("a bot has at most {MAX_FALLBACKS} fallbacks");
    }
    let mut seen = HashSet::from([login::of(cfg)]);
    for f in &cfg.fallbacks {
        let custom = f.command.as_deref().is_some_and(|c| !c.trim().is_empty());
        if !custom && !codync_agent::backends::is_known(&f.backend) {
            bail!("unknown fallback agent {}", f.backend);
        }
        if !seen.insert(step_login(f)) {
            bail!("{} is already in this bot's agents: each agent appears once per login profile", name(&f.backend));
        }
    }
    Ok(())
}

/// What the user calls agent `backend`.
pub fn name(backend: &str) -> String {
    if backend == "custom" {
        return "Custom command".into();
    }
    if let Some(h) = codync_agent::backends::harness(backend) {
        return h.name.into();
    }
    codync_agent::registry::agent(backend)
        .and_then(|a| a["name"].as_str().map(str::to_owned))
        .unwrap_or_else(|| backend.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(fallbacks: &[(&str, Option<&str>)]) -> BotConfig {
        let mut cfg: BotConfig =
            serde_json::from_value(serde_json::json!({"id": "b", "name": "B", "backend": "claude", "model": "opus"}))
                .expect("a bot");
        cfg.fallbacks = fallbacks
            .iter()
            .map(|(backend, model)| Fallback {
                backend: (*backend).into(),
                model: model.map(Into::into),
                ..Fallback::default()
            })
            .collect();
        cfg
    }

    #[test]
    fn an_agent_appears_again_on_another_of_its_login_profiles() {
        let mut cfg = bot(&[("claude", None), ("claude", None), ("codex", None), ("claude", None)]);
        cfg.fallbacks[0].login_profile = Some("a1".into());
        cfg.fallbacks[1].login_profile = Some("a1".into());
        cfg.fallbacks[3].login_profile = Some("a2".into());
        assert!(check(&cfg).is_err(), "the same login profile twice");
        tidy(&mut cfg);
        let steps: Vec<_> = cfg.fallbacks.iter().map(|f| (f.backend.as_str(), f.login_profile.as_deref())).collect();
        assert_eq!(steps, [("claude", Some("a1")), ("codex", None), ("claude", Some("a2"))]);
        check(&cfg).expect("each login once");
        assert_eq!(effective(&cfg, 1).login_profile.as_deref(), Some("a1"), "the step runs on its login profile");
        assert_eq!(effective(&cfg, 2).login_profile, None, "another agent runs on the computer's own login");
        assert_eq!(step_of(&cfg, &login(&cfg, 3)), Some(3));
        assert_eq!(step_of(&cfg, "claude"), Some(0), "the bot's own agent, on the computer's own login");
        // The bot's own agent on a login profile leaves the computer's own login free for a fallback.
        cfg.login_profile = Some("a1".into());
        tidy(&mut cfg);
        assert_eq!(cfg.fallbacks.len(), 2, "the step on the bot's own login profile goes");
    }

    #[test]
    fn in_advisor_mode_threads_run_on_their_own_agent_and_fallbacks() {
        let step = |backend: &str, model: &str| Fallback {
            backend: backend.into(),
            model: Some(model.into()),
            ..Fallback::default()
        };
        let base = BotConfig { mode: Some("acceptEdits".into()), fast_mode: true, ..bot(&[("codex", None)]) };
        assert_eq!(threads(&base).backend, "claude", "off: the chat's own agent");
        let advisor = BotConfig {
            thread_agent: Some(step("codex", "gpt-5")),
            thread_fallbacks: vec![step("gemini", "pro")],
            ..base.clone()
        };
        let on = threads(&advisor);
        assert_eq!(
            (on.backend.as_str(), on.model.as_deref(), on.mode.as_deref(), on.fast_mode),
            ("codex", Some("gpt-5"), Some(codync_agent::modes::AUTO), false),
            "another agent: its own model, Auto for the chat's picked mode, no fast mode"
        );
        assert_eq!(on.fallbacks, [step("gemini", "pro")], "the threads' fallbacks, not the chat's");
        let lighter = BotConfig { thread_agent: Some(step("claude", "haiku")), ..base };
        assert_eq!(threads(&lighter).mode.as_deref(), Some("acceptEdits"), "the same agent keeps the mode");
    }

    #[test]
    fn advisor_modes_chain_starts_at_the_threads_agent_and_goes_with_it() {
        let mut cfg = bot(&[]);
        cfg.thread_agent = Some(Fallback { backend: "codex".into(), ..Fallback::default() });
        cfg.thread_fallbacks = bot(&[("claude", None), ("codex", None), ("gemini", None), ("gemini", None)]).fallbacks;
        tidy(&mut cfg);
        let steps: Vec<_> = cfg.thread_fallbacks.iter().map(|f| f.backend.as_str()).collect();
        assert_eq!(
            steps,
            ["claude", "gemini"],
            "the chat's agent may carry the threads on; the threads' own is theirs"
        );
        cfg.thread_agent = None;
        tidy(&mut cfg);
        assert!(cfg.thread_fallbacks.is_empty(), "off: no threads' chain is kept");
    }

    #[test]
    fn a_new_turn_starts_on_the_first_agent_that_isnt_resting() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let cfg = bot(&[("codex", Some("gpt")), ("gemini", None)]);
        let now = crate::store::now_ms();
        assert_eq!(starting(&store, &cfg, now).backend, "claude");
        rest::record(&store, "claude", Reason::UsageLimit, now, None);
        let next = starting(&store, &cfg, now);
        assert_eq!((next.backend.as_str(), next.model.as_deref()), ("codex", Some("gpt")));
        rest::record(&store, "codex", Reason::RateLimited, now, None);
        rest::record(&store, "gemini", Reason::SignedOut, now, None);
        assert_eq!(starting(&store, &cfg, now).backend, "claude", "every one rests: its own");
    }

    #[test]
    fn a_list_sent_by_agent_alone_stands_for_the_saved_steps_in_order() {
        let mut cfg = bot(&[("claude", None), ("codex", None), ("claude", None)]);
        cfg.fallbacks[0].login_profile = Some("a1".into());
        cfg.fallbacks[2].login_profile = Some("a2".into());
        let profiles = |agents: &[&str]| -> Vec<Option<&str>> {
            let saved = saved_steps(&cfg.fallbacks, agents);
            saved.into_iter().map(|f| f.and_then(|f| f.login_profile.as_deref())).collect()
        };
        assert_eq!(profiles(&["claude", "claude", "codex"]), [Some("a1"), Some("a2"), None]);
        assert_eq!(profiles(&["gemini", "claude", "claude", "claude"]), [None, Some("a1"), Some("a2"), None]);
    }

    #[test]
    fn a_step_runs_its_own_agent_and_model_with_the_bots_settings() {
        let cfg = bot(&[("codex", Some("gpt-5.5")), ("gemini", None)]);
        assert_eq!(steps(&cfg), 3);
        assert_eq!(effective(&cfg, 0).model.as_deref(), Some("opus"));
        let codex = effective(&cfg, 1);
        assert_eq!((codex.backend.as_str(), codex.model.as_deref()), ("codex", Some("gpt-5.5")));
        assert_eq!(codex.id, "b");
        assert_eq!(effective(&cfg, 2).model, None);
        assert_eq!(step_of(&cfg, "gemini"), Some(2));
        assert_eq!(backend(&cfg, 9), "claude", "a step past the end is the bot's own");
    }

    #[test]
    fn the_chain_names_each_agent_once() {
        crate::agent::init_runtime();
        assert!(check(&bot(&[("codex", None), ("gemini", None)])).is_ok());
        assert!(check(&bot(&[("claude", Some("sonnet"))])).is_err(), "the bot's own agent again");
        assert!(check(&bot(&[("codex", None), ("codex", Some("o3"))])).is_err());
        assert!(check(&bot(&[("no-such-agent", None)])).is_err());
        assert!(check(&bot(&[("codex", None), ("gemini", None), ("cursor", None), ("qwen", None)])).is_err());
    }

    #[test]
    fn making_a_fallback_the_bots_own_agent_drops_it_from_the_list() {
        let mut cfg = bot(&[("codex", None), ("gemini", None), ("codex", Some("o3"))]);
        cfg.backend = "gemini".into();
        tidy(&mut cfg);
        assert_eq!(cfg.fallbacks.iter().map(|f| f.backend.as_str()).collect::<Vec<_>>(), ["codex"]);
    }
}
