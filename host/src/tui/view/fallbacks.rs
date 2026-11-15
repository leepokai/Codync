//! How the terminal shows a bot's fallback chain (`agent::fallback`): the agent it's on, and each step's state.

use super::super::app::{App, Bot};
use super::time::now_ms;
use super::usage::when_future;
use crate::agent::fallback::Reason;
use crate::store::Fallback;

/// Fallback `i` of `bot`'s chain: "1 Codex · gpt-5.5 (in use)", "2 Claude Code (Work) · default (usage resets 15:00)".
pub(in crate::tui) fn step_item(app: &App, bot: Option<&Bot>, i: usize, f: &Fallback) -> String {
    let login_profile = f.login_profile.as_deref();
    let agent = app.agent_name(&f.backend);
    let agent =
        login_profile.map_or(agent.clone(), |id| format!("{agent} ({})", app.login_profile_name(&f.backend, id)));
    let item = format!("{} {agent} · {}", i + 1, f.model.as_deref().unwrap_or("default"));
    if bot.is_some_and(|b| {
        b.agent_step.as_deref() == Some(f.backend.as_str()) && b.step_login_profile.as_deref() == login_profile
    }) {
        format!("{item} (in use)")
    } else if let Some(note) = bot.and_then(|b| rest_note(b, &f.backend, login_profile)) {
        format!("{item} ({note})")
    } else {
        item
    }
}

/// A resting login's state: "usage resets 15:00" or "resting until 15:00".
pub(in crate::tui) fn rest_note(bot: &Bot, agent: &str, login_profile: Option<&str>) -> Option<String> {
    let rest = bot.rests(agent, login_profile, now_ms())?;
    Some(if rest.reason == Reason::UsageLimit {
        format!("usage resets {}", when_future(rest.until))
    } else {
        format!("resting until {}", when_future(rest.until))
    })
}

/// The Details page's line while the bot runs on a fallback: "Using Codex until Claude Code resets 15:00". A login
/// on a login profile goes by it too ("Claude Code (Work)"), and so does the computer's own when both are the same
/// agent.
pub(in crate::tui) fn using_line(app: &App, bot: &Bot) -> Option<String> {
    let step = bot.agent_step.as_deref()?;
    let same = step == bot.backend;
    let name = |agent: &str, login_profile: Option<&str>| {
        let on = login_profile
            .map(|id| app.login_profile_name(agent, id))
            .or_else(|| same.then(|| app.default_login_profile_name(agent)));
        on.map_or_else(|| app.agent_name(agent), |on| format!("{} ({on})", app.agent_name(agent)))
    };
    let (own, using) =
        (name(&bot.backend, bot.login_profile.as_deref()), name(step, bot.step_login_profile.as_deref()));
    Some(match bot.rests(&bot.backend, bot.login_profile.as_deref(), now_ms()) {
        Some(rest) => format!("Using {using} until {own} resets {}", when_future(rest.until)),
        None => format!("Using {using} while {own} can't answer"),
    })
}

/// The Details page's chain: "Fallbacks: 1 Codex · gpt-5.5 (in use) · 2 Gemini CLI · default".
pub(in crate::tui) fn chain_line(app: &App, bot: &Bot) -> Option<String> {
    let steps: Vec<String> = bot.fallbacks.iter().enumerate().map(|(i, f)| step_item(app, Some(bot), i, f)).collect();
    (!steps.is_empty()).then(|| format!("Fallbacks: {}", steps.join(" · ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::test_app;
    use serde_json::json;

    #[tokio::test]
    async fn the_line_tells_two_logins_of_one_agent_apart() {
        let mut app = test_app();
        app.backends = vec![
            json!({"id": "claude", "name": "Claude Code", "loginProfiles": [{"id": "a1", "name": "Work"}]}),
            json!({"id": "codex", "name": "Codex"}),
        ];
        let line = |step| {
            let bot = json!({"id": "b", "name": "B", "backend": "claude", "agentStep": step});
            using_line(&app, &Bot::parse(&bot).expect("a bot"))
        };
        assert_eq!(
            line(json!({"agent": "claude", "loginProfile": "a1"})).as_deref(),
            Some("Using Claude Code (Work) while Claude Code (Default login profile) can't answer")
        );
        assert_eq!(line(json!({"agent": "codex"})).as_deref(), Some("Using Codex while Claude Code can't answer"));
    }
}
