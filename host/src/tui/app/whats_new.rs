//! What's new (docs/features/whats-new.md): an important release's highlights, once, in a sheet at start. This
//! terminal has no sign that it was used before the feature, so without a saved version it starts like a new install.

use crossterm::event::{KeyCode, KeyEvent};
use serde::Deserialize;

use super::{App, Overlay};
use crate::analytics::reported::Reported;
use crate::compat::at_least;
use crate::tui::manage::Scroll;

/// The version this terminal last opened on.
const FILE: &str = "tui-whats-new";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Deserialize)]
pub struct Announcement {
    pub version: String,
    /// English phrases from the catalogs.
    pub notes: Vec<String>,
}

/// The sheet: the announcements due, newest first.
pub struct WhatsNew {
    pub due: Vec<Announcement>,
    pub scroll: Scroll,
    /// On screen once (then saved and reported).
    shown: bool,
}

impl WhatsNew {
    /// Only esc closes it; the rest scrolls.
    pub(super) fn key(mut self, k: KeyEvent) -> Option<Overlay> {
        if k.code == KeyCode::Esc {
            return None;
        }
        self.scroll.key(k);
        Some(Overlay::WhatsNew(self))
    }
}

/// The file every client reads, newest first.
fn announcements() -> Vec<Announcement> {
    serde_json::from_str(include_str!(
        "../../../../apps/ios/Kit/Sources/CodyncKit/Resources/Announcements/announcements.json"
    ))
    .expect("announcements.json is a list of {version, notes}")
}

/// Those with `seen < version ≤ current`, in the file's order (newest first); none without `seen`.
fn due(all: Vec<Announcement>, seen: Option<&str>, current: &str) -> Vec<Announcement> {
    let Some(seen) = seen else { return Vec::new() };
    all.into_iter().filter(|a| !at_least(seen, &a.version) && at_least(current, &a.version)).collect()
}

fn save() {
    if let Err(error) = super::super::choice::write(FILE, CURRENT) {
        tracing::warn!(%error, "couldn't save the version What's new last showed");
    }
}

impl App {
    /// At start: the sheet when an announcement is due, else this version counts as seen right away.
    pub(in crate::tui) fn open_whats_new(&mut self) {
        let seen = super::super::choice::read(FILE);
        let due = due(announcements(), seen.as_deref().map(str::trim), CURRENT);
        if due.is_empty() {
            save();
        } else {
            self.overlays.push(Overlay::WhatsNew(WhatsNew { due, scroll: Scroll::default(), shown: false }));
        }
    }

    /// Before each draw: once the sheet is on top, this version counts as seen.
    pub(in crate::tui) fn whats_new_seen(&mut self) {
        let Some(Overlay::WhatsNew(sheet)) = self.overlays.last_mut() else { return };
        if sheet.shown {
            return;
        }
        sheet.shown = true;
        save();
        self.track(Reported::WhatsNewShown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(versions: &[&str]) -> Vec<Announcement> {
        versions.iter().map(|v| Announcement { version: (*v).into(), notes: vec![format!("New in {v}.")] }).collect()
    }

    fn due_versions(seen: Option<&str>, current: &str) -> Vec<String> {
        due(list(&["2.46.0", "2.45.0", "2.44.0"]), seen, current).into_iter().map(|a| a.version).collect()
    }

    #[test]
    fn the_embedded_file_parses_newest_first() {
        let all = announcements();
        for a in &all {
            assert!(a.version.split('.').count() == 3 && !a.notes.is_empty(), "{a:?}");
        }
        for pair in all.windows(2) {
            assert!(!at_least(&pair[1].version, &pair[0].version), "{} before {}", pair[0].version, pair[1].version);
        }
    }

    #[test]
    fn due_between_the_seen_and_the_current_version() {
        assert!(due_versions(None, "2.45.0").is_empty(), "a new install shows nothing");
        assert_eq!(due_versions(Some("2.43.2"), "2.44.0"), ["2.44.0"]);
        assert!(due_versions(Some("2.44.0"), "2.44.0").is_empty(), "already seen on this version");
        assert_eq!(due_versions(Some("2.44.0"), "2.45.1"), ["2.45.0"], "a later version shows it too");
        assert_eq!(due_versions(Some("2.43.0"), "2.46.0"), ["2.46.0", "2.45.0", "2.44.0"], "newest first");
        assert!(due_versions(Some("2.46.0"), "2.44.0").is_empty(), "going back shows nothing");
        assert!(due_versions(Some("garbage"), "2.46.0").is_empty(), "a damaged file shows nothing");
    }
}
