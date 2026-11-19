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
use std::collections::HashSet;

/// Most fallbacks a bot has.
pub const MAX_FALLBACKS: usize = 3;

/// The steps of `cfg`'s chain: its own agent and each saved fallback, with any other signed-in login profiles of an
/// agent in `store` automatically tried right after that agent's configured step (keeping its model and options)
/// before moving on to the next saved fallback.
fn chain(store: &Store, cfg: &BotConfig) -> Vec<Fallback> {
    let mut steps = Vec::with_capacity(1 + cfg.fallbacks.len());
    let mut seen = HashSet::new();
    let configured: Vec<Fallback> = std::iter::once(Fallback {
        backend: cfg.backend.clone(),
        command: cfg.command.clone(),
        login_profile: cfg.login_profile.clone(),
        model: cfg.model.clone(),
        model_options: cfg.model_options.clone(),
    })
    .chain(cfg.fallbacks.iter().cloned())
    .collect();
    // Reserve every explicitly configured login so an earlier step's automatic rotation never replaces its model or
    // options.
    let explicit: HashSet<String> = configured.iter().map(step_login).collect();
    for step in configured {
        if seen.insert(step_login(&step)) {
            steps.push(step.clone());
        }
        let custom = step.command.as_deref().is_some_and(|c| !c.trim().is_empty());
        if !custom {
            for id in crate::agent::login_profiles::signed_in_ids(store, &step.backend) {
                let extra = Fallback { login_profile: Some(id), ..step.clone() };
                let login = step_login(&extra);
                if !explicit.contains(&login) && seen.insert(login) {
                    steps.push(extra);
                }
            }
        }
    }
    steps
}

/// How many steps the bot's chain has, its own agent and any signed-in login profiles included.
pub fn steps(store: &Store, cfg: &BotConfig) -> usize {
    chain(store, cfg).len()
}

/// Applies `step` to `cfg`: that step's agent, login profile, command and model, everything else `cfg`'s.
pub fn apply(cfg: &BotConfig, step: &Fallback) -> BotConfig {
    let mut out = cfg.clone();
    out.backend.clone_from(&step.backend);
    out.command.clone_from(&step.command);
    out.login_profile.clone_from(&step.login_profile);
    out.model.clone_from(&step.model);
    out.model_options.clone_from(&step.model_options);
    out
}

/// The bot as step `step` runs it: that step's agent and model, everything else the bot's.
pub fn effective(store: &Store, cfg: &BotConfig, step: usize) -> BotConfig {
    chain(store, cfg).into_iter().nth(step).map_or_else(|| cfg.clone(), |f| apply(cfg, &f))
}

/// A step chosen from one `chain` snapshot: its configuration and any signed-out logins before it.
#[derive(Clone, Debug)]
pub struct Picked {
    pub cfg: BotConfig,
    pub signed_out_before: Vec<crate::agent::login_profiles::Login>,
}

impl Picked {
    /// The bot's own agent (step 0), with no preceding signed-out logins.
    pub fn own(cfg: &BotConfig) -> Self {
        Self { cfg: cfg.clone(), signed_out_before: Vec::new() }
    }
}

/// The first step, in order, whose login `passed` doesn't pass over and that isn't resting at `now`, with any steps
/// before it that rest because a bot found them signed out. Evaluated from one `chain` snapshot so concurrent
/// profile changes cannot shift a selected index into another step.
pub fn pick_step(store: &Store, cfg: &BotConfig, now: i64, passed: impl Fn(&str) -> bool) -> Option<Picked> {
    let steps = chain(store, cfg);
    let idx = steps.iter().position(|f| {
        let login = step_login(f);
        !passed(&login) && rest::get(store, &login, now).is_none()
    })?;
    let chosen = apply(cfg, &steps[idx]);
    let signed_out_before = steps[..idx]
        .iter()
        .filter(|f| rest::get(store, &step_login(f), now).is_some_and(|r| r.reason == Reason::SignedOut))
        .map(|f| crate::agent::login_profiles::Login::of(store, &apply(cfg, f)))
        .collect();
    Some(Picked { cfg: chosen, signed_out_before })
}

/// The bot as a new turn of its starts at `now`: on the first step whose login isn't resting (its own agent when
/// every one is).
pub fn starting(store: &Store, cfg: &BotConfig, now: i64) -> BotConfig {
    pick_step(store, cfg, now, |_| false).map_or_else(|| cfg.clone(), |p| p.cfg)
}

/// The step that runs on login `login`, if the chain has it.
pub fn step_of(store: &Store, cfg: &BotConfig, login: &str) -> Option<usize> {
    chain(store, cfg).iter().position(|f| step_login(f) == login)
}

