//! The approval rules sheet: the list, the pattern being typed and the command being tried.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::{Mode, QUICK_CLASSES, RulesSheet};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, restyle, u};
use super::style::theme;

const WARNING: &str = "An allow rule never approves a command that runs several (; && || | ` $( > < or a new line): such a command asks instead.";

pub(super) fn rules(buf: &mut Buffer, area: Rect, s: &RulesSheet) {
    let t = theme();
    let rows = s.rules.len().max(1) + 9;
    let r = centered(area, 86, u(rows.clamp(14, usize::from(area.height.saturating_sub(2)).max(14))));
    let title = format!("{} · {}", tr("Approval rules"), s.bot_name);
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 8 {
        return;
    }
    let mut y = inner.y + 1;
    put(
        buf,
        inner.x,
        y,
        inner.width,
        &tr("The first rule that matches decides, before the Approvals setting."),
        t.secondary.patch(t.panel),
    );
    y += 1;
    put(buf, inner.x, y, inner.width, &truncate(&tr(WARNING), usize::from(inner.width)), t.dim.patch(t.panel));
    y += 1;
    let profile = s.profile.as_deref().map_or_else(|| tr("your own rules").into_owned(), str::to_owned);
    let changed = if s.profile_changed { " (P replaced the rules; save to keep)" } else { "" };
    put(
        buf,
        inner.x,
        y,
        inner.width,
        &truncate(&format!("{}: {profile}{changed}", tr("Security profile")), usize::from(inner.width)),
        t.secondary.patch(t.panel),
    );
    y += 2;
    if s.rules.is_empty() {
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &tr("No rules: the Approvals setting decides everything."),
            t.secondary.patch(t.panel),
        );
        y += 1;
    }
    let room = usize::from(inner.bottom().saturating_sub(y)).saturating_sub(6).max(1);
    let off = s.sel.saturating_sub(room - 1);
    for (i, rule) in s.rules.iter().enumerate().skip(off).take(room) {
        let selected = i == s.sel;
        if selected {
            restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width + 2, 1), t.sel);
        }
        let base = if selected { t.sel } else { t.panel };
        let pattern = if selected && s.mode == Mode::Pattern { s.editor.text.as_str() } else { rule.pattern.as_str() };
        let line = format!(
            "{:>2}  {:<5} {:<8} {}",
            i + 1,
            rule.decision,
            rule.kind.as_deref().unwrap_or("any"),
            if pattern.is_empty() { "(type a pattern)" } else { pattern }
        );
        put(buf, inner.x, y, inner.width, &line, t.text.patch(base));
        y += 1;
    }
    let bottom = inner.bottom();
    let test_y = bottom.saturating_sub(4);
    if let Some(on) = s.quick_on {
        let state = if on { "on" } else { "off" };
        let classes: Vec<String> = QUICK_CLASSES
            .iter()
            .map(|(class, label)| {
                format!("{label}: {}", s.policies.get(*class).and_then(|p| p.as_str()).unwrap_or("-"))
            })
            .collect();
        let line = format!("{} {state} · {}", tr("Quick rules"), classes.join(" · "));
        put(
            buf,
            inner.x,
            test_y - 1,
            inner.width,
            &truncate(&line, usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
    }
    let tester = if s.mode == Mode::Test { s.editor.text.as_str() } else { "" };
    let prompt =
        if s.mode == Mode::Test { format!("{} {tester}", tr("Try:")) } else { tr("Try a command: T").into_owned() };
    put(buf, inner.x, test_y, inner.width, &prompt, if s.mode == Mode::Test { t.bold } else { t.dim }.patch(t.panel));
    if let Some(answer) = &s.answer {
        put(
            buf,
            inner.x,
            test_y + 1,
            inner.width,
            &truncate(answer, usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
    }
    if let Some(error) = &s.error {
        put(buf, inner.x, test_y + 2, inner.width, &truncate(error, usize::from(inner.width)), t.amber.patch(t.panel));
    }
    let footer = match s.mode {
        Mode::Pattern => "↵ keep the pattern · esc cancel",
        Mode::Test => "↵ test · esc back",
        Mode::List if s.saving => "saving…",
        Mode::List if s.quick_on.is_some() => {
            "a add · ↵ edit · d del · K J move · m t · P profile · Q quick · 1 2 3 · T try · ^s save"
        }
        Mode::List => "a add · ↵ edit · d delete · K J move · m decision · t tool · P profile · T try · ^s save",
    };
    put(buf, inner.x, bottom - 1, inner.width, &truncate(&tr(footer), usize::from(inner.width)), t.dim.patch(t.panel));
}

/// "Always allow" on a card: the rule it would keep, which the person may edit, or why there is none.
pub(super) fn always(buf: &mut Buffer, area: Rect, s: &super::super::app::AlwaysSheet) {
    let t = theme();
    let r = centered(area, 78, 10);
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Always allow").as_ref(), t.text)));
    if inner.height < 6 {
        return;
    }
    let y = inner.y + 1;
    let width = usize::from(inner.width);
    if let Some(why) = &s.refusal {
        put(buf, inner.x, y, inner.width, &truncate(&tr(why), width), t.secondary.patch(t.panel));
    } else {
        let note = tr("This rule is kept for the bot. Edit it to cover more: * is any text.");
        put(buf, inner.x, y, inner.width, &note, t.secondary.patch(t.panel));
        put(buf, inner.x, y + 2, inner.width, &truncate(&s.editor.text, width), t.bold.patch(t.panel));
    }
    if let Some(error) = &s.error {
        put(buf, inner.x, inner.y + 5, inner.width, &truncate(error, width), t.amber.patch(t.panel));
    }
    let footer =
        if s.refusal.is_some() { "o allow once · esc cancel" } else { "↵ keep the rule and allow · esc cancel" };
    put(buf, inner.x, inner.bottom() - 1, inner.width, &tr(footer), t.dim.patch(t.panel));
}
