//! Messaging channels in the terminal: the Telegram bots of this host (state, last message, who is allowed, which bot
//! answers) and the Slack connection, with `p` to make a pairing code. Adding a Telegram bot takes its token, which
//! the desktop app asks for; the host owns everything else.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::{After, App, Overlay};

#[derive(Default)]
pub struct ChannelsSheet {
    pub bots: Vec<Value>,
    pub slack: Option<Value>,
    pub sel: usize,
    /// The Telegram bot a pairing code was just asked for.
    pub pairing_for: Option<String>,
    /// The code made, with the bot's name: shown once.
    pub code: Option<(String, String)>,
}

pub fn state_text(state: &str) -> &'static str {
    match state {
        "ok" => "Connected",
        "starting" => "Starting…",
        _ => "Cannot reach Telegram",
    }
}

pub fn last_received_text(at: Option<i64>, now: i64) -> String {
    let Some(at) = at else { return "Nothing received yet".into() };
    let minutes = ((now - at) / 60_000).max(0);
    match minutes {
        0 => "Last message just now".into(),
        1..=59 => format!("Last message {minutes} min ago"),
        60..=1439 => format!("Last message {} h ago", minutes / 60),
        _ => format!("Last message {} d ago", minutes / 1440),
    }
}

pub fn pairing_instruction(code: &str, username: &str) -> String {
    let who = if username.is_empty() { "your Telegram bot".to_owned() } else { format!("@{username}") };
    format!("Send /start {code} to {who} in Telegram. The code works once, for 10 minutes.")
}

pub fn allowed_text(count: usize) -> String {
    if count == 1 { "1 person allowed".into() } else { format!("{count} people allowed") }
}

/// What the sheet says about the Slack connection.
pub fn slack_text(slack: Option<&Value>) -> String {
    match slack {
        None => "Loading…".into(),
        Some(s) if s["configured"] != true => "Slack is not connected.".into(),
        Some(s) => {
            let how = if s["socketMode"] == true { "Socket Mode" } else { "the HTTP endpoint" };
            format!("Connected through {how} · {}", s["workspaceId"].as_str().unwrap_or_default())
        }
    }
}

/// One Telegram bot as the list shows it, on two lines.
pub fn bot_lines(bot: &Value, now: i64) -> [String; 2] {
    let username = bot["username"].as_str().unwrap_or_default();
    let handle = if username.is_empty() { String::new() } else { format!(" · @{username}") };
    let first = format!(
        "{}{handle} → {}",
        bot["name"].as_str().unwrap_or_default(),
        bot["botName"].as_str().unwrap_or_default()
    );
    let second = format!(
        "{} · {} · {}",
        state_text(bot["state"].as_str().unwrap_or("starting")),
        last_received_text(bot["lastReceivedAt"].as_i64(), now),
        allowed_text(bot["allowed"].as_array().map_or(0, Vec::len))
    );
    [first, second]
}

impl App {
    pub(super) fn open_channels(&mut self) {
        self.overlays.push(Overlay::Channels(Box::default()));
        self.call("telegramStatus", json!({}), After::Channels);
        self.call("slackStatus", json!({}), After::ChannelsSlack);
    }

    pub(super) fn channels_key(&mut self, mut s: Box<ChannelsSheet>, k: KeyEvent) -> Option<Overlay> {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => s.sel = (s.sel + 1).min(s.bots.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => s.sel = s.sel.saturating_sub(1),
            KeyCode::Char('r') => {
                self.call("telegramStatus", json!({}), After::Channels);
                self.call("slackStatus", json!({}), After::ChannelsSlack);
            }
            KeyCode::Char('p') => {
                if let Some(name) = s.bots.get(s.sel).and_then(|b| b["name"].as_str()).map(str::to_owned) {
                    self.call("pairTelegram", json!({"name": name}), After::Channels);
                    s.pairing_for = Some(name);
                }
            }
            _ => {}
        }
        Some(Overlay::Channels(s))
    }

    /// The host's Telegram status, or the answer to a pairing request (which also carries the code).
    pub(super) fn channels_loaded(&mut self, v: &Value) {
        if let Some(Overlay::Channels(s)) = self.overlays.last_mut() {
            s.bots = v["bots"].as_array().cloned().unwrap_or_default();
            s.sel = s.sel.min(s.bots.len().saturating_sub(1));
            if let (Some(code), Some(name)) = (v["result"]["code"].as_str(), s.pairing_for.take()) {
                s.code = Some((name, code.to_owned()));
            }
        }
    }

    pub(super) fn channels_slack_loaded(&mut self, v: Value) {
        if let Some(Overlay::Channels(s)) = self.overlays.last_mut() {
            s.slack = Some(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bot_is_told_in_two_lines() {
        let bot = json!({"name": "home", "username": "home_bot", "botName": "Agent", "state": "ok",
            "lastReceivedAt": 1_000_000, "allowed": [42]});
        assert_eq!(
            bot_lines(&bot, 1_000_000 + 5 * 60_000),
            ["home · @home_bot → Agent".to_owned(), "Connected · Last message 5 min ago · 1 person allowed".to_owned()]
        );
        let fresh = json!({"name": "work", "botName": "Runner", "state": "error: x", "allowed": []});
        assert_eq!(bot_lines(&fresh, 0)[1], "Cannot reach Telegram · Nothing received yet · 0 people allowed");
    }

    #[test]
    fn words_follow_the_other_clients() {
        let now = 10 * 24 * 3_600_000;
        assert_eq!(last_received_text(Some(now - 20_000), now), "Last message just now");
        assert_eq!(last_received_text(Some(now - 3 * 3_600_000), now), "Last message 3 h ago");
        assert_eq!(last_received_text(Some(now - 2 * 24 * 3_600_000), now), "Last message 2 d ago");
        assert_eq!(last_received_text(Some(now + 5000), now), "Last message just now");
        assert_eq!(
            pairing_instruction("ABCD2345", "home_bot"),
            "Send /start ABCD2345 to @home_bot in Telegram. The code works once, for 10 minutes."
        );
        assert!(pairing_instruction("X", "").contains("your Telegram bot"));
        assert_eq!(slack_text(None), "Loading…");
        assert_eq!(slack_text(Some(&json!({"configured": false}))), "Slack is not connected.");
        assert_eq!(
            slack_text(Some(&json!({"configured": true, "socketMode": true, "workspaceId": "T1"}))),
            "Connected through Socket Mode · T1"
        );
    }
}
