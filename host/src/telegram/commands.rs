//! `/estado`, `/rutinas` and `/detener` (also `/status`, `/routines`, `/stop`): answered by the host at once, with
//! no model turn and so no tokens spent.

use crate::hub::{BotStatus, Hub};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Command {
    Status,
    Routines,
    Stop,
}

/// The command a message is, if it is one of ours. `/estado@my_bot` counts; a command to another bot does not.
pub(super) fn parse(text: &str, username: Option<&str>) -> Option<Command> {
    let first = text.split_whitespace().next()?.strip_prefix('/')?;
    let (word, target) = first.split_once('@').map_or((first, None), |(w, t)| (w, Some(t)));
    if target.is_some_and(|t| username.is_none_or(|u| !u.eq_ignore_ascii_case(t))) {
        return None;
    }
    match word.to_ascii_lowercase().as_str() {
        "estado" | "status" => Some(Command::Status),
        "rutinas" | "routines" => Some(Command::Routines),
        "detener" | "stop" => Some(Command::Stop),
        _ => None,
    }
}

pub(super) fn run(hub: &Arc<Hub>, bot: &str, command: &Command) -> String {
    let name = hub.bot_name(bot);
    match command {
        Command::Status => {
            let runtime = hub.runtime(bot);
            let state = match runtime.status {
                BotStatus::Idle => "idle",
                BotStatus::Working => "working",
                BotStatus::NeedsInput => "waiting for you",
                BotStatus::Error => "stopped by an error",
            };
            if runtime.activity.is_empty() {
                format!("{name}: {state}")
            } else {
                format!("{name}: {state} · {}", runtime.activity)
            }
        }
        Command::Routines => {
            let list = hub.routines.list(bot);
            let lines: Vec<String> = list["routines"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    format!(
                        "{} {}",
                        if r["enabled"] == true { "●" } else { "○" },
                        r["name"].as_str().unwrap_or("Routine")
                    )
                })
                .collect();
            if lines.is_empty() {
                format!("{name} has no routines.")
            } else {
                format!("{name}'s routines:\n{}", lines.join("\n"))
            }
        }
        Command::Stop => match hub.stop_bot(bot) {
            Ok(()) => format!("Stopping {name}."),
            Err(error) => format!("Could not stop {name}: {error:#}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_ours_in_either_language_and_only_when_aimed_at_us() {
        assert_eq!(parse("/estado", None), Some(Command::Status));
        assert_eq!(parse("/STATUS now", None), Some(Command::Status));
        assert_eq!(parse("/rutinas@home_bot", Some("Home_Bot")), Some(Command::Routines));
        assert_eq!(parse("/detener", None), Some(Command::Stop));
        assert_eq!(parse("/estado@other_bot", Some("home_bot")), None);
        assert_eq!(parse("/estado@home_bot", None), None);
        assert_eq!(parse("/deploy", None), None);
        assert_eq!(parse("estado", None), None, "a command starts with a slash");
        assert_eq!(parse("", None), None);
    }
}
