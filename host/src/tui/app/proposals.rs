//! Proposed skills in the terminal: the skills the selected bot drafted after a procedure that used several kinds of tool.
//! `a` approves one (it becomes a skill of the bot), `x` dismisses it for good. Editing a draft is done in the desktop app
//! or on the phone; the terminal has no multi-line editor for it.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::{After, App, Overlay};

#[derive(Default)]
pub struct ProposalsSheet {
    pub bot_id: String,
    pub bot_name: String,
    /// `None` until the host answered.
    pub items: Option<Vec<Value>>,
    pub sel: usize,
}

pub fn inbox_title(count: usize) -> String {
    if count > 0 { format!("Proposed skills ({count})") } else { "Proposed skills".to_owned() }
}

/// `execute:cargo` reads as the program.
pub fn tools_text(proposal: &Value) -> String {
    proposal["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|t| t.strip_prefix("execute:").map_or(t, |p| if p.is_empty() { "commands" } else { p }))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first numbered steps of the instructions.
pub fn steps_preview(proposal: &Value, max: usize) -> Vec<String> {
    proposal["instructions"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter(|l| l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
        .take(max)
        .map(str::to_owned)
        .collect()
}

impl App {
    pub(super) fn open_proposals(&mut self) {
        let Some(bot) = self.bot().filter(|b| !b.group) else {
            self.flash("Pick a bot first");
            return;
        };
        let sheet = ProposalsSheet { bot_id: bot.id.clone(), bot_name: bot.name.clone(), ..ProposalsSheet::default() };
        self.call("skillProposals", json!({"botId": sheet.bot_id}), After::Proposals);
        self.overlays.push(Overlay::Proposals(Box::new(sheet)));
    }

    pub(super) fn proposals_key(&mut self, mut s: Box<ProposalsSheet>, k: KeyEvent) -> Option<Overlay> {
        let count = s.items.as_ref().map_or(0, Vec::len);
        let picked =
            s.items.as_ref().and_then(|items| items.get(s.sel)).and_then(|p| p["id"].as_str()).map(str::to_owned);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => s.sel = (s.sel + 1).min(count.saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => s.sel = s.sel.saturating_sub(1),
            KeyCode::Char('r') => self.call("skillProposals", json!({"botId": s.bot_id}), After::Proposals),
            KeyCode::Char('a') => {
                if let Some(id) = picked {
                    self.call("approveSkillProposal", json!({"botId": s.bot_id, "id": id}), After::Proposals);
                }
            }
            KeyCode::Char('x') => {
                if let Some(id) = picked {
                    self.call("dismissSkillProposal", json!({"botId": s.bot_id, "id": id}), After::Proposals);
                }
            }
            _ => {}
        }
        Some(Overlay::Proposals(s))
    }

    /// Every inbox call answers with the list as it is now.
    pub(super) fn proposals_loaded(&mut self, v: &Value) {
        if let Some(Overlay::Proposals(s)) = self.overlays.last_mut() {
            let items = v["proposals"].as_array().cloned().unwrap_or_default();
            s.sel = s.sel.min(items.len().saturating_sub(1));
            s.items = Some(items);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inbox_title_counts_what_waits() {
        assert_eq!(inbox_title(0), "Proposed skills");
        assert_eq!(inbox_title(2), "Proposed skills (2)");
    }

    #[test]
    fn tools_read_as_words_and_steps_as_the_first_numbered_lines() {
        let p = json!({"tools": ["edit", "execute:cargo", "execute:"], "instructions": "Use this when: x\n\n1. Test\n2. Tag\n3. Push\n4. Announce\n"});
        assert_eq!(tools_text(&p), "edit, cargo, commands");
        assert_eq!(steps_preview(&p, 3), ["1. Test", "2. Tag", "3. Push"]);
        assert_eq!(steps_preview(&json!({"instructions": "no steps"}), 3), [] as [String; 0]);
    }
}
