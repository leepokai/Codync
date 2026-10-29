//! A read-only sheet of what one agent already brings: skills, connectors, rules and plugins.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Overlay};
use super::Reply;

/// One skill, connector, rule or plugin the agent keeps for itself.
#[derive(Clone, Debug)]
pub struct GearItem {
    pub name: String,
    pub detail: String,
}

/// The four lists, once `agentGear` has answered.
#[derive(Clone, Debug)]
pub struct GearLists {
    pub skills: Vec<GearItem>,
    pub connectors: Vec<GearItem>,
    pub rules: Vec<GearItem>,
    pub plugins: Vec<GearItem>,
}

/// What one agent brings, opened from its sign-in sheet.
pub struct Gear {
    pub backend: String,
    /// `None` until `agentGear` answers.
    pub lists: Option<GearLists>,
    pub failed: bool,
    pub cursor: usize,
}

pub enum GearRow {
    Heading(&'static str),
    Note,
    Item(usize, usize),
}

impl Gear {
    fn sections(&self) -> [(&'static str, &[GearItem]); 4] {
        let empty: &[GearItem] = &[];
        let lists = self.lists.as_ref();
        [
            ("Skills", lists.map_or(empty, |l| l.skills.as_slice())),
            ("Connectors", lists.map_or(empty, |l| l.connectors.as_slice())),
            ("Rules", lists.map_or(empty, |l| l.rules.as_slice())),
            ("Plugins", lists.map_or(empty, |l| l.plugins.as_slice())),
        ]
    }

    pub fn rows(&self) -> Vec<GearRow> {
        if self.lists.is_none() || self.failed {
            return vec![GearRow::Note];
        }
        let mut rows = Vec::new();
        for (section, (label, items)) in self.sections().into_iter().enumerate() {
            if items.is_empty() {
                continue;
            }
            rows.push(GearRow::Heading(label));
            rows.extend((0..items.len()).map(|index| GearRow::Item(section, index)));
        }
        if rows.is_empty() { vec![GearRow::Note] } else { rows }
    }

    pub fn count_of(&self, label: &str) -> usize {
        let Some(lists) = &self.lists else { return 0 };
        match label {
            "Skills" => lists.skills.len(),
            "Connectors" => lists.connectors.len(),
            "Rules" => lists.rules.len(),
            "Plugins" => lists.plugins.len(),
            _ => 0,
        }
    }

    pub fn item(&self, section: usize, index: usize) -> Option<&GearItem> {
        self.sections().get(section).and_then(|(_, items)| items.get(index))
    }
}

pub(super) fn items(value: &Value) -> Vec<GearItem> {
    value.as_array().map_or(Vec::new(), |rows| {
        rows.iter()
            .filter_map(|row| {
                let name = row["name"].as_str()?.trim();
                if name.is_empty() {
                    return None;
                }
                let detail = row["description"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| row["detail"].as_str())
                    .unwrap_or("")
                    .to_owned();
                Some(GearItem { name: name.to_owned(), detail })
            })
            .collect()
    })
}

impl App {
    pub(in crate::tui) fn open_agent_gear(&mut self, backend: &str) {
        self.overlays.push(Overlay::Gear(Gear { backend: backend.to_owned(), lists: None, failed: false, cursor: 0 }));
        self.sheet("agentGear", json!({"backend": backend}), Reply::Gear(backend.to_owned()));
    }

    pub(in crate::tui) fn gear_key(mut gear: Gear, k: KeyEvent) -> Option<Overlay> {
        let rows = gear.rows();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => gear.cursor = (gear.cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => gear.cursor = gear.cursor.saturating_sub(1),
            _ => {}
        }
        Some(Overlay::Gear(gear))
    }
}
