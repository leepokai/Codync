//! Usage analytics: the ^k toggle, `app_opened` and `screen_viewed`. Sharing is on unless turned off.
//! The TUI only talks to the host over its loopback API, so it can always decide for this computer.

use serde_json::{Value, json};

use super::{After, App, Overlay, TraceMode};
use crate::analytics::reported::{App as Reporter, Reported, Screen, Track};

/// What this run has reported: the launch, the screen on top, the conversation in view.
#[derive(Default)]
pub(super) struct Seen {
    opened: bool,
    screen: Option<Screen>,
    chat: Option<String>,
}

impl App {
    /// Every `hello`: keep the setting; the first one of this run also reports the launch.
    pub(super) fn analytics_hello(&mut self, v: &Value) {
        self.analytics = v["analytics"].as_bool();
        if self.seen.opened {
            return;
        }
        self.seen.opened = true;
        self.track(Reported::AppOpened { at_login: None });
    }

    /// Before each draw: `screen_viewed` for a screen just opened, and for each conversation
    /// opened (hiding one lets its next opening count).
    pub(in crate::tui) fn report_screen(&mut self) {
        if !self.seen.opened {
            return;
        }
        let screen = self.screen_on_top();
        if screen != self.seen.screen {
            self.seen.screen = screen;
            if let Some(screen) = screen {
                self.track(Reported::ScreenViewed { screen });
            }
        }
        // The full trace covers the conversation like a pushed screen: coming back isn't an opening.
        if self.trace == TraceMode::Full {
            return;
        }
        let chat = self.selected.clone().filter(|_| self.chat_visible());
        if chat.is_some() && chat != self.seen.chat {
            self.track(Reported::ScreenViewed { screen: Screen::Chat });
        }
        self.seen.chat = chat;
    }

    /// The screen the user is on, other than the conversation: the topmost overlay that is
    /// one, else the full trace.
    fn screen_on_top(&self) -> Option<Screen> {
        let overlay = self.overlays.iter().rev().find_map(|o| match o {
            Overlay::Usage => Some(Screen::Usage),
            Overlay::Pair(_) => Some(Screen::Pairing),
            Overlay::Memory(_) => Some(Screen::Memory),
            Overlay::Library(_) => Some(Screen::Library),
            Overlay::Routines(_) => Some(Screen::Routines),
            Overlay::Market(_) => Some(Screen::Marketplace),
            Overlay::Help(_) => Some(Screen::Help),
            Overlay::Form(_) => Some(Screen::BotDetails),
            Overlay::Group(g) if g.group_id.is_some() => Some(Screen::BotDetails),
            _ => None,
        });
        overlay.or_else(|| (self.trace == TraceMode::Full).then_some(Screen::Trace))
    }

    fn track(&self, event: Reported) {
        self.call("track", Track::body(Reporter::Tui, event), After::Tracked);
    }

    /// ^k → Share usage analytics: flips it.
    pub(super) fn toggle_analytics(&mut self) {
        if let Some(on) = self.analytics {
            self.call("setAnalytics", json!({"enabled": !on}), After::Analytics);
        }
    }

    pub(super) fn analytics_set(&mut self, v: &Value) {
        self.analytics = v["enabled"].as_bool();
        self.flash(if self.analytics == Some(true) {
            "Sharing usage analytics"
        } else {
            "Not sharing usage analytics"
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Width, test_app};
    use super::*;

    #[tokio::test]
    async fn screens_count_as_they_open_and_chats_as_the_bot_changes() {
        let mut app = test_app();
        app.overlays.push(Overlay::Usage);
        app.report_screen();
        assert_eq!(app.seen.screen, None, "nothing before the first hello");
        app.seen.opened = true;
        app.report_screen();
        assert_eq!(app.seen.screen, Some(Screen::Usage));
        app.overlays.push(Overlay::Analytics(crate::tui::manage::AnalyticsSheet::default()));
        app.report_screen();
        assert_eq!(app.seen.screen, Some(Screen::Usage), "a sheet over it isn't a screen");

        app.overlays.clear();
        app.trace = TraceMode::Full;
        app.report_screen();
        assert_eq!(app.seen.screen, Some(Screen::Trace));

        app.trace = TraceMode::Off;
        (app.width, app.selected) = (Width::Wide, Some("a".into()));
        app.report_screen();
        assert_eq!((app.seen.screen, app.seen.chat.as_deref()), (None, Some("a")));
        app.selected = Some("b".into());
        app.report_screen();
        assert_eq!(app.seen.chat.as_deref(), Some("b"));

        app.trace = TraceMode::Full;
        app.report_screen();
        assert_eq!(app.seen.chat.as_deref(), Some("b"), "the full trace only covers the conversation");
        app.trace = TraceMode::Off;
        (app.width, app.chat_page) = (Width::Narrow, false);
        app.report_screen();
        assert_eq!(app.seen.chat, None, "back to the list lets reopening the same bot count");
        app.chat_page = true;
        app.report_screen();
        assert_eq!(app.seen.chat.as_deref(), Some("b"));
    }
}
