//! The usage analytics sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::l10n::tr;
use super::super::manage::AnalyticsSheet;
use super::super::md;
use super::buffer::{centered, frame_box, hline, put, put_line, u};
use super::style::theme;

/// What sharing usage analytics sends; the same text as the desktop app's and the iPhone's,
/// true to the privacy policy.
const DETAILS: [(&str, &str); 7] = [
    (
        "Features you use",
        "Opening the app and its screens, setting up and pairing (and whether you allow notifications), creating or deleting a bot, sending a message (how many attachments, never the files), running a routine or a voice call, installing a connector. Each comes with the app version, the platform, and the bot's agent and model.",
    ),
    (
        "Crashes and errors",
        "When Codync crashes or an agent fails to answer: the kind of error and where in Codync's own code it happened. Never the error's message; file paths are cut down to Codync's own files.",
    ),
    (
        "Approximate location",
        "Country and city, looked up from your IP address. The IP address itself is never stored.",
    ),
    ("Who it belongs to", "Your Codync account when you're signed in, otherwise a random ID made on this device."),
    (
        "Never shared",
        "Your messages, prompts, code, files, file names or paths, bot names, credentials, or anything your agents produce.",
    ),
    (
        "How it's used",
        "Only to spot bugs sooner and see which features work and where people get stuck. Never sold, never used for ads or to track you across other apps and websites.",
    ),
    (
        "Your choice",
        "Each iPhone and each computer has its own switch. Turning it off stops sending right away and drops anything not sent yet.",
    ),
];

pub(super) fn analytics(buf: &mut Buffer, area: Rect, s: &AnalyticsSheet) {
    let t = theme();
    let r = centered(area, 72, area.height.saturating_sub(4).min(44));
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Usage analytics").as_ref(), t.text)));
    if inner.height < 5 {
        return;
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    put(buf, inner.x, inner.bottom() - 1, inner.width, " ↑↓ scroll · esc close", t.dim.patch(t.panel));

    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    let width = usize::from(body.width);
    let mut lines = Vec::new();
    for (title, detail) in DETAILS {
        let title = tr(title).into_owned();
        let detail = tr(detail).into_owned();
        lines.push(Line::from(Span::styled(format!(" {title}"), t.bold)));
        lines.extend(md::wrap(&[Span::styled(detail, t.secondary)], width, &[Span::raw(" ")], &[Span::raw(" ")]));
        lines.push(Line::default());
    }
    let policy = tr("See the Privacy Policy for details: ").into_owned();
    lines.push(Line::from(vec![
        Span::styled(format!(" {policy}"), t.dim),
        Span::styled("https://codync.dev/privacy", t.dim.add_modifier(Modifier::UNDERLINED)),
    ]));
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    s.max_scroll.set(max);
    s.page.set(h);
    for (i, l) in lines.iter().skip(s.scroll.min(max)).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
}
