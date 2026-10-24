//! Permission cards and their decided lines.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::app::{App, Bot, Click, Entry, tilde};
use super::super::md::{self, truncate, width as w};
use super::buffer::u;
use super::style::{spin, theme};
use super::time::clock;
use super::transcript::Built;

/// Keys printed on approval buttons: y / a / n for the usual kinds, digits otherwise.
fn option_keys(opts: &[(String, String, String)]) -> Vec<String> {
    let mut used = vec![];
    opts.iter()
        .enumerate()
        .map(|(i, (_, _, kind))| {
            let k = match kind.as_str() {
                "allow_once" => "y",
                "allow_always" => "a",
                "reject_once" | "reject_always" => "n",
                _ => "",
            };
            let k = if k.is_empty() || used.contains(&k) { (i + 1).to_string() } else { k.to_owned() };
            if k.len() == 1 && k.chars().all(char::is_alphabetic) {
                used.push(match k.as_str() {
                    "y" => "y",
                    "a" => "a",
                    _ => "n",
                });
            }
            k
        })
        .collect()
}

pub(super) fn permission_card(out: &mut Built, app: &App, e: &Entry, b: &Bot, width: usize) {
    let t = theme();
    let home = app.home.as_str();
    // The option whose answer is on its way: it spins, the others dim.
    let answering = app.answering.get(&e.id);
    let bw = width.saturating_sub(1).max(20);
    let inner = bw.saturating_sub(4);
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let head = format!(" ◆ {} wants to: {} ", b.name, truncate(title, inner.saturating_sub(w(&b.name) + 14)));
    let side = bw.saturating_sub(w(&head) + 3);
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("╭─", t.amber),
        Span::styled(head, t.amber.add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}╮", "─".repeat(side)), t.amber),
    ]));
    let row = |content: Vec<Span<'static>>| -> Line<'static> {
        let used: usize = content.iter().map(|s| w(&s.content)).sum();
        let mut spans = vec![Span::raw(" "), Span::styled("│ ", t.amber)];
        spans.extend(content);
        spans.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
        spans.push(Span::styled(" │", t.amber));
        Line::from(spans)
    };
    if let Some(cmd) = e.data["command"].as_str().filter(|c| !c.is_empty()) {
        for (i, l) in md::wrap(&[Span::styled(cmd.to_owned(), t.bold)], inner.saturating_sub(2), &[], &[])
            .into_iter()
            .take(6)
            .enumerate()
        {
            let mut c = vec![Span::styled(if i == 0 { "$ " } else { "  " }, t.dim)];
            c.extend(l.spans);
            out.lines.push(row(c));
        }
        let cwd = tilde(e.data["cwd"].as_str().unwrap_or(&b.cwd), home);
        out.lines.push(row(vec![Span::styled(format!("  in {}", truncate(&cwd, inner.saturating_sub(5))), t.dim)]));
    }
    for d in e.data["diffs"].as_array().into_iter().flatten().take(3) {
        let path = tilde(d["path"].as_str().unwrap_or_default(), home);
        let stat = format!("+{} −{}", d["added"].as_u64().unwrap_or(0), d["removed"].as_u64().unwrap_or(0));
        let p = truncate(&path, inner.saturating_sub(w(&stat) + 2));
        let pad = inner.saturating_sub(w(&p) + w(&stat));
        out.lines.push(row(vec![Span::styled(p, t.bold), Span::raw(" ".repeat(pad)), Span::styled(stat, t.secondary)]));
        let patch = d["patch"].as_str().unwrap_or_default();
        let n = patch.lines().count();
        for l in patch.lines().take(6) {
            let s = if l.starts_with('+') {
                t.added
            } else if l.starts_with('-') {
                t.removed
            } else {
                t.secondary
            };
            let body = truncate(l, inner);
            let padded = format!("{body}{}", " ".repeat(inner.saturating_sub(w(&body))));
            out.lines.push(row(vec![Span::styled(padded, s)]));
        }
        if n > 6 {
            out.lines.push(row(vec![Span::styled(format!("… {} more lines · t shows the whole diff", n - 6), t.dim)]));
        }
    }
    if let Some(detail) = e.data["detail"].as_str().filter(|d| !d.trim().is_empty() && e.data["command"].is_null()) {
        // As the apps show it: wrapped, up to the length a team change's card is held to.
        let detail: String = detail.chars().take(crate::chat::manage::MAX_CARD_DETAIL).collect();
        for l in md::wrap(&[Span::styled(detail, t.secondary)], inner, &[], &[]) {
            out.lines.push(row(l.spans));
        }
    }
    out.lines.push(row(vec![]));
    // Buttons, wrapping onto more lines when narrow.
    let opts = e.options();
    let keys = option_keys(&opts);
    let mut line: Vec<Span<'static>> = vec![];
    let mut used = 0usize;
    let mut pending: Vec<(u16, u16, Click)> = vec![];
    let flush =
        |out: &mut Built, line: &mut Vec<Span<'static>>, used: &mut usize, pending: &mut Vec<(u16, u16, Click)>| {
            let li = out.lines.len();
            out.lines.push(row(std::mem::take(line)));
            for (x, bw, c) in pending.drain(..) {
                out.buttons.push((li, x, bw, c));
            }
            *used = 0;
        };
    for (i, (id, name, kind)) in opts.iter().enumerate() {
        let key = format!(" {} ", if answering == Some(id) { spin(app) } else { keys[i].as_str() });
        let label = format!("{name} ");
        let bw_ = w(&key) + w(&label);
        if used > 0 && used + bw_ + 1 > inner {
            flush(out, &mut line, &mut used, &mut pending);
        }
        let style = if answering.is_some_and(|a| a != id) {
            t.dim.patch(t.btn)
        } else if i == 0 {
            t.btn_primary
        } else if kind.starts_with("reject") {
            t.red.patch(t.btn)
        } else {
            t.text.patch(t.btn)
        };
        if used > 0 {
            line.push(Span::raw(" "));
            used += 1;
        }
        pending.push((u(3 + used), u(bw_), Click::Option { entry: e.id.clone(), option: id.clone() }));
        line.push(Span::styled(key, style.add_modifier(Modifier::BOLD)));
        line.push(Span::styled(label, style));
        used += bw_;
    }
    if !line.is_empty() {
        let hint = "N reject + say why";
        if used + w(hint) + 3 <= inner {
            line.push(Span::raw(" ".repeat(inner - used - w(hint))));
            line.push(Span::styled(hint, t.dim));
        }
        flush(out, &mut line, &mut used, &mut pending);
    }
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("╰{}╯", "─".repeat(bw.saturating_sub(2))), t.amber),
    ]));
}

pub(super) fn decided_line(out: &mut Built, e: &Entry) {
    let t = theme();
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let sel = e.data["selected"].as_str();
    let chosen = sel.and_then(|s| e.options().into_iter().find(|o| o.0 == s));
    let (g, gs, what) = match (e.data["status"].as_str(), chosen) {
        (Some("answered"), Some((_, name, kind))) if kind.starts_with("allow") => ("✓", t.green, name),
        (Some("answered"), Some((_, name, _))) => ("×", t.red, name),
        (Some("expired"), _) => ("·", t.dim, "Expired".to_owned()),
        _ => ("·", t.dim, "Cancelled".to_owned()),
    };
    out.lines.push(Line::from(vec![
        Span::styled(format!(" {g} "), gs),
        Span::styled(what, t.secondary),
        Span::styled(format!(" · {title}"), t.dim),
        Span::styled(format!(" · {}", clock(e.created_at)), t.dim),
    ]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_keys_use_letters_then_digits() {
        let o = |k: &str| (String::new(), String::new(), k.to_owned());
        let keys = option_keys(&[o("allow_once"), o("allow_always"), o("reject_once"), o("reject_always")]);
        assert_eq!(keys, ["y", "a", "n", "4"]);
    }
}
