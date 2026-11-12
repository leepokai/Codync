//! Find in the open conversation (`/` or ^f): the text searched, the match shown and stepping between matches. The
//! chat view counts and paints the matches in the lines it draws.

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Editor, page};
use crate::analytics::reported::Reported;

pub struct Find {
    pub query: Editor,
    /// The match shown, from 0; the view keeps it under `matches`.
    pub active: usize,
    /// How many matches the last draw found.
    pub matches: usize,
    /// The next draw scrolls the shown match into view (a new text, a step).
    pub reveal: bool,
    /// The text last reported as searched, so each text counts once.
    reported: String,
}

impl App {
    /// `/` or ^f on a conversation: the find bar, typing into it (again: back into its text).
    pub(in crate::tui) fn open_find(&mut self) {
        if self.selected.is_none() {
            return;
        }
        self.typing = false;
        if self.find.is_some() {
            return;
        }
        self.find =
            Some(Find { query: Editor::default(), active: 0, matches: 0, reveal: false, reported: String::new() });
        self.track(Reported::ConversationFindOpened);
    }

    /// Keys while the bar is open: the text, ↵ ↓ next, ↑ previous, esc closes.
    pub(super) fn find_key(&mut self, k: KeyEvent) {
        let Some(find) = self.find.as_mut() else { return };
        match k.code {
            KeyCode::Esc => self.close_find(),
            KeyCode::Enter | KeyCode::Down => self.step_find(true),
            KeyCode::Up => self.step_find(false),
            KeyCode::PageUp => self.scroll_chat(page(self.chat_height)),
            KeyCode::PageDown => self.scroll_chat(-page(self.chat_height)),
            _ => {
                // Cursor moves keep the shown match; only a changed text starts again from the first.
                let before = find.query.text.clone();
                if find.query.key(k) && find.query.text != before {
                    find.active = 0;
                    find.reveal = true;
                }
            }
        }
    }

    pub(super) fn paste_find(&mut self, s: &str) -> bool {
        let Some(find) = self.find.as_mut() else { return false };
        find.query.insert(s);
        find.active = 0;
        find.reveal = true;
        true
    }

    fn step_find(&mut self, forward: bool) {
        let Some(find) = self.find.as_mut().filter(|f| f.matches > 0) else { return };
        find.active = (find.active + if forward { 1 } else { find.matches - 1 }) % find.matches;
        find.reveal = true;
        self.report_find();
    }

    /// Closing clears the highlights with the bar (also on picking another bot or typing a message).
    pub(super) fn close_find(&mut self) {
        self.report_find();
        self.find = None;
    }

    /// `conversation_find_searched` once per text, when it is stepped through or the bar closes: whether it found
    /// anything, never the text.
    fn report_find(&mut self) {
        let Some(find) = self.find.as_mut() else { return };
        if find.query.text.trim().is_empty() || find.reported == find.query.text {
            return;
        }
        find.reported.clone_from(&find.query.text);
        let found = find.matches > 0;
        self.track(Reported::ConversationFindSearched { found });
    }
}

/// One character folded for comparing without case; the two Greek small sigmas are one letter.
fn fold(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|l| if l == 'ς' { 'σ' } else { l })
}

/// Where `query` is in `line`, ignoring case, as (first char, end char); overlapping matches count once.
pub fn match_ranges(line: &str, query: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = query.chars().flat_map(fold).collect();
    if needle.is_empty() {
        return vec![];
    }
    // Each folded char, with the char of `line` it came from (one may fold to two).
    let hay: Vec<(char, usize)> = line.chars().enumerate().flat_map(|(i, c)| fold(c).map(move |l| (l, i))).collect();
    let mut found = vec![];
    let mut at = 0;
    while at + needle.len() <= hay.len() {
        if hay[at..at + needle.len()].iter().map(|h| h.0).eq(needle.iter().copied()) {
            found.push((hay[at].1, hay[at + needle.len() - 1].1 + 1));
            at += needle.len();
        } else {
            at += 1;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::test_app;
    use super::*;

    #[test]
    fn matching_ignores_case_and_counts_each_place_once() {
        assert_eq!(match_ranges("Week week WEEK", "week"), [(0, 4), (5, 9), (10, 14)]);
        assert_eq!(match_ranges("aaaa", "aa"), [(0, 2), (2, 4)]);
        assert_eq!(match_ranges("anything", ""), []);
        assert_eq!(match_ranges("İstanbul and week", "week"), [(13, 17)]);
        assert_eq!(match_ranges("ΛΟΓΟΣ", "ος"), [(3, 5)]);
    }

    #[tokio::test]
    async fn the_bar_steps_around_and_closes() {
        let mut app = test_app();
        app.open_find();
        assert!(app.find.is_none(), "nothing to find in without a conversation");
        app.selected = Some("a".into());
        app.typing = true;
        app.open_find();
        assert!(!app.typing);
        for c in "hi".chars() {
            app.on_key(KeyEvent::from(KeyCode::Char(c)));
        }
        let find = app.find.as_mut().expect("open");
        assert_eq!(find.query.text, "hi");
        find.matches = 3;
        app.on_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.find.as_ref().map(|f| f.active), Some(2), "back from the first wraps to the last");
        app.on_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(app.find.as_ref().map(|f| f.active), Some(2), "moving the cursor keeps the shown match");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.find.as_ref().map(|f| (f.active, f.reported.as_str())), Some((0, "hi")));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.find.is_none());
        app.open_find();
        app.start_typing();
        assert!(app.find.is_none() && app.typing, "typing a message closes the bar");
        app.open_find();
        app.select("b");
        assert!(app.find.is_none(), "another bot closes the bar");
    }
}
