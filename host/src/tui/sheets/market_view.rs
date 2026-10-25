//! The Extensions sheet: three tabs, a search field, and the open tab's list.

use super::{App, Buffer, Rect, centered, footer, frame_box, put, row, rput, s, theme, truncate, u, window};
use crate::tui::l10n::tr;
use crate::tui::manage::{Market, Row, TABS, Tab};
use crate::tui::view::field_line;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, m: &Market) {
    let t = theme();
    let r = centered(area, 96, 30);
    let title = format!("{} · {}", tr("Extensions"), app.host);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let mut x = inner.x;
    for (tab, label) in TABS {
        let style = if tab == m.tab { t.btn_primary } else { t.secondary.patch(t.btn) };
        let tab_label = tr(label);
        x = put(buf, x, inner.y, 20, &format!(" {tab_label} "), style) + 1;
    }
    let search = Rect::new(x + 2, inner.y, inner.right().saturating_sub(x + 2), 1);
    let placeholder = match m.tab {
        Tab::Connectors => tr("search the MCP registry"),
        _ => tr("filter"),
    };
    field_line(buf, search, inner.y, &m.query, &placeholder, true);
    let source = match m.tab {
        Tab::Agents => tr("Agents come from the ACP registry and install on {name}."),
        Tab::Connectors => tr("Connectors come from the official MCP Registry and install on {name}."),
        Tab::Skills => tr("Skills come from Anthropic and install on {name}."),
    };
    let source = source.replace("{name}", &app.host);
    put(buf, inner.x, inner.y + 1, inner.width, &truncate(&source, usize::from(inner.width)), t.dim.patch(t.panel));
    let rows = app.market_rows(m);
    // Tabs, source line, then the list; under it the error and the keys.
    let list_h = usize::from(inner.height.saturating_sub(5));
    let mut y = inner.y + 2;
    if App::market_stale(m) {
        put(buf, inner.x, y, inner.width, &tr("↵ searches the registry"), t.secondary.patch(t.panel));
        y += 1;
    }
    for (i, item) in rows.iter().enumerate().skip(window(m.cursor, list_h)).take(list_h) {
        let base = row(buf, inner, y, i == m.cursor);
        let (mark, name, detail, right): (&str, String, String, String) = match item {
            Row::Node => (
                "+",
                tr("Add Node.js").into_owned(),
                tr("Some agents need it to run. Codync adds a copy just for them.").into_owned(),
                String::new(),
            ),
            Row::Agent(b) => {
                let status = match (b["installed"] == true, b["signedIn"].as_bool()) {
                    (false, _) if b["curated"] == true => "not installed",
                    _ if b["node"] == "missing" => "needs Node.js",
                    (_, Some(true)) => "signed in",
                    (true, Some(false)) => "not signed in",
                    (true, None) => "installed",
                    (false, _) => "sets up on first use",
                };
                let status = tr(status);
                let mark = if b["signedIn"] == true || b["installed"] == true { "✓" } else { "↓" };
                (mark, s(b, "name").into(), s(b, "description").into(), status.into_owned())
            }
            Row::Installed(c) => {
                let right = match s(c, "auth") {
                    "signedOut" => tr("↵ sign in"),
                    _ if c["kind"] == "composio" => std::borrow::Cow::Borrowed("Composio"),
                    _ => tr("installed"),
                };
                let detail = c["command"].as_str().or(c["url"].as_str()).unwrap_or(s(c, "description"));
                ("✓", s(c, "name").into(), detail.into(), right.into_owned())
            }
            Row::Custom => (
                "+",
                tr("Custom connector").into_owned(),
                tr("Paste any MCP server's config").into_owned(),
                String::new(),
            ),
            Row::Registry(c) => ("↓", s(c, "title").into(), s(c, "description").into(), String::new()),
            Row::OwnSkill(v) => ("✓", s(v, "name").into(), s(v, "description").into(), tr("installed").into_owned()),
            Row::Skill(v) => ("↓", s(v, "name").into(), s(v, "description").into(), String::new()),
        };
        let ok = mark == "✓";
        put(buf, inner.x, y, 2, mark, if ok { t.green } else { t.dim }.patch(base));
        let nx = put(buf, inner.x + 2, y, 26, &truncate(&name, 26), t.bold.patch(base)) + 2;
        let rx = rput(buf, inner.right(), y, &right, t.dim.patch(base));
        let room = usize::from(rx.saturating_sub(nx + 1));
        put(buf, nx, y, u(room), &truncate(&detail.replace('\n', " "), room), t.secondary.patch(base));
        y += 1;
    }
    if rows.is_empty() {
        let msg = if m.loading { tr("Loading…") } else { tr("Nothing matches") };
        put(buf, inner.x, y, inner.width, &msg, t.dim.patch(t.panel));
    }
    let keys = match m.tab {
        Tab::Agents => tr("tab switch · ↵ install / sign in · ^r refresh · esc close"),
        Tab::Connectors => tr("tab switch · ↵ add / sign in · ^x remove · ^r refresh · esc"),
        Tab::Skills => tr("tab switch · ↵ details · ^x remove · esc close"),
    };
    let loading = m.loading && m.tab == Tab::Connectors && !rows.is_empty();
    let registry = tr("Loading the registry…");
    footer(buf, inner, m.error.as_deref().or(loading.then_some(registry.as_ref())), &keys);
}