/// Restores `cfg` on `target_login`, even when an automatically added profile left the active chain (e.g. signed out).
/// Returns `None` when `target_login` refers to a deleted profile or an unknown custom command.
pub fn for_login(store: &Store, cfg: &BotConfig, target_login: &str) -> Option<BotConfig> {
    if let Some(step) = step_of(store, cfg, target_login) {
        return Some(effective(store, cfg, step));
    }
    let backend = login::backend_of(target_login);
    let custom_of = |command: Option<&str>| command.is_some_and(|c| !c.trim().is_empty());
    let (model, model_options) = if cfg.backend == backend && !custom_of(cfg.command.as_deref()) {
        (cfg.model.clone(), cfg.model_options.clone())
    } else if let Some(saved) = cfg.fallbacks.iter().find(|f| f.backend == backend && !custom_of(f.command.as_deref()))
    {
        (saved.model.clone(), saved.model_options.clone())
    } else {
        (None, std::collections::BTreeMap::new())
    };
    if let Some(profile_id) = login::login_profile(target_login) {
        let profile = crate::agent::login_profiles::find(store, profile_id).ok()?;
        if profile.account.agent != crate::agent::backends::agent_id(backend) {
            return None;
        }
        return Some(apply(
            cfg,
            &Fallback {
                backend: backend.to_owned(),
                command: None,
                login_profile: Some(profile_id.to_owned()),
                model,
                model_options,
            },
        ));
    }
    if !target_login.contains('#') && crate::agent::backends::is_known(target_login) {
        return Some(apply(
            cfg,
            &Fallback { backend: target_login.to_owned(), command: None, login_profile: None, model, model_options },
        ));
    }
    None
}

/// Drops fallbacks that repeat an earlier login (the bot's own included), as when the user makes a fallback the
/// bot's own agent. The same agent on another of its login profiles is another login.
pub fn tidy(cfg: &mut BotConfig) {
    let mut seen = HashSet::from([login::of(cfg)]);
    cfg.fallbacks.retain(|f| seen.insert(step_login(f)));
}

fn step_login(f: &Fallback) -> String {
    login::key(&f.backend, f.command.as_deref(), f.login_profile.as_deref())
}

/// The login step `step` runs on (`login`): what tells the chain's steps apart, and whose usage a rest is about.
pub fn login(store: &Store, cfg: &BotConfig, step: usize) -> String {
    chain(store, cfg).get(step).map_or_else(|| login::of(cfg), step_login)
}

/// For each agent of a list that came in order without what it can't show (a bot's own tools), the saved step it stands for: the n-th step of an agent is the n-th saved one of it.
pub fn saved_steps<'a>(saved: &'a [Fallback], agents: &[&str]) -> Vec<Option<&'a Fallback>> {
    let nth = |i: usize| agents[..i].iter().filter(|a| **a == agents[i]).count();
    (0..agents.len()).map(|i| saved.iter().filter(|f| f.backend == agents[i]).nth(nth(i))).collect()
}

/// The agents of `cfg`'s chain that are resting at `now` (`resting` on the bot's wire shape).
pub fn resting(store: &Store, cfg: &BotConfig, now: i64) -> Vec<Value> {
    chain(store, cfg)
        .into_iter()
        .filter_map(|f| {
            let r = rest::get(store, &step_login(&f), now)?;
            Some(json!({"agent": f.backend, "loginProfile": f.login_profile, "until": r.until, "reason": r.reason}))
        })
        .collect()
}

