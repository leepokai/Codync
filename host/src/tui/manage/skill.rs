//! A skill's details sheet: its instructions (`SKILL.md`), with ↵ to install one from the catalog.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Overlay};
use super::{Reply, ctrl};

pub struct SkillSheet {
    /// The installed skill's id, or `catalog/<source>`.
    pub id: String,
    pub name: String,
    pub description: String,
    /// A catalog skill's folder in anthropics/skills; `None` for an installed one.
    pub source: Option<String>,
    /// The instructions; `None` while they load.
    pub markdown: Option<String>,
    pub error: Option<String>,
    /// Lines down from the top.
    pub scroll: usize,
    /// Set by the last draw, for clamping the scroll and paging.
    pub max_scroll: Cell<usize>,
    pub page: Cell<usize>,
}

impl App {
    /// Opens the details of an installed skill (`v` from `skills`) or, with `catalog`, of one
    /// from `marketSkills`.
    pub(in crate::tui) fn open_skill(&mut self, v: &Value, catalog: bool) {
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        let source = catalog.then(|| text("source"));
        let id = source.as_ref().map_or_else(|| text("id"), |s| format!("catalog/{s}"));
        let body = source.as_ref().map_or_else(|| json!({"id": id}), |s| json!({"source": s}));
        self.sheet("skillDetails", body, Reply::SkillDetails(id.clone()));
        self.overlays.push(Overlay::Skill(SkillSheet {
            id,
            name: text("name"),
            description: text("description"),
            source,
            markdown: None,
            error: None,
            scroll: 0,
            max_scroll: Cell::default(),
            page: Cell::default(),
        }));
    }

    /// Whether the sheet's skill is on this computer (a catalog one after it was installed).
    pub(in crate::tui) fn skill_installed(&self, s: &SkillSheet) -> bool {
        s.source.as_deref().is_none_or(|source| {
            let id = crate::market::slug(source);
            self.plugins.1.iter().any(|v| v["id"] == id.as_str())
        })
    }

    pub(in crate::tui) fn skill_key(&mut self, mut s: SkillSheet, k: KeyEvent) -> Option<Overlay> {
        let max = s.max_scroll.get();
        let half = (s.page.get() / 2).max(1);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Enter => {
                let source = s.source.clone().filter(|_| !self.skill_installed(&s))?;
                self.flash("Installing the skill…");
                self.sheet("installSkill", json!({"source": source}), Reply::FieldsSaved);
            }
            KeyCode::Down | KeyCode::Char('j') => s.scroll = (s.scroll + 1).min(max),
            KeyCode::Up | KeyCode::Char('k') => s.scroll = s.scroll.saturating_sub(1),
            KeyCode::PageDown => s.scroll = (s.scroll + half).min(max),
            KeyCode::PageUp => s.scroll = s.scroll.saturating_sub(half),
            KeyCode::Char('d') if ctrl(k) => s.scroll = (s.scroll + half).min(max),
            KeyCode::Char('u') if ctrl(k) => s.scroll = s.scroll.saturating_sub(half),
            _ => {}
        }
        Some(Overlay::Skill(s))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::{App, Overlay};
    use super::super::super::net::Client;
    use serde_json::json;

    #[tokio::test]
    async fn a_catalog_skill_counts_as_installed_once_the_computer_has_it() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.open_skill(&json!({"source": "Doc-Coauthoring", "name": "doc", "description": "Docs"}), true);
        let Some(Overlay::Skill(s)) = app.overlays.pop() else { unreachable!("open_skill shows its sheet") };
        assert_eq!((s.id.as_str(), s.source.as_deref()), ("catalog/Doc-Coauthoring", Some("Doc-Coauthoring")));
        assert!(!app.skill_installed(&s));
        app.plugins.1 = vec![json!({"id": "doc-coauthoring"})];
        assert!(app.skill_installed(&s), "installed under its slug");
        app.open_skill(&json!({"id": "mine", "name": "Mine", "source": "custom"}), false);
        let Some(Overlay::Skill(own)) = app.overlays.pop() else { unreachable!("open_skill shows its sheet") };
        assert_eq!((own.id.as_str(), own.source), ("mine", None), "an installed skill is looked up by id");
    }
}
