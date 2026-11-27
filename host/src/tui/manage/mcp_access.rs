//! The MCP access sheet: the outside apps that may drive this computer's threads, and the computer's own levels
//! (`mcpClients`; docs/features/mcp-access.md). ←/→ changes the selected row's level, n gives a new app access (its
//! token shown once, y copies the command with it), r renews an app, x revokes it after a second x.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{After, App, Editor, Overlay, copy};
use crate::chat::team::control::Level;

/// The computer's own levels, in the order the sheet lists them before the apps: the host's key, its name, what it is.
pub const OWN: [(&str, &str, &str); 3] = [
    ("max", "Most an app may do", "An app given more acts as if given this."),
    ("local", "Sessions on this computer", "Coding agents here without a token. Any program of yours can act as one."),
    (
        "cloud",
        "Cloud assistants",
        "ChatGPT or Claude through your Codync account. Codync's cloud can read these calls.",
    ),
];

#[derive(Default)]
pub enum McpStep {
    #[default]
    List,
    /// Naming a new app.
    Name(Editor),
    /// The command with a new app's token, shown this once.
    Token(String),
    /// Asked once to revoke this app.
    Revoke(String),
}

#[derive(Default)]
pub struct McpSheet {
    /// `mcpClients`' answer; none until it came, or from a host before MCP access.
    pub data: Option<Value>,
    pub cursor: usize,
    pub step: McpStep,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum McpReply {
    View,
    Created,
    Changed,
}

impl McpSheet {
    /// The apps still let in, as the host lists them.
    pub fn apps(&self) -> Vec<&Value> {
        let all = self.data.as_ref().and_then(|d| d["clients"].as_array());
        all.into_iter().flatten().filter(|c| c["revokedAt"].is_null()).collect()
    }

    fn rows(&self) -> usize {
        OWN.len() + self.apps().len()
    }
}

/// The level after (`+1`) or before (`-1`) `now` (as the host spells it), staying within the ladder.
pub fn step_level(now: &str, by: i8) -> Level {
    let at = Level::parse(now).and_then(|l| Level::ALL.iter().position(|x| *x == l)).unwrap_or(0);
    let next = if by > 0 { (at + 1).min(Level::ALL.len() - 1) } else { at.saturating_sub(1) };
    Level::ALL[next]
}

/// Whole days an app's access has left, rounded up as the desktop app does; none once it expired.
pub fn days_left(expires_at: i64, now: i64) -> Option<i64> {
    let day = 86_400_000;
    let left = expires_at - now;
    (left > 0).then(|| (left + day - 1) / day)
}

impl App {
    pub fn open_mcp_access(&mut self) {
        self.call("mcpClients", json!({}), After::Mcp(McpReply::View));
        self.overlays.push(Overlay::Mcp(Box::default()));
    }

    pub(in crate::tui) fn mcp_reply(&mut self, r: McpReply, res: Result<Value, String>) {
        let Some(Overlay::Mcp(s)) = self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Mcp(_))) else {
            return;
        };
        match (r, res) {
            (_, Err(e)) => s.error = Some(e),
            (McpReply::View, Ok(v)) => {
                s.error = None;
                s.data = Some(v);
                s.cursor = s.cursor.min(s.rows().saturating_sub(1));
            }
            (McpReply::Created, Ok(v)) => {
                s.step = McpStep::Token(v["command"].as_str().unwrap_or_default().to_owned());
                self.call("mcpClients", json!({}), After::Mcp(McpReply::View));
            }
            (McpReply::Changed, Ok(_)) => self.call("mcpClients", json!({}), After::Mcp(McpReply::View)),
        }
    }

    pub(in crate::tui) fn mcp_key(&mut self, mut s: Box<McpSheet>, k: KeyEvent) -> Option<Overlay> {
        let changed = After::Mcp(McpReply::Changed);
        s.step = match std::mem::take(&mut s.step) {
            McpStep::Name(mut name) => match k.code {
                KeyCode::Esc => McpStep::List,
                KeyCode::Enter if !name.text.trim().is_empty() => {
                    // What a new app may do is the host's default.
                    self.call("mcpClientCreate", json!({"label": name.text.trim()}), After::Mcp(McpReply::Created));
                    McpStep::List
                }
                _ => {
                    name.key(k);
                    McpStep::Name(name)
                }
            },
            McpStep::Token(command) => match k.code {
                KeyCode::Char('y') => {
                    copy(&command);
                    self.flash("Copied: it isn't shown again");
                    McpStep::Token(command)
                }
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => McpStep::List,
                _ => McpStep::Token(command),
            },
            McpStep::Revoke(id) => {
                if k.code == KeyCode::Char('x') {
                    self.call("mcpClientRevoke", json!({"id": id}), changed);
                }
                McpStep::List
            }
            McpStep::List => {
                let apps: Vec<Value> = s.apps().into_iter().cloned().collect();
                let app = s.cursor.checked_sub(OWN.len()).and_then(|i| apps.get(i));
                match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => return None,
                    KeyCode::Up | KeyCode::Char('k') => s.cursor = s.cursor.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => s.cursor = (s.cursor + 1).min(s.rows().saturating_sub(1)),
                    KeyCode::Char('n') => {
                        return Some(Overlay::Mcp(Box::new(McpSheet { step: McpStep::Name(Editor::default()), ..*s })));
                    }
                    KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l') => {
                        let by = if matches!(k.code, KeyCode::Right | KeyCode::Char('l')) { 1 } else { -1 };
                        match (app, OWN.get(s.cursor).map(|(key, ..)| key)) {
                            (Some(app), _) => {
                                let level = step_level(app["access"].as_str().unwrap_or_default(), by);
                                self.call("mcpClientUpdate", json!({"id": app["id"], "access": level}), changed);
                            }
                            (None, Some(own)) => {
                                let data = s.data.as_ref().map(|d| d["levels"][own].clone()).unwrap_or_default();
                                let level = step_level(data.as_str().unwrap_or_default(), by);
                                self.call("setMcpAccess", json!({ *own: level }), changed);
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(app) = app {
                            self.call("mcpClientUpdate", json!({"id": app["id"], "renew": true}), changed);
                        }
                    }
                    KeyCode::Char('x') => {
                        if let Some(id) = app.and_then(|a| a["id"].as_str()) {
                            return Some(Overlay::Mcp(Box::new(McpSheet {
                                step: McpStep::Revoke(id.to_owned()),
                                ..*s
                            })));
                        }
                    }
                    _ => {}
                }
                McpStep::List
            }
        };
        Some(Overlay::Mcp(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_steps_along_the_ladder_and_stops_at_its_ends_and_days_round_up() {
        assert_eq!(step_level("ask", 1), Level::Auto);
        assert_eq!(step_level("full", 1), Level::Full);
        assert_eq!(step_level("readOnly", -1), Level::ReadOnly);
        assert_eq!(step_level("unknown", 1), Level::Ask, "an unknown level starts from the least");
        let day = 86_400_000;
        assert_eq!(
            (days_left(10 + day / 2, 10), days_left(10 + 3 * day, 10), days_left(10, 10)),
            (Some(1), Some(3), None)
        );
        let sheet = McpSheet {
            data: Some(json!({"clients": [{"id": "a", "revokedAt": null}, {"id": "b", "revokedAt": 5}]})),
            ..McpSheet::default()
        };
        assert_eq!(sheet.apps().len(), 1, "a revoked app is gone from the list");
        assert_eq!(sheet.rows(), 4);
    }
}