/// Login `rested` started or stopped resting: every bot that uses it shows it.
pub fn tell_bots(hub: &Hub, rested: &str) {
    let Ok(rows) = hub.store.bots() else { return };
    for row in rows.iter().filter(|r| !r.deleted && !r.config.is_group()) {
        if chain(&hub.store, &row.config).iter().any(|f| step_login(f) == rested) {
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
        if !custom && !crate::agent::backends::is_known(&f.backend) {
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
    if let Some(h) = crate::agent::backends::harness(backend) {
        return h.name.into();
    }
    crate::agent::registry::agent(backend)
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
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let mut cfg = bot(&[("claude", None), ("claude", None), ("codex", None), ("claude", None)]);
        cfg.fallbacks[0].login_profile = Some("a1".into());
        cfg.fallbacks[1].login_profile = Some("a1".into());
        cfg.fallbacks[3].login_profile = Some("a2".into());
        assert!(check(&cfg).is_err(), "the same login profile twice");
        tidy(&mut cfg);
        let steps: Vec<_> = cfg.fallbacks.iter().map(|f| (f.backend.as_str(), f.login_profile.as_deref())).collect();
        assert_eq!(steps, [("claude", Some("a1")), ("codex", None), ("claude", Some("a2"))]);
        check(&cfg).expect("each login once");
        assert_eq!(
            effective(&store, &cfg, 1).login_profile.as_deref(),
            Some("a1"),
            "the step runs on its login profile"
        );
        assert_eq!(effective(&store, &cfg, 2).login_profile, None, "another agent runs on the computer's own login");
        assert_eq!(step_of(&store, &cfg, &login(&store, &cfg, 3)), Some(3));
        assert_eq!(step_of(&store, &cfg, "claude"), Some(0), "the bot's own agent, on the computer's own login");
        // The bot's own agent on a login profile leaves the computer's own login free for a fallback.
        cfg.login_profile = Some("a1".into());
        tidy(&mut cfg);
        assert_eq!(cfg.fallbacks.len(), 2, "the step on the bot's own login profile goes");
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
    fn signed_in_login_profiles_of_the_same_agent_rotate_automatically_before_other_fallbacks() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        crate::agent::login_profiles::tests::insert(&store, &[("p1", "claude"), ("p2", "claude"), ("p3", "claude")]);
        crate::agent::login_profiles::note_signed_in(&store, "p1", true);
        crate::agent::login_profiles::note_signed_in(&store, "p2", true);
        let mut cfg = bot(&[("codex", Some("gpt-5.5"))]);
        cfg.login_profile = Some("p1".into());
        let now = crate::store::now_ms();
        assert_eq!(steps(&store, &cfg), 3, "p1, signed-in p2, and codex (signed-out p3 excluded)");
        assert_eq!(starting(&store, &cfg, now).login_profile.as_deref(), Some("p1"));
        rest::record(&store, &login::of(&cfg), Reason::UsageLimit, now, None);
        let on_p2 = starting(&store, &cfg, now);
        assert_eq!(
            (on_p2.backend.as_str(), on_p2.login_profile.as_deref(), on_p2.model.as_deref()),
            ("claude", Some("p2"), Some("opus")),
            "automatically rotates to the next signed-in profile of the same agent with the same model"
        );
        rest::record(&store, &login::of(&on_p2), Reason::UsageLimit, now, None);
        let on_codex = starting(&store, &cfg, now);
        assert_eq!((on_codex.backend.as_str(), on_codex.model.as_deref()), ("codex", Some("gpt-5.5")));
    }

    #[test]
    fn explicitly_saved_fallback_step_keeps_its_model_and_options_during_auto_rotation() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        crate::agent::login_profiles::tests::insert(&store, &[("p1", "claude"), ("p2", "claude"), ("p3", "claude")]);
        for id in ["p1", "p2", "p3"] {
            crate::agent::login_profiles::note_signed_in(&store, id, true);
        }
        let mut cfg = bot(&[("claude", Some("sonnet")), ("codex", Some("gpt-5.5"))]);
        cfg.login_profile = Some("p1".into());
        cfg.model_options.insert("effort".into(), "high".into());
        cfg.fallbacks[0].login_profile = Some("p2".into());
        cfg.fallbacks[0].model_options.insert("effort".into(), "low".into());

        assert_eq!(steps(&store, &cfg), 4, "p1, auto-inserted p3, explicit p2, and codex");
        let auto_p3 = effective(&store, &cfg, 1);
        assert_eq!(
            (auto_p3.backend.as_str(), auto_p3.login_profile.as_deref(), auto_p3.model.as_deref()),
            ("claude", Some("p3"), Some("opus"))
        );
        assert_eq!(auto_p3.model_options.get("effort").map(String::as_str), Some("high"));

        let saved_p2 = effective(&store, &cfg, 2);
        assert_eq!(
            (saved_p2.backend.as_str(), saved_p2.login_profile.as_deref(), saved_p2.model.as_deref()),
            ("claude", Some("p2"), Some("sonnet")),
            "explicit fallback keeps its own saved model instead of being overwritten by primary auto-rotation"
        );
        assert_eq!(saved_p2.model_options.get("effort").map(String::as_str), Some("low"));
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
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let cfg = bot(&[("codex", Some("gpt-5.5")), ("gemini", None)]);
        assert_eq!(steps(&store, &cfg), 3);
        assert_eq!(effective(&store, &cfg, 0).model.as_deref(), Some("opus"));
        let codex = effective(&store, &cfg, 1);
        assert_eq!((codex.backend.as_str(), codex.model.as_deref()), ("codex", Some("gpt-5.5")));
        assert_eq!(codex.id, "b");
        assert_eq!(effective(&store, &cfg, 2).model, None);
        assert_eq!(step_of(&store, &cfg, "gemini"), Some(2));
        assert_eq!(effective(&store, &cfg, 9).backend, "claude", "a step past the end is the bot's own");
    }

    #[test]
    fn the_chain_names_each_agent_once() {
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
