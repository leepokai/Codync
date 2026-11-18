//! The Ko-fi invitation, as the desktop app's (`apps/desktop/src/renderer/lib/kofi-ask.ts`): a sheet at start once
//! this terminal has sent messages on 3, 30 and 90 days, each counted from the one before so one that came late never
//! brings the next one sooner, then never again. It counts once on screen; esc closes it. Not on the phones: the
//! stores' rules on external tip links.

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Overlay};
use crate::analytics::reported::Reported;
use crate::tui::manage::Scroll;

pub(in crate::tui) const URL: &str = "https://ko-fi.com/leepokai";
/// This terminal's saved state: `<asked> <day> <days> <last>`.
const FILE: &str = "tui-kofi-invite";
/// The active days each invitation comes at.
const ASK_DAYS: [u32; 3] = [3, 30, 90];

/// Read and written whole around each change, so terminals running at once never put back an older copy.
#[derive(Debug, Default, PartialEq)]
struct Invites {
    /// Invitations that came.
    asked: u8,
    /// The active-day count at the last one.
    day: u32,
    /// Days this terminal sent a message on, and the last one (a local date).
    days: u32,
    last: String,
}

impl Invites {
    /// Anything else saved (the earlier card's state included) starts over.
    fn load() -> Self {
        super::super::choice::read(FILE).and_then(|saved| Self::parse(&saved)).unwrap_or_default()
    }

    fn parse(saved: &str) -> Option<Self> {
        let parts: Vec<&str> = saved.split_whitespace().collect();
        let [asked, day, days, last] = parts[..] else { return None };
        Some(Self {
            asked: asked.parse().ok()?,
            day: day.parse().ok()?,
            days: days.parse().ok()?,
            last: last.to_owned(),
        })
    }

    /// `today` counts once; whether it was new.
    fn count(&mut self, today: &str) -> bool {
        if self.last == today {
            return false;
        }
        self.days += 1;
        today.clone_into(&mut self.last);
        true
    }

    /// The invitation due (1 for the first): it waits the days between its mark and the one before, counted from the
    /// last one.
    fn due(&self) -> Option<u8> {
        let i = usize::from(self.asked);
        let at = *ASK_DAYS.get(i)?;
        let before = i.checked_sub(1).and_then(|b| ASK_DAYS.get(b)).copied().unwrap_or(0);
        (self.days.saturating_sub(self.day) >= at - before).then_some(self.asked + 1)
    }

    fn save(&self) {
        let value = format!("{} {} {} {}", self.asked, self.day, self.days, self.last);
        if let Err(error) = super::super::choice::write(FILE, &value) {
            tracing::warn!(%error, "couldn't save the Ko-fi invitation's state");
        }
    }
}

/// The sheet: invitation `ask` (1 for the first).
pub struct KofiSheet {
    ask: u8,
    pub scroll: Scroll,
    /// On screen once (then saved and reported).
    shown: bool,
}

impl KofiSheet {
    /// Only esc closes it, as What's new.
    pub(super) fn key(mut self, k: KeyEvent) -> Option<Overlay> {
        if k.code == KeyCode::Esc {
            return None;
        }
        self.scroll.key(k);
        Some(Overlay::Kofi(self))
    }
}

/// A message from this terminal arrived: today is an active day.
pub(super) fn sent() {
    let mut invites = Invites::load();
    if invites.count(&chrono::Local::now().format("%Y-%m-%d").to_string()) {
        invites.save();
    }
}

impl App {
    /// At start, after What's new: the invitation due, unless that sheet came.
    pub(in crate::tui) fn open_kofi(&mut self) {
        if let Some(ask) = Invites::load().due()
            && self.overlays.is_empty()
        {
            self.overlays.push(Overlay::Kofi(KofiSheet { ask, scroll: Scroll::default(), shown: false }));
        }
    }

    /// Before each draw: once the sheet is on top, its invitation counts, unless another terminal's already did.
    pub(in crate::tui) fn kofi_seen(&mut self) {
        let Some(Overlay::Kofi(sheet)) = self.overlays.last_mut() else { return };
        if sheet.shown {
            return;
        }
        sheet.shown = true;
        let ask = sheet.ask;
        let mut invites = Invites::load();
        if invites.asked >= ask {
            return;
        }
        invites.asked = ask;
        invites.day = invites.days;
        invites.save();
        self.track(Reported::KofiInviteShown { ask: Some(ask) });
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent};

    use super::super::{Overlay, test_app};
    use super::{Invites, sent};

    fn at(asked: u8, day: u32, days: u32) -> Invites {
        Invites { asked, day, days, last: String::new() }
    }

    #[test]
    fn a_day_counts_once() {
        let mut invites = Invites::default();
        assert!(invites.count("2026-11-17"));
        assert!(!invites.count("2026-11-17"));
        assert!(invites.count("2026-11-20"));
        assert_eq!(invites.days, 2);
    }

    #[test]
    fn it_comes_at_3_30_and_90_days_then_never() {
        assert_eq!(at(0, 0, 2).due(), None);
        assert_eq!(at(0, 0, 3).due(), Some(1));
        assert_eq!(at(1, 3, 29).due(), None);
        assert_eq!(at(1, 3, 30).due(), Some(2));
        assert_eq!(at(2, 30, 90).due(), Some(3));
        assert_eq!(at(3, 90, 1000).due(), None);
    }

    #[test]
    fn a_late_one_never_brings_the_next_sooner() {
        assert_eq!(at(0, 0, 40).due(), Some(1));
        assert_eq!(at(1, 40, 41).due(), None);
        assert_eq!(at(1, 40, 67).due(), Some(2));
    }

    #[test]
    fn the_saved_state_reads_back() {
        let invites = Invites { asked: 1, day: 3, days: 5, last: "2026-11-17".into() };
        assert_eq!(Invites::parse("1 3 5 2026-11-17\n"), Some(invites));
        assert_eq!(Invites::parse("done"), None, "the earlier card's state starts over");
        assert_eq!(Invites::parse("due b1 1700"), None);
    }

    // The only test that touches the saved state.
    #[tokio::test]
    async fn the_sheet_comes_at_start_on_its_own_counts_once_and_esc_closes_it() {
        Invites { asked: 0, day: 0, days: 3, last: "x".into() }.save();
        let mut held = test_app();
        held.overlays.push(Overlay::Usage(0));
        held.open_kofi();
        assert_eq!(held.overlays.len(), 1, "never over another sheet");

        let mut app = test_app();
        app.open_kofi();
        app.kofi_seen();
        app.kofi_seen();
        assert_eq!(Invites::load(), Invites { asked: 1, day: 3, days: 3, last: "x".into() });
        assert_eq!(Invites::load().due(), None, "not at the next start");
        let Some(Overlay::Kofi(sheet)) = app.overlays.pop() else { panic!("the sheet is on top") };
        assert!(sheet.key(KeyEvent::from(KeyCode::Esc)).is_none());

        sent();
        sent();
        assert_eq!(Invites::load().days, 4, "today counts once");
    }
}
