//! Device-local ordering of whole roster groups, including unsectioned bots.

use super::App;

impl App {
    pub(crate) fn rename_section_order(&mut self, from: &str, to: Option<&str>) {
        if to == Some(from) {
            return;
        }
        let destination_exists = to.is_some_and(|name| self.section_order.iter().any(|n| n == name));
        self.section_order = self
            .section_order
            .iter()
            .filter_map(|name| {
                if name != from {
                    Some(name.clone())
                } else if destination_exists {
                    None
                } else {
                    to.map(str::to_owned)
                }
            })
            .collect();
    }

    /// Capture the effective displayed order before a rename's bot events arrive.
    pub(crate) fn shown_section_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for bot in self.roster().into_iter().filter(|b| !b.pinned) {
            let section = bot.shown_section().unwrap_or("").to_owned();
            if !names.contains(&section) {
                names.push(section);
            }
        }
        names
    }

    /// Alt+↑/↓ moves the selected bot's entire section one place, keeping pinned bots on top.
    pub(super) fn move_section(&mut self, direction: isize) {
        let Some(bot) = self.bot().filter(|b| !b.pinned) else { return };
        let name = bot.shown_section().unwrap_or("").to_owned();
        let mut names = self.shown_section_names();
        let Some(from) = names.iter().position(|n| n == &name) else { return };
        let Some(to) = from.checked_add_signed(direction).filter(|&i| i < names.len()) else { return };
        names.swap(from, to);
        self.section_order = names;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bot, test_app};
    use serde_json::json;

    #[tokio::test]
    async fn merging_into_a_new_group_keeps_its_displayed_place() {
        let mut app = test_app();
        for (id, section) in [("a", Some("Work")), ("b", Some("Home")), ("loose", None)] {
            app.bots.insert(id.into(), Bot::parse(&json!({"id": id, "section": section})).expect("bot"));
        }
        app.section_order = vec!["Work".into(), String::new()];
        app.section_order = app.shown_section_names();
        app.rename_section_order("Work", Some("Home"));
        assert_eq!(app.section_order, ["", "Home"]);
    }

    #[tokio::test]
    async fn whole_sections_move_both_ways_past_bots_without_moving_pinned_bots() {
        let mut app = test_app();
        for (id, pinned, section) in
            [("pin", true, None), ("a", false, Some("Work")), ("b", false, Some("Work")), ("loose", false, None)]
        {
            app.bots.insert(
                id.into(),
                Bot::parse(&json!({"id": id, "name": id, "pinned": pinned, "section": section})).expect("bot"),
            );
        }
        app.selected = Some("a".into());
        app.move_section(1);
        let ids = |app: &super::App| app.roster().iter().map(|b| b.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&app), ["pin", "loose", "a", "b"]);
        app.folded.insert("Work".into());
        app.move_section(-1);
        assert_eq!(ids(&app), ["pin", "a", "b", "loose"]);
        app.move_section(-1);
        assert_eq!(ids(&app), ["pin", "a", "b", "loose"]);
    }
}
