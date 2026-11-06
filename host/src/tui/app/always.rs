//! "Always allow" on an approval card, in the terminal: the host proposes the rule it would keep (`proposeRule`), the
//! person sees and may edit its pattern, and only then is the card answered, with that rule.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::{After, App, Editor, Overlay};

pub struct AlwaysSheet {
    pub entry: String,
    /// The `allow_always` option being answered.
    pub option: String,
    pub editor: Editor,
    /// The proposed rule's tool kind.
    pub kind: Option<String>,
    /// Why there is no rule, when the host gave none.
    pub refusal: Option<String>,
    /// The card's "allow once" option, to fall back on.
    pub once: Option<String>,
    pub error: Option<String>,
}

impl App {
    /// Whether `option` of card `entry` is its "always allow".
    fn is_always(&self, entry: &str, option: &str) -> bool {
        self.entries
            .values()
            .flat_map(|items| items.values())
            .find(|item| item.id == entry)
            .is_some_and(|item| item.options().iter().any(|(id, _, kind)| id == option && kind == "allow_always"))
    }

    /// Answers `entry` with `option`; "always allow" asks the host for its rule first.
    pub(super) fn answer(&mut self, entry: &str, option: &str) {
        if self.is_always(entry, option) && !self.answering.contains_key(entry) {
            self.call("proposeRule", json!({"entryId": entry}), After::Proposal(entry.to_owned(), option.to_owned()));
            return;
        }
        self.send_answer(entry, option, None);
    }

    /// The host's proposal for an "always allow" (an older host has none: the answer goes as it always did).
    pub(super) fn always_proposed(&mut self, entry: String, option: String, reply: Result<Value, String>) {
        let Ok(v) = reply else {
            self.send_answer(&entry, &option, None);
            return;
        };
        let once =
            self.entries.values().flat_map(|items| items.values()).find(|item| item.id == entry).and_then(|item| {
                item.options().into_iter().find(|(_, _, kind)| kind == "allow_once").map(|(id, _, _)| id)
            });
        let rule = v["rule"].as_object();
        let sheet = AlwaysSheet {
            entry,
            option,
            editor: Editor::with(rule.and_then(|r| r["pattern"].as_str()).unwrap_or_default()),
            kind: rule.and_then(|r| r["kind"].as_str()).map(str::to_owned),
            refusal: rule
                .is_none()
                .then(|| v["reason"].as_str().unwrap_or("No rule can be kept for this request.").to_owned()),
            once,
            error: None,
        };
        self.overlays.push(Overlay::Always(Box::new(sheet)));
    }

    pub(super) fn always_key(&mut self, mut s: Box<AlwaysSheet>, k: KeyEvent) -> Option<Overlay> {
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Enter if s.refusal.is_none() && !s.editor.text.trim().is_empty() => {
                let rule = json!({"pattern": s.editor.text.trim(), "kind": s.kind, "decision": "allow"});
                self.send_answer(&s.entry, &s.option, Some(rule));
                return None;
            }
            // Allow just this once, without a rule: always possible, and the only way out of a refusal.
            KeyCode::Char('o') if s.refusal.is_some() || s.editor.text.is_empty() => {
                if let Some(once) = s.once.clone() {
                    self.send_answer(&s.entry, &once, None);
                    return None;
                }
                s.error = Some("This request has no \"allow once\" answer.".into());
            }
            _ if s.refusal.is_none() => {
                s.editor.key(k);
            }
            _ => {}
        }
        Some(Overlay::Always(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Bot, Entry, Kind, test_app};

    fn card(app: &mut App) {
        let data = json!({"title": "Run git", "command": "git status", "toolKind": "execute", "status": "pending", "options": [
            {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
            {"optionId": "always", "name": "Always", "kind": "allow_always"},
            {"optionId": "no", "name": "Reject", "kind": "reject_once"}]});
        let entry = Entry { id: "c1".into(), seq: 1, turn: 1, kind: Kind::Permission, data, created_at: 0 };
        app.entries.entry("b".into()).or_default().insert(1, entry);
    }

    #[tokio::test]
    async fn only_the_always_option_asks_for_a_rule_first() {
        let mut app = test_app();
        app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "Bot"})).expect("a bot"));
        card(&mut app);
        assert!(app.is_always("c1", "always"));
        assert!(!app.is_always("c1", "yes") && !app.is_always("c1", "no"));
    }

    #[tokio::test]
    async fn the_proposal_opens_a_sheet_to_edit_and_a_refusal_offers_allow_once() {
        let mut app = test_app();
        app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "Bot"})).expect("a bot"));
        card(&mut app);
        app.always_proposed(
            "c1".into(),
            "always".into(),
            Ok(json!({"rule": {"pattern": "git status", "kind": "execute", "decision": "allow"}})),
        );
        let Some(Overlay::Always(sheet)) = app.overlays.last() else { panic!("the sheet opens") };
        assert_eq!(
            (sheet.editor.text.as_str(), sheet.kind.as_deref(), sheet.refusal.is_none()),
            ("git status", Some("execute"), true)
        );
        assert_eq!(sheet.once.as_deref(), Some("yes"));
        app.overlays.clear();
        app.always_proposed(
            "c1".into(),
            "always".into(),
            Ok(json!({"rule": null, "reason": "It runs several commands."})),
        );
        let Some(Overlay::Always(sheet)) = app.overlays.last() else { panic!("the sheet opens") };
        assert_eq!(sheet.refusal.as_deref(), Some("It runs several commands."));
    }
}
