//! The pair-a-phone sheet.

use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

use super::super::app::{App, PairCode};
use super::super::l10n::tr;
use super::super::md::{truncate, width as w, wrap};
use super::buffer::{centered, frame_box, put, put_line, u};
use super::style::theme;

pub(super) fn pair(buf: &mut Buffer, area: Rect, app: &App, code: &PairCode) {
    if let Some(request) = app.pairing_requests.first() {
        return ask(buf, area, request);
    }
    let t = theme();
    let (text, style) = match code {
        PairCode::Ready(url) => return qr(buf, area, url, app.outside_account()),
        PairCode::Loading => ("Getting the pairing code…", t.secondary),
        PairCode::Failed(e) => (e.as_str(), t.red),
    };
    let r = centered(area, 56, 6);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
    put(buf, inner.x, inner.y + 1, inner.width, text, style.patch(t.panel));
    if matches!(code, PairCode::Failed(_)) {
        put(buf, inner.x, inner.bottom() - 1, inner.width, "r try again · esc close", t.dim.patch(t.panel));
    }
}

/// A phone that scanned the code waits for this computer: `y` lets it in and the computer joins its account.
fn ask(buf: &mut Buffer, area: Rect, request: &Value) {
    let t = theme();
    let name = request["deviceName"].as_str().unwrap_or("A phone");
    let email =
        request["email"].as_str().filter(|e| !e.is_empty()).map_or_else(|| tr("A Codync account"), Cow::Borrowed);
    let code = request["code"].as_str().unwrap_or_default();
    // A no-break space: the code never wraps apart.
    let code = match code.split_at_checked(3) {
        Some((a, b)) if code.len() == 6 => format!("{a}\u{a0}{b}"),
        _ => code.to_owned(),
    };
    // The name last: whatever it says stays as written.
    let text = tr("{name} wants to use this computer. It joins {email}'s account. Check the phone shows {code}.")
        .replace("{email}", &email)
        .replace("{code}", &code)
        .replace("{name}", name);
    let lines = note(text, 52, t.text.patch(t.panel));
    let r = centered(area, 56, u(lines.len() + 5));
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
    for (y, line) in (inner.y + 1..).zip(&lines) {
        put_line(buf, inner.x, y, inner.width, line);
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "y let in · n don't allow", t.dim.patch(t.panel));
}

fn note(text: String, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap(&[Span::styled(text, style)], width, &[], &[])
}

fn qr(buf: &mut Buffer, area: Rect, url: &str, outside_account: bool) {
    let t = theme();
    let qr = qrcode::QrCode::new(url.as_bytes())
        .map(|c| c.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build())
        .unwrap_or_default();
    let lines: Vec<&str> = qr.lines().collect();
    let qw = lines.first().map_or(0, |l| w(l));
    let account = |width: usize| {
        let text =
            tr("Not in an account: a signed-in phone that scans this asks to bring the computer into its account.");
        if outside_account { note(text.into_owned(), width, t.dim.patch(t.panel)) } else { vec![] }
    };
    let wide = account(qw.max(50));
    let fits = u(lines.len() + 6 + wide.len()) <= area.height && u(qw + 4) <= area.width;
    let (bw, bh, account) = if fits {
        (u(qw.max(50) + 4), u(lines.len() + 6 + wide.len()), wide)
    } else {
        let narrow = account(66);
        (70, u(8 + narrow.len()), narrow)
    };
    let r = centered(area, bw, bh);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
    let mut y = inner.y;
    if fits {
        // The QR needs light modules on dark ones regardless of the terminal theme.
        let qs = Style::default().fg(Color::White).bg(Color::Black);
        let x = inner.x + inner.width.saturating_sub(u(qw)) / 2;
        for l in &lines {
            put(buf, x, y, u(qw), l, qs);
            y += 1;
        }
    } else {
        put(buf, inner.x, y, inner.width, "Make the window taller to show the QR code.", t.secondary.patch(t.panel));
        y += 1;
    }
    put(
        buf,
        inner.x,
        y,
        inner.width,
        if url.starts_with("codync-dev:") {
            "Scan with Codync Dev on your iPhone:"
        } else {
            "Scan with the Codync iPhone app, or open on the phone:"
        },
        t.secondary.patch(t.panel),
    );
    put(buf, inner.x, y + 1, inner.width, &truncate(url, usize::from(inner.width)), t.dim.patch(t.panel));
    for (y, line) in (y + 2..).zip(&account) {
        put_line(buf, inner.x, y, inner.width, line);
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "r new code · esc close", t.dim.patch(t.panel));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shown(buf: &Buffer) -> String {
        let a = buf.area;
        (a.top()..a.bottom())
            .map(|y| (a.left()..a.right()).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_waiting_phone_shows_its_account_the_code_and_the_keys() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        let request = json!({"requestId": "a", "deviceName": "Pixel", "email": "k@example.com", "code": "123456"});
        let area = buf.area;
        ask(&mut buf, area, &request);
        let text = shown(&buf);
        for part in ["Pixel", "k@example.com", "123\u{a0}456", "y let in · n don't allow"] {
            assert!(text.contains(part), "{part} in\n{text}");
        }
    }

    #[test]
    fn a_computer_in_no_account_says_so_under_the_code() {
        let note =
            tr("Not in an account: a signed-in phone that scans this asks to bring the computer into its account.");
        // Its start, which fits on the first line in any language.
        let first: String = note.chars().take(6).collect();
        for (outside, says) in [(true, true), (false, false)] {
            let mut buf = Buffer::empty(Rect::new(0, 0, 100, 60));
            let area = buf.area;
            qr(&mut buf, area, "codync://pair?id=x&code=y", outside);
            assert_eq!(shown(&buf).contains(&first), says);
        }
    }
}
