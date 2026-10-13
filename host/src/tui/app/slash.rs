//! The composer's `/` menu: the bot's skills, offered while the draft is just `/partial`.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use super::{After, App, Editor};

/// A skill the menu offers.
pub struct SkillMatch {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl App {
    /// The selected bot's skills whose id or name holds the `/partial` being typed (none in a group).
    pub fn skill_matches(&self) -> Vec<SkillMatch> {
        let Some(bot) = self.bot().filter(|b| !b.group) else { return vec![] };
        let draft = self.draft();
        let Some((query, None)) = crate::market::slash_command(&draft.text) else { return vec![] };
        let query = query.to_lowercase();
        self.plugins
            .1
            .iter()
            .filter_map(|s| {
                let id = s["id"].as_str()?;
                let name = s["name"].as_str().unwrap_or(id);
                let hit = id.contains(&query) || name.to_lowercase().contains(&query);
                (hit && bot.skills.iter().any(|on| on == id)).then(|| SkillMatch {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    description: s["description"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect()
    }

    /// Tab, or Return before the name is complete, fills in the first match. Returns whether it did.
    pub(super) fn complete_skill(&mut self, k: KeyEvent) -> bool {
        if !k.modifiers.is_empty() || !matches!(k.code, KeyCode::Tab | KeyCode::Enter) {
            return false;
        }
        let matches = self.skill_matches();
        let Some(first) = matches.first() else { return false };
        let typed = self.draft().text;
        // `/pdf` already names a skill: Return sends it.
        if k.code == KeyCode::Enter && matches.iter().any(|m| typed[1..] == m.id) {
            return false;
        }
        let Some(key) = self.draft_key() else { return false };
        self.drafts.insert(key, Editor::with(&format!("/{} ", first.id)));
        true
    }

    /// Starting a draft with `/` loads the computer's skills again, so the menu is current.
    pub(super) fn slash_typed(&self) {
        if self.bot().is_some_and(|b| !b.group) && self.draft().text == "/" {
            self.call("skills", json!({}), After::Skills);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bot, test_app};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    #[tokio::test]
    async fn the_menu_offers_and_fills_in_the_bots_skills() {
        let mut app = test_app();
        let bot = Bot::parse(&json!({"id": "b", "name": "Bot", "skills": ["pdf", "docx"]})).expect("a bot parses");
        app.bots.insert("b".into(), bot);
        app.selected = Some("b".into());
        app.plugins.1 = vec![
            json!({"id": "pdf", "name": "pdf", "description": "PDFs"}),
            json!({"id": "docx", "name": "docx"}),
            json!({"id": "xlsx", "name": "xlsx"}),
        ];
        let ids = |app: &super::App| app.skill_matches().into_iter().map(|m| m.id).collect::<Vec<_>>();
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        app.drafts.insert("b".into(), super::Editor::with("/"));
        assert_eq!(ids(&app), ["pdf", "docx"], "only skills the bot has on");
        app.drafts.insert("b".into(), super::Editor::with("/pd"));
        assert_eq!(ids(&app), ["pdf"]);
        assert!(app.complete_skill(key(KeyCode::Enter)), "Return fills in an unfinished name");
        assert_eq!(app.draft().text, "/pdf ");
        assert!(ids(&app).is_empty(), "the menu closes once the name is done");
        app.drafts.insert("b".into(), super::Editor::with("/pdf"));
        assert!(!app.complete_skill(key(KeyCode::Enter)), "a full name sends");
        assert!(app.complete_skill(key(KeyCode::Tab)));
    }
}
