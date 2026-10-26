//! How the terminal shows a bot's fallback chain (`agent::fallback`): the agent it's on, and each step's state.

use super::super::app::{App, Bot, Effort, effort, effort_setting_of};
use super::time::now_ms;
use super::usage::when_future;
use crate::agent::fallback::Reason;
use crate::store::Fallback;

/// Fallback `i` of `bot`'s chain: "1 Codex · gpt-5.5 · effort ‹High› (in use)", "2 Gemini CLI · default (usage resets
/// 15:00)". The effort level shows where its agent lists one for the model, as its own session reports it while the bot
/// runs on it as saved ("effort: In use: Low · Saved: High not offered"); else what is saved for it, as saved.
pub(in crate::tui) fn step_item(app: &App, bot: Option<&Bot>, i: usize, f: &Fallback) -> String {
    step_label(app, bot, i, f, true)
}

/// A fallback's entry without its effort level, for a row too narrow for the full one; the
/// settings form shows the picked entry's effort level on its own line instead.
pub(in crate::tui) fn step_item_short(app: &App, bot: Option<&Bot>, i: usize, f: &Fallback) -> String {
    step_label(app, bot, i, f, false)
}

fn step_label(app: &App, bot: Option<&Bot>, i: usize, f: &Fallback, effort_level: bool) -> String {
    let model = f.model.as_deref().unwrap_or("default");
    // While the bot runs this agent on the saved model, its session's own list; else the agent's catalog.
    let host = bot.and_then(|b| b.fallbacks.iter().find(|x| x.backend == f.backend && x.model == f.model));
    let settings = app.settings_for(bot, &f.backend, f.model.as_deref().unwrap_or_default(), host.is_some());
    let setting = effort_setting_of(&settings, bot.filter(|_| host.is_some()).and_then(|b| b.session_of(&f.backend)));
    // Running as saved, its own session says what it runs; anything else is what is picked for it.
    let session = bot.filter(|_| host.is_some_and(|h| h.model_options == f.model_options)).and_then(|b| {
        (b.agent_step.as_deref() == Some(f.backend.as_str())).then(|| b.session_of(&f.backend)).flatten()
    });
    let reported = session.and_then(|s| {
        // The setting `effort` describes: the session's own effort level before the catalog's.
        let Effort::Row { setting: Some(o), .. } = effort(&settings, None, Some(s), false, false) else { return None };
        let saved = f.model_options.get(&o.id).map(String::as_str);
        match effort(&settings, saved, Some(s), false, false) {
            Effort::Row { caption: Some(c), .. } => Some(format!(" · effort: {c}")),
            _ => None,
        }
    });
    let shown = reported.or_else(|| {
        setting.map(|o| {
            let value = f.model_options.get(&o.id).map_or("default", |v| {
                o.values.iter().find(|(id, _)| id == v).map_or(v.as_str(), |(_, name)| name.as_str())
            });
            format!(" · effort ‹{value}›")
        })
    });
    // Without its agent's list, what is saved for it, as saved.
    let shown = shown.or_else(|| {
        let saved: Vec<&str> = f.model_options.values().map(String::as_str).collect();
        (!saved.is_empty()).then(|| format!(" · saved ‹{}›", saved.join(", ")))
    });
    let shown = shown.filter(|_| effort_level);
    let item = format!("{} {} · {model}{}", i + 1, app.agent_name(&f.backend), shown.unwrap_or_default());
    if bot.and_then(|b| b.agent_step.as_deref()) == Some(f.backend.as_str()) {
        format!("{item} (in use)")
    } else if let Some(note) = bot.and_then(|b| rest_note(b, &f.backend)) {
        format!("{item} ({note})")
    } else {
        item
    }
}

/// A resting agent's state: "usage resets 15:00" or "resting until 15:00".
pub(in crate::tui) fn rest_note(bot: &Bot, agent: &str) -> Option<String> {
    let rest = bot.rests(agent, now_ms())?;
    Some(if rest.reason == Reason::UsageLimit {
        format!("usage resets {}", when_future(rest.until))
    } else {
        format!("resting until {}", when_future(rest.until))
    })
}

/// The Details page's line while the bot runs on a fallback: "Using Codex until Claude Code resets 15:00".
pub(in crate::tui) fn using_line(app: &App, bot: &Bot) -> Option<String> {
    let agent = bot.agent_step.as_deref()?;
    let own = app.agent_name(&bot.backend);
    let using = app.agent_name(agent);
    Some(match bot.rests(&bot.backend, now_ms()) {
        Some(rest) => format!("Using {using} until {own} resets {}", when_future(rest.until)),
        None => format!("Using {using} while {own} can't answer"),
    })
}

/// The Details page's chain: "Fallbacks: 1 Codex · gpt-5.5 (in use) · 2 Gemini CLI · default".
pub(in crate::tui) fn chain_line(app: &App, bot: &Bot) -> Option<String> {
    let steps: Vec<String> = bot.fallbacks.iter().enumerate().map(|(i, f)| step_item(app, Some(bot), i, f)).collect();
    (!steps.is_empty()).then(|| format!("Fallbacks: {}", steps.join(" · ")))
}
