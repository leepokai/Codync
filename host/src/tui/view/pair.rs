//! The pair-a-phone sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::super::md::{truncate, width as w};
use super::buffer::{centered, frame_box, put, u};
use super::style::theme;

pub(super) fn pair(buf: &mut Buffer, area: Rect, url: Option<&str>) {
    let t = theme();
    let Some(url) = url else {
        let r = centered(area, 40, 5);
        let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
        put(buf, inner.x, inner.y + 1, inner.width, "Getting the pairing code…", t.secondary.patch(t.panel));
        return;
    };
    let qr = qrcode::QrCode::new(url.as_bytes())
        .map(|c| c.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build())
        .unwrap_or_default();
    let lines: Vec<&str> = qr.lines().collect();
    let qw = lines.first().map_or(0, |l| w(l));
    let fits = u(lines.len() + 6) <= area.height && u(qw + 4) <= area.width;
    let (bw, bh) = if fits { (u(qw.max(50) + 4), u(lines.len() + 6)) } else { (70, 8) };
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
}
