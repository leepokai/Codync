//! The band over the composer: the user's messages waiting for their turn, and the keys that act on them.

use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::{App, Bot, Entry};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{put, rput, u};
use super::style::theme;

/// Draws `bot`'s waiting messages right over the composer at `box_top`: where the band starts.
pub(super) fn queue_band(buf: &mut Buffer, r: Rect, app: &App, bot: &Bot, box_top: u16, typing: bool) -> u16 {
    let t = theme();
    let keys = match (app.queue_keys(), typing) {
        (false, _) => QueueKeys::Hidden,
        (true, false) => QueueKeys::Ready,
        (true, true) => QueueKeys::AfterEsc,
    };
    let rows = queued_rows(&app.queued(&bot.id), keys);
    let strip_top = box_top.saturating_sub(u(rows.len()));
    for (i, (text, hint)) in rows.iter().enumerate() {
        let y = strip_top + u(i);
        let room = rput(buf, r.right() - 1, y, hint, t.dim).saturating_sub(r.x + 3);
        put(buf, r.x + 1, y, room, &truncate(text, usize::from(room)), t.secondary);
    }
    strip_top
}

/// Which keys the queue band names.
#[derive(Clone, Copy)]
enum QueueKeys {
    /// A host too old for them.
    Hidden,
    Ready,
    /// Typing: they wait for esc.
    AfterEsc,
}

/// The queue band's rows, (text, keys): up to three messages, oldest first (more are counted), `S` beside the first
/// (it goes next) and `D` beside the last.
fn queued_rows(queued: &[&Entry], keys: QueueKeys) -> Vec<(String, String)> {
    let label = |e: &Entry| {
        let text = super::roster::plain(e.text().lines().next().unwrap_or_default());
        let files: Vec<String> = e.attachments().iter().map(|(name, _)| format!("▤ {name}")).collect();
        format!("› {}", if text.is_empty() { files.join("  ") } else { text })
    };
    let mut rows: Vec<(String, String)> = if queued.len() > 3 {
        let mut rows: Vec<_> = queued[..2].iter().map(|e| (label(e), String::new())).collect();
        rows.push((format!("  +{} more", queued.len() - 2), String::new()));
        rows
    } else {
        queued.iter().map(|e| (label(e), String::new())).collect()
    };
    // `first`: the band's label leads.
    let hint = |first: bool, said: &[&str]| {
        let mut parts: Vec<Cow<str>> = if first { vec![tr("queued")] } else { vec![] };
        match keys {
            QueueKeys::Hidden => {}
            QueueKeys::Ready => parts.extend(said.iter().map(|s| tr(s))),
            QueueKeys::AfterEsc => parts.extend(std::iter::once("esc".into()).chain(said.iter().map(|s| tr(s)))),
        }
        parts.join(" · ")
    };
    match rows.as_mut_slice() {
        [] => {}
        [only] => only.1 = hint(true, &["S sends now", "D removes"]),
        [first, .., last] => {
            first.1 = hint(true, &["S sends this now"]);
            last.1 = hint(false, &["D removes the last"]);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_band_shows_the_oldest_first_with_the_keys_beside_who_they_act_on() {
        use crate::tui::app::Kind;
        let entry = |id: &str, data: serde_json::Value| Entry {
            id: id.into(),
            seq: 0,
            turn: 0,
            kind: Kind::User,
            data,
            created_at: 0,
        };
        let one = entry("1", serde_json::json!({"text": "first\nmore", "status": "queued"}));
        let file = entry("2", serde_json::json!({"attachments": [{"name": "a.png", "size": 1}], "status": "queued"}));
        assert_eq!(queued_rows(&[], QueueKeys::Ready), Vec::<(String, String)>::new());
        assert_eq!(
            queued_rows(&[&one], QueueKeys::Ready),
            [("› first".to_owned(), "queued · S sends now · D removes".to_owned())]
        );
        assert_eq!(
            queued_rows(&[&one, &file], QueueKeys::AfterEsc),
            [
                ("› first".to_owned(), "queued · esc · S sends this now".to_owned()),
                ("› ▤ a.png".to_owned(), "esc · D removes the last".to_owned())
            ]
        );
        let rows = queued_rows(&[&one, &one, &one, &file], QueueKeys::Ready);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2], ("  +2 more".to_owned(), "D removes the last".to_owned()));
        // An older host takes neither key: the band only says what waits.
        let rows = queued_rows(&[&one, &file], QueueKeys::Hidden);
        assert_eq!((rows[0].1.as_str(), rows[1].1.as_str()), ("queued", ""));
    }
}
