//! A bot's Computer page: the computer's name and connection, then Use the computer (shown only when the
//! computer reports a screen, with how to set it up while computer access isn't ready).

use super::{App, Buffer, Rect, centered, footer, frame_box, name_of, page_tabs, put, rput, theme};
use crate::tui::l10n::tr;
use crate::tui::manage::Page;
use crate::tui::md::truncate;
use crate::tui::view::computer_access;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, bot: &str) {
    let t = theme();
    let r = centered(area, 90, 28);
    let title = name_of(app, bot);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let inner = page_tabs(buf, inner, Page::Computer);
    let host = if app.host.is_empty() { "codync" } else { &app.host };
    put(buf, inner.x, inner.y, inner.width, host, t.bold.patch(t.panel));
    let (state, style) = if !app.online {
        ("Offline", t.amber)
    } else if app.mismatch.is_some() {
        ("Needs update", t.red)
    } else {
        ("Connected", t.secondary)
    };
    rput(buf, inner.right(), inner.y, &tr(state), style.patch(t.panel));
    if !app.screen.is_null() {
        let y = inner.y + 2;
        let on = app.bots.get(bot).is_some_and(|b| b.computer);
        put(buf, inner.x, y, inner.width, &tr("Use the computer"), t.text.patch(t.panel));
        let x = rput(buf, inner.right(), y, "  u switches", t.dim.patch(t.panel));
        rput(buf, x, y, if on { "◉ On" } else { "○ Off" }, if on { t.text } else { t.secondary }.patch(t.panel));
        if !computer_access::ready(&app.screen) {
            let how = tr("On that computer, open Codync → Settings → Computer access to set it up first.");
            put(
                buf,
                inner.x,
                y + 1,
                inner.width,
                &truncate(&how, usize::from(inner.width)),
                t.secondary.patch(t.panel),
            );
        }
    }
    footer(buf, inner, None, "esc close");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Bot, test_app};
    use serde_json::json;

    fn shown(app: &App) -> String {
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        draw(&mut buf, area, app, "b");
        buf.content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    #[tokio::test]
    async fn shows_the_computer_and_offers_its_use_only_with_a_screen() {
        let mut app = test_app();
        app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "B", "computer": true})).expect("a bot"));
        (app.host, app.online) = ("studio".into(), true);
        let text = shown(&app);
        assert!(text.contains("studio") && text.contains("Connected"), "{text}");
        assert!(!text.contains("Use the computer"), "no screen reported: {text}");

        app.screen = json!({"enabled": true, "connected": true, "capture": true, "input": false});
        let text = shown(&app);
        assert!(text.contains("Use the computer") && text.contains("◉ On"), "{text}");
        assert!(text.contains("Computer access to set it up first"), "not ready yet: {text}");
        app.screen["input"] = true.into();
        assert!(!shown(&app).contains("set it up first"), "ready");

        app.online = false;
        assert!(shown(&app).contains("Offline"));
    }
}
