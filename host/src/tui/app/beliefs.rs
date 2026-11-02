//! "What I know about you" in the terminal: the selected bot's notes as beliefs, the least sure first, with how sure it is
//! and when it was last confirmed. `y` confirms, `x` forgets, `f` and `c` filter by kind and confidence, `g` shows what was
//! forgotten, Enter opens a note's history and `b` brings an earlier version back. The host owns the meaning.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::{After, App, Overlay};

pub const KINDS: [(&str, &str); 4] =
    [("user", "About you"), ("feedback", "How to work"), ("project", "Projects"), ("reference", "References")];
pub const CONFIDENCES: [(&str, &str); 3] = [("high", "Sure"), ("medium", "Probably"), ("low", "Unsure")];

#[derive(Default)]
pub struct BeliefsSheet {
    pub bot_id: String,
    pub bot_name: String,
    /// `None` until the host answered.
    pub items: Option<Vec<Value>>,
    pub sel: usize,
    pub kind: Option<usize>,
    pub confidence: Option<usize>,
    pub forgotten: bool,
    /// A note's versions are open, with this one picked.
    pub history: Option<usize>,
    pub error: Option<String>,
}

pub fn kind_label(kind: &str) -> &'static str {
    KINDS.iter().find(|(id, _)| *id == kind).map_or("About you", |(_, label)| label)
}

pub fn confidence_label(confidence: &str) -> &'static str {
    CONFIDENCES.iter().find(|(id, _)| *id == confidence).map_or("Probably", |(_, label)| label)
}

pub fn confirmed_text(confirmed: Option<&str>) -> String {
    confirmed.map_or_else(|| "Never confirmed".to_owned(), |date| format!("Confirmed {date}"))
}

fn rank(confidence: &str) -> u8 {
    match confidence {
        "low" => 0,
        "high" => 2,
        _ => 1,
    }
}

impl BeliefsSheet {
    /// The notes that pass the filters, the unsure first and then the longest unconfirmed.
    pub fn shown(&self) -> Vec<&Value> {
        let mut shown: Vec<&Value> = self
            .items
            .iter()
            .flatten()
            .filter(|b| self.kind.is_none_or(|k| b["kind"] == KINDS[k].0))
            .filter(|b| self.confidence.is_none_or(|c| b["confidence"] == CONFIDENCES[c].0))
            .collect();
        shown.sort_by(|a, b| {
            rank(a["confidence"].as_str().unwrap_or_default())
                .cmp(&rank(b["confidence"].as_str().unwrap_or_default()))
                .then_with(|| a["confirmed"].as_str().unwrap_or_default().cmp(b["confirmed"].as_str().unwrap_or_default()))
                .then_with(|| a["file"].as_str().unwrap_or_default().cmp(b["file"].as_str().unwrap_or_default()))
        });
        shown
    }

    /// A note as the list shows it, on two lines.
    pub fn lines(belief: &Value) -> [String; 2] {
        let shared = if belief["shared"] == true { " · Shared" } else { "" };
        [
            belief["description"].as_str().or(belief["file"].as_str()).unwrap_or_default().to_owned(),
            format!(
                "{} · {} · {}{shared}",
                kind_label(belief["kind"].as_str().unwrap_or_default()),
                confidence_label(belief["confidence"].as_str().unwrap_or_default()),
                confirmed_text(belief["confirmed"].as_str())
            ),
        ]
    }

    /// The versions of the selected note, newest first, with whether each can be brought back (not the newest, not a forgetting).
    pub fn versions(belief: &Value) -> Vec<(i64, &'static str, bool)> {
        belief["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, v)| {
                let label = match v["cause"].as_str().unwrap_or("edit") {
                    "delete" => "Forgotten",
                    "restore" => "Brought back",
                    "undo" => "Undone",
                    "dream" => "Tidied while dreaming",
                    _ => "Changed",
                };
                (v["id"].as_i64().unwrap_or(0), label, !v["content"].is_null() && i > 0)
            })
            .collect()
    }
}

fn cycle(at: Option<usize>, len: usize) -> Option<usize> {
    match at {
        None => Some(0),
        Some(i) if i + 1 < len => Some(i + 1),
        Some(_) => None,
    }
}

impl App {
    pub(super) fn open_beliefs(&mut self) {
        let Some(bot) = self.bot().filter(|b| !b.group) else {
            self.flash("Pick a bot first");
            return;
        };
        let sheet = BeliefsSheet { bot_id: bot.id.clone(), bot_name: bot.name.clone(), ..BeliefsSheet::default() };
        self.call("beliefs", json!({"botId": sheet.bot_id, "forgotten": false}), After::Beliefs);
        self.overlays.push(Overlay::Beliefs(Box::new(sheet)));
    }

    fn reload_beliefs(&mut self, s: &BeliefsSheet) {
        self.call("beliefs", json!({"botId": s.bot_id, "forgotten": s.forgotten}), After::Beliefs);
    }

