//! The roster as shown: its order and its lines (pinned bots, then each section under its label,
//! sorted by name, then bots in none), and moving through it.

use std::cmp::Reverse;
use std::collections::HashSet;

use icu_normalizer::DecomposingNormalizerBorrowed;
use serde_json::json;

use super::super::l10n::tr;
use super::super::manage::{Submit, section_fields};
use super::{After, App, Bot, Click, Overlay};

/// The first host with roster sections (`section`, `renameSection`).
const SECTIONS_SINCE: &str = "2.32.0";
/// The first host where the user picks the lead (`setLead`) and may delete it.
const LEAD_CHOICE_SINCE: &str = "2.33.0";

/// One line of the roster.
pub enum RosterLine<'a> {
    /// `PINNED` above the pinned bots, `BOTS` above the bots in no section.
    Label(&'static str),
    /// A section's label: how many bots it holds, their unread messages (shown while folded) and the
    /// first of them, which the cursor lands on while it is folded.
    Section {
        name: &'a str,
        bots: usize,
        unread: i64,
        folded: bool,
        first: &'a Bot,
    },
    Bot(&'a Bot),
}

impl Bot {
    /// The section the roster shows it in: none while pinned (it stays on top, outside sections).
    pub fn shown_section(&self) -> Option<&str> {
        if self.pinned { None } else { self.section.as_deref() }
    }

    /// The bots it shows among and reorders with: its band (`Bot::band`), and for the rest its section
    /// (or none).
    pub fn roster_group(&self) -> (u8, Option<&str>) {
        (self.band(), self.shown_section())
    }
}

/// How sections sort: by name with case and accents set aside ("Çalışma" beside "Cherry", "home"
/// before "Work"); then unaccented first ("Cafe", "Café"), then lowercase first ("work", "Work",
/// whose capitals have the lower code points), as the apps' collators order them.
// ponytail: accents folded, no locale rules (Swedish "å" sorts as "a", Turkish "ı" by code point,
// unlike the apps' system collators); icu_collator if the terminal must match them exactly.
fn section_key(name: &str) -> (String, String, Reverse<&str>) {
    let base = DecomposingNormalizerBorrowed::new_nfd()
        .normalize(name)
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .collect();
    (base, name.to_lowercase(), Reverse(name))
}

/// `roster` (in `App::roster` order) as lines; a folded section is its label alone. Without pinned
/// bots or sections there is nothing to tell apart, so no labels.
pub fn roster_lines<'a>(roster: &[&'a Bot], folded: &HashSet<String>) -> Vec<RosterLine<'a>> {
    let labeled = roster.iter().any(|b| b.pinned || b.shown_section().is_some());
    let mut lines = vec![];
    let mut group = None;
    for &b in roster {
        let here = b.roster_group();
        if labeled && group != Some(here) {
            lines.push(match here {
                (_, None) if b.pinned => RosterLine::Label("PINNED"),
                (_, None) => RosterLine::Label("BOTS"),
                (_, Some(name)) => {
                    let members: Vec<&Bot> =
                        roster.iter().copied().filter(|m| m.shown_section() == Some(name)).collect();
                    RosterLine::Section {
                        name,
                        bots: members.len(),
                        unread: members.iter().map(|m| m.unread).sum(),
                        folded: folded.contains(name),
                        first: b,
                    }
                }
            });
        }
        group = Some(here);
        if !here.1.is_some_and(|name| folded.contains(name)) {
            lines.push(RosterLine::Bot(b));
        }
    }
    lines
}

/// The line the cursor is on: the selected bot's, or its folded section's label.
pub fn cursor_line(lines: &[RosterLine<'_>], selected: Option<&Bot>) -> Option<usize> {
    let selected = selected?;
    lines.iter().position(|line| match line {
        RosterLine::Bot(b) => b.id == selected.id,
        RosterLine::Section { name, folded: true, .. } => selected.shown_section() == Some(*name),
        _ => false,
    })
}

impl RosterLine<'_> {
    /// The bot the cursor selects on this line: a bot's own, a folded section's first.
    fn stop(&self) -> Option<&str> {
        match self {
            Self::Bot(b) | Self::Section { first: b, folded: true, .. } => Some(b.id.as_str()),
            _ => None,
        }
    }

    /// What a click on this line does: a section's label folds or unfolds it, a bot's row selects it.
    pub fn click(&self) -> Option<Click> {
        match self {
            Self::Section { name, .. } => Some(Click::Section((*name).to_owned())),
            Self::Bot(b) => Some(Click::Bot(b.id.clone())),
            Self::Label(_) => None,
        }
    }
}

