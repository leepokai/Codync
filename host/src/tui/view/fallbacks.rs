//! How the terminal shows a bot's fallback chain (`agent::fallback`): the agent it's on, and each step's state.

use std::borrow::Cow;

use super::super::app::{App, Bot};
use super::time::now_ms;
use super::usage::when_future;
use crate::agent::fallback::Reason;
use crate::store::Fallback;
use crate::tui::l10n::{fill, tr};

/// Fallback `i` of `bot`'s chain: "1 Codex · gpt-5.5 (in use)", "2 Claude Code (Work) · default (Usage resets 15:00)".
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

/// A resting login's state, in the interface language: "Usage resets 15:00" or "Resting until 15:00".
pub(in crate::tui) fn rest_note(bot: &Bot, agent: &str, login_profile: Option<&str>) -> Option<String> {
    let rest = bot.rests(agent, login_profile, now_ms())?;
    Some(fill(&tr(rest_phrase(rest.reason)), &[("time", &when_future(rest.until))]))
}

/// A resting login's phrase before it's translated.
pub(in crate::tui) fn rest_phrase(reason: Reason) -> &'static str {
    if reason == Reason::UsageLimit { "Usage resets {time}" } else { "Resting until {time}" }
}

/// The Settings page's line while the bot runs on a fallback, in the interface language: "Using Codex until Claude
/// Code resets at 15:00" on a usage limit, "Using Codex while Claude Code is signed out" on another rest. A login on a
/// login profile goes by it too ("Claude Code (Work)"), and so does the computer's own when both are the same agent.
/// The phrase is translated before the names go in, so a login profile named "Work while traveling" stays whole.
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
    let rest = bot.rests(&bot.backend, bot.login_profile.as_deref(), now_ms());
    let time = rest.map(|r| when_future(r.until)).unwrap_or_default();
    Some(fill(&tr(&using_phrase(rest.map(|r| r.reason))), &[("to", &using), ("own", &own), ("time", &time)]))
}

/// The Settings page line's phrase before it's translated, for why the bot's own login rests (none: it can't answer);
/// only a usage limit gives a reset time.
pub(in crate::tui) fn using_phrase(rest: Option<Reason>) -> Cow<'static, str> {
    match rest {
        Some(Reason::UsageLimit) => Cow::Borrowed("Using {to} until {own} resets at {time}"),
        rest => Cow::Owned(format!("Using {{to}} while {{own}} {}", rest.map_or("can't answer", Reason::phrase))),
    }
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

    #[tokio::test]
    async fn only_a_usage_limit_gives_the_line_a_reset_time() {
        let mut app = test_app();
        app.backends = vec![json!({"id": "claude", "name": "Claude Code"}), json!({"id": "codex", "name": "Codex"})];
        let line = |reason: &str| {
            let rest = json!({"agent": "claude", "until": now_ms() + 3_600_000, "reason": reason});
            let bot = json!({"id": "b", "name": "B", "backend": "claude", "agentStep": {"agent": "codex"}, "resting": [rest]});
            using_line(&app, &Bot::parse(&bot).expect("a bot")).expect("on a fallback")
        };
        assert!(line("usageLimit").starts_with("Using Codex until Claude Code resets at "));
        for reason in ["signedOut", "rateLimited", "outOfCredits", "overloaded", "notStarting"] {
            assert!(!line(reason).contains("resets"), "{reason}");
        }
        assert_eq!(line("signedOut"), "Using Codex while Claude Code is signed out");
    }
}