    pub(super) fn beliefs_key(&mut self, mut s: Box<BeliefsSheet>, k: KeyEvent) -> Option<Overlay> {
        let selected = s.shown().get(s.sel).map(|b| (*b).clone());
        if let (Some(at), Some(belief)) = (s.history, &selected) {
            let versions = BeliefsSheet::versions(belief);
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => s.history = None,
                KeyCode::Down | KeyCode::Char('j') => s.history = Some((at + 1).min(versions.len().saturating_sub(1))),
                KeyCode::Up | KeyCode::Char('k') => s.history = Some(at.saturating_sub(1)),
                KeyCode::Char('b') => {
                    if let Some((id, _, true)) = versions.get(at) {
                        let body = json!({"botId": s.bot_id, "file": belief["file"], "shared": belief["shared"], "action": "restore", "versionId": id});
                        self.call("beliefAction", body, After::BeliefsChanged);
                        s.history = None;
                    }
                }
                _ => {}
            }
            return Some(Overlay::Beliefs(s));
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => s.sel = (s.sel + 1).min(s.shown().len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => s.sel = s.sel.saturating_sub(1),
            KeyCode::Char('f') => {
                s.kind = cycle(s.kind, KINDS.len());
                s.sel = 0;
            }
            KeyCode::Char('c') => {
                s.confidence = cycle(s.confidence, CONFIDENCES.len());
                s.sel = 0;
            }
            KeyCode::Char('g') => {
                s.forgotten = !s.forgotten;
                s.items = None;
                s.sel = 0;
                self.reload_beliefs(&s);
            }
            KeyCode::Enter => {
                if selected.is_some() {
                    s.history = Some(0);
                }
            }
            KeyCode::Char(key @ ('y' | 'x')) if !s.forgotten => {
                if let Some(belief) = &selected {
                    let action = if key == 'y' { "confirm" } else { "forget" };
                    let body = json!({"botId": s.bot_id, "file": belief["file"], "shared": belief["shared"], "action": action});
                    self.call("beliefAction", body, After::BeliefsChanged);
                }
            }
            _ => {}
        }
        Some(Overlay::Beliefs(s))
    }

    /// The host's list of notes.
    pub(super) fn beliefs_loaded(&mut self, v: &Value) {
        if let Some(Overlay::Beliefs(s)) = self.overlays.last_mut() {
            s.items = Some(v["beliefs"].as_array().cloned().unwrap_or_default());
            s.error = None;
            s.sel = s.sel.min(s.shown().len().saturating_sub(1));
        }
    }

    /// A confirm, forget or restore went through: read the list again.
    pub(super) fn beliefs_changed(&mut self) {
        if let Some(Overlay::Beliefs(s)) = self.overlays.last() {
            let body = json!({"botId": s.bot_id, "forgotten": s.forgotten});
            self.call("beliefs", body, After::Beliefs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belief(file: &str, kind: &str, confidence: &str, confirmed: Option<&str>) -> Value {
        json!({"file": file, "kind": kind, "confidence": confidence, "confirmed": confirmed, "description": file, "versions": []})
    }

    #[test]
    fn the_unsure_come_first_and_filters_narrow_the_list() {
        let sheet = BeliefsSheet {
            items: Some(vec![
                belief("sure.md", "user", "high", Some("2026-10-01")),
                belief("old.md", "user", "medium", Some("2026-01-01")),
                belief("doubt.md", "feedback", "low", Some("2026-10-30")),
            ]),
            ..BeliefsSheet::default()
        };
        let files = |s: &BeliefsSheet| s.shown().iter().map(|b| b["file"].as_str().unwrap_or_default().to_owned()).collect::<Vec<_>>();
        assert_eq!(files(&sheet), ["doubt.md", "old.md", "sure.md"]);
        let feedback = BeliefsSheet { kind: Some(1), items: sheet.items.clone(), ..BeliefsSheet::default() };
        assert_eq!(files(&feedback), ["doubt.md"]);
        let high = BeliefsSheet { confidence: Some(0), items: sheet.items, ..BeliefsSheet::default() };
        assert_eq!(files(&high), ["sure.md"]);
    }

    #[test]
    fn a_note_is_told_in_two_lines_like_the_other_clients() {
        let b = json!({"file": "home.md", "description": "Where the user lives", "kind": "project", "confidence": "low", "confirmed": null, "shared": true});
        assert_eq!(
            BeliefsSheet::lines(&b),
            ["Where the user lives".to_owned(), "Projects · Unsure · Never confirmed · Shared".to_owned()]
        );
        assert_eq!(confirmed_text(Some("2026-10-30")), "Confirmed 2026-10-30");
    }

    #[test]
    fn only_an_older_version_that_is_not_a_forgetting_can_be_brought_back() {
        let b = json!({"versions": [
            {"id": 3, "content": null, "cause": "delete"}, {"id": 2, "content": "x", "cause": "edit"}, {"id": 1, "content": "y", "cause": "dream"}]});
        assert_eq!(
            BeliefsSheet::versions(&b),
            [(3, "Forgotten", false), (2, "Changed", true), (1, "Tidied while dreaming", true)]
        );
    }

    #[test]
    fn a_filter_cycles_through_its_values_and_back_to_all() {
        assert_eq!(cycle(None, 3), Some(0));
        assert_eq!(cycle(Some(1), 3), Some(2));
        assert_eq!(cycle(Some(2), 3), None);
    }
}
