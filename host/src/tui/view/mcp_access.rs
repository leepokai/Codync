//! The MCP access sheet: the computer's own levels, then the apps it lets in.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::l10n::{fill, tr};
use super::super::manage::{MCP_OWN, McpSheet, McpStep, mcp_days_left};
use super::buffer::{centered, frame_box, hline, put, restyle, rput};
use super::network::wrapped;
use super::overlay::field_line;
use super::style::theme;

/// What each level reads as, as the desktop app's picker says it.
fn level_name(level: &str) -> String {
    tr(match level {
        "ask" => "Ask for every request",
        "auto" => "Auto",
        "full" => "Full access",
        _ => "Read only",
    })
    .into_owned()
}

pub(super) fn mcp_access(buf: &mut Buffer, area: Rect, s: &McpSheet) {
    let t = theme();
    let r = centered(area, 84, area.height.saturating_sub(4).min(26));
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("MCP access").as_ref(), t.text)));
    if inner.height < 8 {
        return;
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    let mut y = body.y;
    let keys = match &s.step {
        McpStep::Name(name) => {
            put(buf, body.x, y, body.width, &tr("Give an app access"), t.bold.patch(t.panel));
            y += 1;
            wrapped(
                buf,
                body,
                &mut y,
                &tr("It starts at Ask for every request on any bot, for 30 days. You can change it after."),
                t.secondary,
            );
            field_line(buf, body, y + 1, name, &tr("Name"), true);
            tr("enter creates · esc back")
        }
        McpStep::Token(command) => {
            put(buf, body.x, y, body.width, &tr("Copy its token now"), t.bold.patch(t.panel));
            y += 1;
            wrapped(
                buf,
                body,
                &mut y,
                &tr("It isn't shown again. Anyone with it can act as this app: keep it out of chats and repositories."),
                t.secondary,
            );
            y += 1;
            wrapped(buf, body, &mut y, command, t.text);
            tr("y copies · enter done")
        }
        McpStep::List | McpStep::Revoke(_) => {
            let Some(data) = &s.data else {
                let text = s.error.clone().unwrap_or_else(|| tr("Loading").into_owned());
                put(buf, body.x, y, body.width, &text, t.secondary.patch(t.panel));
                return keys_line(buf, inner, &tr("esc closes"));
            };
            for (i, (key, label, detail)) in MCP_OWN.iter().enumerate() {
                let sel = s.cursor == i;
                row(buf, body, y, &tr(label), &level_name(data["levels"][key].as_str().unwrap_or_default()), sel);
                put(buf, body.x + 2, y + 1, body.width - 2, &tr(detail), t.dim.patch(t.panel));
                y += 2;
            }
            y += 1;
            let apps = s.apps();
            if apps.is_empty() {
                put(buf, body.x, y, body.width, &tr("No apps yet. Press n to give one access."), t.dim.patch(t.panel));
            }
            for (i, app) in apps.iter().enumerate() {
                if y >= body.bottom() {
                    break;
                }
                let sel = s.cursor == i + MCP_OWN.len();
                let left = mcp_days_left(app["expiresAt"].as_i64().unwrap_or_default(), crate::store::now_ms());
                let until = left.map_or_else(
                    || tr("Expired").into_owned(),
                    |days| fill(&tr("Expires in {count} days"), &[("count", &days.to_string())]),
                );
                let name = format!("{} · {until}", app["label"].as_str().unwrap_or_default());
                row(buf, body, y, &name, &level_name(app["access"].as_str().unwrap_or_default()), sel);
                y += 1;
            }
            if let Some(error) = &s.error {
                put(buf, body.x, body.bottom() - 1, body.width, error, t.red.patch(t.panel));
            }
            if matches!(s.step, McpStep::Revoke(_)) {
                tr("x again revokes it: its token stops working right away · any other key keeps it")
            } else {
                tr("←/→ level · n new app · r renew 30 days · x revoke · esc closes")
            }
        }
    };
    keys_line(buf, inner, &keys);
}

fn keys_line(buf: &mut Buffer, inner: Rect, keys: &str) {
    let t = theme();
    put(buf, inner.x, inner.bottom() - 1, inner.width, keys, t.dim.patch(t.panel));
}

fn row(buf: &mut Buffer, body: Rect, y: u16, label: &str, level: &str, sel: bool) {
    let t = theme();
    let base = if sel { t.sel } else { t.panel };
    if sel {
        restyle(buf, Rect::new(body.x - 1, y, body.width + 2, 1), t.sel);
    }
    let x = rput(buf, body.right(), y, &format!("‹ {level} ›"), t.text.patch(base));
    put(buf, body.x, y, x.saturating_sub(body.x + 1), label, t.text.patch(base));
}
