//! A bot's fallback chain: when its agent can't answer (out of usage, rate limited, signed out, won't start), the
//! next agent in its list carries on in the same chat, and the bot goes back once its own agent rests enough.
//! Step 0 is the bot's own agent; step `n` is `fallbacks[n - 1]`. Which failures move on is `reason`; how long an
//! agent rests is `rest`, shared by every bot on the computer; what the next agent is told is `carry`. The actor's
//! side is `agent::bot::fallback`. Design: `docs/features/agent-fallback.md`.

pub mod carry;
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
    let mut out = cfg.clone();
    if let Some(f) = fallback_at(cfg, step) {
        out.backend.clone_from(&f.backend);
        out.command.clone_from(&f.command);
        out.model.clone_from(&f.model);
        out.model_options.clone_from(&f.model_options);
    }
    out
}

/// The step that runs agent `backend`, if the chain has it.
pub fn step_of(cfg: &BotConfig, backend: &str) -> Option<usize> {
    (0..steps(cfg)).find(|&s| self::backend(cfg, s) == backend)
}

/// Drops fallbacks that repeat an earlier agent (the bot's own included), as when the user makes a fallback the
/// bot's own agent.
pub fn tidy(cfg: &mut BotConfig) {
    let mut seen = HashSet::from([cfg.backend.clone()]);
    cfg.fallbacks.retain(|f| seen.insert(f.backend.clone()));
}

/// Whose usage step `step` runs on (see `rest::account`).
pub fn account(cfg: &BotConfig, step: usize) -> String {
    match fallback_at(cfg, step) {
        Some(f) => rest::account_of(&f.backend, f.command.as_deref()),
        None => rest::account(cfg),
    }
}

/// The agents of `cfg`'s chain that are resting at `now` (`resting` on the bot's wire shape).
pub fn resting(store: &Store, cfg: &BotConfig, now: i64) -> Vec<Value> {
    (0..steps(cfg))
        .filter_map(|s| {
            let r = rest::get(store, &account(cfg, s), now)?;
            Some(json!({"agent": backend(cfg, s), "until": r.until, "reason": r.reason}))
        })
        .collect()
}

/// An account started or stopped resting: every bot that uses it shows it.
pub fn tell_bots(hub: &Hub, rested: &str) {
    let Ok(rows) = hub.store.bots() else { return };
    for row in rows.iter().filter(|r| !r.deleted && !r.config.is_group()) {
        if (0..steps(&row.config)).any(|s| account(&row.config, s) == rested) {
            hub.emit_bot(&row.config.id);
        }
    }
}

/// `account`'s rest is over (it answered, the user asked for it again, it's signed in): bots that use it show it.
pub fn end_rest(hub: &Hub, account: &str) {
    if rest::clear(&hub.store, account) {
        tell_bots(hub, account);
    }
}

/// Agent `backend` is signed in now: its rests for being signed out are over (one for usage stays), so bots that
/// moved on to a fallback come back on their next message.
pub fn signed_in(hub: &Hub, backend: &str) {
    for account in rest::signed_out(&hub.store, backend) {
        end_rest(hub, &account);
    }
}

/// Saved fallbacks are at most `MAX_FALLBACKS`, each a known agent or a custom command, and no agent twice
/// (the bot's own included): falling back between one agent's models is that agent's own setting.
pub fn check(cfg: &BotConfig) -> Result<()> {
    if cfg.fallbacks.len() > MAX_FALLBACKS {
        bail!("a bot has at most {MAX_FALLBACKS} fallbacks");
    }
    let mut seen = HashSet::from([cfg.backend.as_str()]);
    for f in &cfg.fallbacks {
        let custom = f.command.as_deref().is_some_and(|c| !c.trim().is_empty());
        if !custom && !crate::agent::backends::is_known(&f.backend) {
            bail!("unknown fallback agent {}", f.backend);
        }
        if !seen.insert(&f.backend) {
            bail!("{} is already in this bot's agents: each agent appears once", name(&f.backend));
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