/// Keeps a section's fold through a rename on this device: a folded `from` stays folded as `to` (a
/// merge folds the section it joins) and is forgotten when removed (`to` is `None`); an unfolded
/// `from` changes nothing, so `to` is never unfolded.
pub fn rename_fold(folded: &mut HashSet<String>, from: &str, to: Option<&str>) {
    if folded.remove(from)
        && let Some(to) = to
    {
        folded.insert(to.to_owned());
    }
}

/// What the cursor can stop on, in order, as (line, bot id): the bots shown and each folded section
/// (at its first bot). The number keys, j/k and the compact strip all count these.
pub fn stops<'l>(lines: &'l [RosterLine<'_>]) -> Vec<(usize, &'l str)> {
    lines.iter().enumerate().filter_map(|(i, l)| Some((i, l.stop()?))).collect()
}

/// The bot `delta` stops from the cursor (no cursor: the first). A folded section is one stop.
fn step(lines: &[RosterLine<'_>], selected: Option<&Bot>, delta: isize) -> Option<String> {
    let stops = stops(lines);
    let last = stops.len().checked_sub(1)?;
    let at = cursor_line(lines, selected).and_then(|c| stops.iter().position(|&(i, _)| i == c));
    let next = at.map_or(0, |i| i.saturating_add_signed(delta).min(last));
    Some(stops[next].1.to_owned())
}

const NOT_IN_A_SECTION: &str = "This bot isn't in a section";

impl App {
    /// Roster order: the lead, pinned bots, then sections by name, then bots in none; inside each,
    /// where the user put them, then most recent (same as the phone).
    pub fn roster(&self) -> Vec<&Bot> {
        let mut v: Vec<&Bot> = self.bots.values().filter(|b| !b.hidden).collect();
        v.sort_by_cached_key(|&b| {
            let section = b.shown_section().map(section_key);
            let name = b.shown_section().unwrap_or("");
            let order = self.section_order.iter().position(|s| s == name).unwrap_or(self.section_order.len());
            (b.band(), order, section.is_none(), section, b.position, Reverse(b.last_at), b.name.clone())
        });
        v
    }

    /// Every section on this computer once (hidden and pinned bots' too), in the roster's order.
    fn section_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.bots.values().filter_map(|b| b.section.as_deref()).collect();
        names.sort_by_cached_key(|&name| section_key(name));
        names.dedup();
        names
    }

    /// j/k in the roster: the next shown bot, a folded section counting as one.
    pub(super) fn move_bot(&mut self, delta: isize) {
        let Some(id) = step(&roster_lines(&self.roster(), &self.folded), self.bot(), delta) else { return };
        self.select(&id);
    }

    /// The bot key `n` (from 0 for `1`) selects: the n-th stop, a folded section being one (its first bot).
    pub(super) fn stop_at(&self, n: usize) -> Option<String> {
        let lines = roster_lines(&self.roster(), &self.folded);
        stops(&lines).get(n).map(|&(_, id)| id.to_owned())
    }

    /// `z`: folds the section under the cursor, or unfolds it.
    pub(super) fn toggle_section(&mut self) {
        let Some(bot) = self.bot() else { return };
        let Some(name) = bot.shown_section().map(str::to_owned) else {
            self.flash(NOT_IN_A_SECTION);
            return;
        };
        self.toggle_fold(&name);
    }

    /// Folds section `name`, or unfolds it (`z`, a click on its label).
    pub(super) fn toggle_fold(&mut self, name: &str) {
        if !self.folded.remove(name) {
            self.folded.insert(name.to_owned());
        }
    }

    /// A click on section `name`'s label: folds or unfolds it. Opened, its first bot takes the cursor
    /// (where j/k land), so the list doesn't scroll back to a bot elsewhere and leave it off screen.
    pub(super) fn click_section(&mut self, name: &str) {
        self.toggle_fold(name);
        if self.folded.contains(name) {
            return;
        }
        if let Some(id) = self.roster().iter().find(|b| b.shown_section() == Some(name)).map(|b| b.id.clone()) {
            self.select(&id);
        }
    }

    /// `*`: makes the selected bot the lead, in place of the one before. A group can't lead.
    pub(super) fn make_lead(&mut self) {
        if !self.bot().is_some_and(|b| !b.lead && !b.group) || !self.lead_choice_supported() {
            return;
        }
        if let Some(b) = self.bot() {
            self.call("setLead", json!({"botId": b.id}), After::Nothing);
        }
    }

    /// `w`: "Move to section" for the selected bot: one of the sections, a new name, or empty for none.
    pub(super) fn ask_section(&mut self) {
        if self.bot().is_none() || !self.sections_supported() {
            return;
        }
        let Some(bot) = self.bot() else { return };
        let names = self.section_names();
        // Translated before the names go in: the sheet's own translation can't match a filled-in note.
        let mut note = tr("Empty takes it out of its section; a new name makes one.").into_owned();
        if !names.is_empty() {
            note.push_str(&tr(" Sections: {list}").replace("{list}", &names.join(", ")));
        }
        let fields = section_fields("Move to section", note, bot.section.as_deref(), Submit::Section(bot.id.clone()));
        self.overlays.push(Overlay::Fields(Box::new(fields)));
    }

    /// `W`: "Rename section" for the section under the cursor: an existing name merges the two, empty
    /// removes the section (its bots stay).
    pub(super) fn ask_rename_section(&mut self) {
        if self.bot().is_none() || !self.sections_supported() {
            return;
        }
        let Some(name) = self.bot().and_then(Bot::shown_section).map(str::to_owned) else {
            self.flash(NOT_IN_A_SECTION);
            return;
        };
        let note = "Empty removes the section; its bots stay. A name in use merges them.".to_owned();
        let fields = section_fields("Rename section", note, Some(name.as_str()), Submit::RenameSection(name.clone()));
        self.overlays.push(Overlay::Fields(Box::new(fields)));
    }

    /// Whether the host has sections; says what to update when it doesn't.
    fn sections_supported(&mut self) -> bool {
        self.host_supports(SECTIONS_SINCE, "Update Codync on {host}: sections need {version} or newer.")
    }

    /// Whether the user may pick the lead and delete it; says what to update when the host is older.
    pub(super) fn lead_choice_supported(&mut self) -> bool {
        self.host_supports(LEAD_CHOICE_SINCE, "Update Codync on {host}: changing the lead needs {version} or newer.")
    }

    /// Whether the host is `since` or newer; says what to update when it isn't (`why`, with `{host}` and `{version}`).
    fn host_supports(&mut self, since: &str, why: &str) -> bool {
        if self.host_version.is_empty() {
            self.flash("Still connecting: try again in a moment.");
            return false;
        }
        let supported = crate::compat::at_least(&self.host_version, since);
        if !supported {
            let why = tr(why).replace("{host}", &self.host).replace("{version}", since);
            self.flash(&why);
        }
        supported
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bot, Click, Overlay, test_app};
    use super::{App, NOT_IN_A_SECTION, RosterLine, cursor_line, rename_fold, roster_lines, section_key};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use serde_json::json;
    use std::collections::HashSet;

    fn add(app: &mut App, id: &str, pinned: bool, section: Option<&str>, last_at: i64, unread: i64) {
        let v = json!({"id": id, "pinned": pinned, "section": section, "lastAt": last_at, "unread": unread});
        app.bots.insert(id.into(), Bot::parse(&v).expect("a bot with an id parses"));
    }

    /// The roster as text: labels, `▾ name (bots)` or `▸ name (bots) unread`, bot ids.
    fn shown(app: &App) -> Vec<String> {
        roster_lines(&app.roster(), &app.folded)
            .iter()
            .map(|line| match line {
                RosterLine::Label(l) => (*l).to_owned(),
                RosterLine::Section { name, bots, folded: false, .. } => format!("▾ {name} ({bots})"),
                RosterLine::Section { name, bots, unread, .. } => format!("▸ {name} ({bots}) {unread}"),
                RosterLine::Bot(b) => b.id.clone(),
            })
            .collect()
    }

    #[tokio::test]
    async fn pinned_bots_then_sections_by_name_then_the_rest() {
        let mut app = test_app();
        for (id, pinned, section, last_at, unread) in [
            ("p", true, Some("Work"), 9, 0),
            ("w1", false, Some("Work"), 3, 2),
            ("w2", false, Some("Work"), 2, 1),
            ("h", false, Some("home"), 1, 0),
            ("x", false, None, 5, 0),
        ] {
            add(&mut app, id, pinned, section, last_at, unread);
        }
        // "home" before "Work": case doesn't count; the pinned bot stays on top and out of Work.
        assert_eq!(shown(&app), ["PINNED", "p", "▾ home (1)", "h", "▾ Work (2)", "w1", "w2", "BOTS", "x"]);
        app.folded.insert("Work".into());
        assert_eq!(shown(&app), ["PINNED", "p", "▾ home (1)", "h", "▸ Work (2) 3", "BOTS", "x"]);
    }

    #[tokio::test]
    async fn the_lead_sits_in_its_section_like_any_bot() {
        let mut app = test_app();
        let lead = json!({"id": "l", "lead": true, "section": "Work", "lastAt": 0});
        app.bots.insert("l".into(), Bot::parse(&lead).expect("a bot with an id parses"));
        add(&mut app, "w", false, Some("Work"), 2, 0);
        add(&mut app, "x", false, None, 1, 0);
        add(&mut app, "p", true, None, 3, 0);
        assert_eq!(shown(&app), ["PINNED", "p", "▾ Work (2)", "w", "l", "BOTS", "x"]);
        app.host_version = "2.32.0".into();
        app.selected = Some("l".into());
        app.ask_section();
        assert!(!app.overlays.is_empty(), "the lead moves to another section like any bot");
    }

    #[tokio::test]
    async fn sections_sort_with_case_and_accents_set_aside_in_the_roster_and_the_prompt() {
        let mut app = test_app();
        for (id, section) in [("d", "Dersler"), ("c", "Çalışma"), ("e", "Éclair"), ("f", "fig"), ("h", "home")] {
            add(&mut app, id, false, Some(section), 0, 0);
        }
        let order = ["Çalışma", "Dersler", "Éclair", "fig", "home"];
        let headers: Vec<String> =
            shown(&app).into_iter().filter_map(|l| l.strip_prefix("▾ ").map(str::to_owned)).collect();
        assert_eq!(headers, order.map(|s| format!("{s} (1)")));
        assert_eq!(app.section_names(), order);
    }

    #[tokio::test]
    async fn j_and_k_pass_a_folded_section_as_one_row() {
        let mut app = test_app();
        for (id, section, last_at) in [("a", Some("Work"), 3), ("b", Some("Work"), 2), ("c", None, 1)] {
            add(&mut app, id, false, section, last_at, 0);
        }
        app.selected = Some("b".into());
        app.toggle_section();
        assert!(app.folded.contains("Work"));
        let lines = roster_lines(&app.roster(), &app.folded);
        assert_eq!(cursor_line(&lines, app.bot()), Some(0), "the cursor is on the folded label");
        app.move_bot(1);
        assert_eq!(app.selected.as_deref(), Some("c"));
        app.move_bot(-1);
        assert_eq!(app.selected.as_deref(), Some("a"), "back onto the label: its first bot");
        app.toggle_section();
        assert!(app.folded.is_empty());
        app.move_bot(1);
        assert_eq!(app.selected.as_deref(), Some("b"));
    }

    #[tokio::test]
    async fn number_keys_count_a_folded_section_as_one_row() {
        let mut app = test_app();
        for (id, section, last_at) in [("a", Some("Work"), 3), ("b", Some("Work"), 2), ("c", None, 1)] {
            add(&mut app, id, false, section, last_at, 0);
        }
        assert_eq!([0, 1, 2, 3].map(|n| app.stop_at(n)), [Some("a".into()), Some("b".into()), Some("c".into()), None]);
        app.folded.insert("Work".into());
        let at = [0, 1, 2].map(|n| app.stop_at(n));
        assert_eq!(at, [Some("a".into()), Some("c".into()), None], "1 is the section's first bot, 2 the next row");
    }

    #[tokio::test]
    async fn a_click_folds_a_section_or_opens_it_on_its_first_bot() {
        let mut app = test_app();
        for (id, section, last_at) in [("a", Some("Work"), 3), ("b", Some("Work"), 2), ("c", None, 1)] {
            add(&mut app, id, false, section, last_at, 0);
        }
        app.selected = Some("c".into());
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        for (folded, selected) in [(true, "c"), (false, "a")] {
            // The label's click, as the full roster and the compact strip (folded: its one cell) register it.
            let section = roster_lines(&app.roster(), &app.folded).iter().find_map(|l| match l {
                RosterLine::Section { .. } => l.click(),
                _ => None,
            });
            assert!(matches!(&section, Some(Click::Section(name)) if name == "Work"));
            app.hits.clicks = section.into_iter().map(|c| (Rect::new(0, 0, 20, 1), c)).collect();
            app.on_mouse(click);
            assert_eq!(app.folded.contains("Work"), folded);
            assert_eq!(app.selected.as_deref(), Some(selected), "folding keeps the selection, opening moves it in");
        }
    }

    #[test]
    fn names_alike_but_for_case_or_accents_sort_as_the_apps_do() {
        for names in [["Work", "work", "Café", "Cafe"], ["Cafe", "work", "Café", "Work"]] {
            let mut v = names.to_vec();
            v.sort_by_cached_key(|&name| section_key(name));
            assert_eq!(v, ["Cafe", "Café", "work", "Work"]);
        }
    }

    #[test]
    fn a_rename_keeps_a_folded_section_folded_under_its_new_name() {
        let fold = |folded: &[&str], from: &str, to: Option<&str>| {
            let mut set: HashSet<String> = folded.iter().map(|&s| s.to_owned()).collect();
            rename_fold(&mut set, from, to);
            let mut v: Vec<String> = set.into_iter().collect();
            v.sort();
            v
        };
        assert_eq!(fold(&["Work", "Home"], "Work", Some("Play")), ["Home", "Play"], "folded: the fold moves");
        assert_eq!(fold(&["Home"], "Work", Some("Play")), ["Home"], "unfolded: nothing changes");
        assert_eq!(fold(&["Work"], "Work", None), Vec::<String>::new(), "removed: forgotten");
        assert_eq!(fold(&["Work"], "Work", Some("Home")), ["Home"], "merged: the joined section folds");
        assert_eq!(fold(&["Home"], "Work", Some("Home")), ["Home"], "an unfolded merge never unfolds it");
    }

    #[tokio::test]
    async fn z_outside_a_section_only_says_so() {
        let mut app = test_app();
        add(&mut app, "p", true, Some("Work"), 1, 0);
        app.selected = Some("p".into());
        app.toggle_section();
        assert!(app.folded.is_empty(), "a pinned bot shows in no section");
        assert_eq!(app.hint.as_ref().map(|(s, _)| s.as_str()), Some(NOT_IN_A_SECTION));
        app.hint = None;
        app.selected = None;
        app.toggle_section();
        assert!(app.hint.is_none(), "nothing selected: nothing to say");
    }

    #[tokio::test]
    async fn sections_wait_for_a_host_that_has_them() {
        let mut app = test_app();
        add(&mut app, "a", false, Some("Work"), 0, 0);
        app.ask_section();
        app.ask_rename_section();
        assert!(app.overlays.is_empty() && app.hint.is_none(), "nothing selected: silent, like z");
        app.selected = Some("a".into());
        app.ask_section();
        assert!(app.overlays.is_empty(), "no hello yet: the host's version is unknown");
        assert_eq!(app.hint.as_ref().map(|(s, _)| s.as_str()), Some("Still connecting: try again in a moment."));
        app.host = "test".into();
        app.host_version = "2.31.2".into();
        app.ask_section();
        assert!(app.overlays.is_empty(), "an older host would drop the name");
        let why = app.hint.as_ref().map(|(s, _)| s.as_str());
        assert_eq!(why, Some("Update Codync on test: sections need 2.32.0 or newer."));
        app.host_version = "2.32.1-dev".into();
        app.ask_section();
        assert!(matches!(app.overlays.pop(), Some(Overlay::Fields(_))), "a dev build after it has them");
        app.host_version = "2.32.0".into();
        app.ask_section();
        let Some(Overlay::Fields(f)) = app.overlays.last() else { panic!("the section prompt") };
        assert_eq!((f.title.as_str(), f.inputs[0].ed.text.as_str()), ("Move to section", "Work"));
        assert_eq!(f.note, "Empty takes it out of its section; a new name makes one. Sections: Work");
        app.overlays.clear();
        app.ask_rename_section();
        let Some(Overlay::Fields(f)) = app.overlays.last() else { panic!("the rename prompt") };
        assert_eq!((f.title.as_str(), f.inputs[0].placeholder.as_str()), ("Rename section", "Section name"));
    }
}
