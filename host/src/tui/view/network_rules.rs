//! A network's rules in the network sheet: the list, and the form for a new one.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::l10n::tr;
use super::super::manage::{NetworkSheet, RuleField, RuleForm};
use super::buffer::{put, restyle};
use super::network::{line, wrapped};
use super::style::theme;

const FOOTER: &str = "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in.";

pub(super) fn rules(buf: &mut Buffer, body: Rect, n: &NetworkSheet, at: usize) -> &'static str {
    let t = theme();
    let mut y = body.y;
    // Rules belong to one network: say which.
    line(buf, body, &mut y, &format!("{} · {}", tr("Who may message bots here"), n.name()), t.bold);
    y += 1;
    if n.rules().is_empty() {
        line(
            buf,
            body,
            &mut y,
            &tr("No rules: every computer in the network may message every bot here."),
            t.secondary,
        );
    }
    // The rows between the title and the footer, scrolled so the selected rule shows.
    let room = usize::from(body.height).saturating_sub(6).max(1);
    let off = at.saturating_sub(room - 1);
    for (i, rule) in n.rules().iter().enumerate().skip(off).take(room) {
        let sel = i == at;
        if sel {
            restyle(buf, Rect::new(body.x - 1, y, body.width + 2, 1), t.sel);
        }
        let base = if sel { t.sel } else { t.panel };
        put(buf, body.x, y, body.width, &format!("{:>2}  {}", i + 1, n.rule_text(rule)), t.text.patch(base));
        y += 1;
    }
    y += 1;
    wrapped(buf, body, &mut y, &tr(FOOTER), t.secondary);
    if n.rules().is_empty() { "n add rule · esc back" } else { "n add rule · x delete · K J move · esc back" }
}

pub(super) fn rule_form(buf: &mut Buffer, body: Rect, n: &NetworkSheet, form: &RuleForm) -> &'static str {
    let t = theme();
    let choices = n.rule_choices(form);
    let mut y = body.y;
    line(buf, body, &mut y, &tr("Add rule"), t.bold);
    y += 1;
    let label = |c: &[(String, String)], i: usize| c.get(i).map_or_else(String::new, |c| c.1.clone());
    let rows = [
        (RuleField::Access, "Access", tr(if form.deny { "Deny" } else { "Allow" }).into_owned()),
        (RuleField::FromComputer, "From computer", label(&choices.computers, form.from_computer)),
        (RuleField::FromAgent, "From bot", label(&choices.agents, form.from_agent)),
        (RuleField::ToAgent, "To bot", label(&choices.mine, form.to_agent)),
        (RuleField::First, "Put it first", tr(if form.first { "On" } else { "Off" }).into_owned()),
    ];
    for (field, name, value) in rows {
        let active = field == form.field;
        let style = if active { t.bold } else { t.secondary };
        put(buf, body.x, y, 18, &tr(name), style.patch(t.panel));
        put(buf, body.x + 18, y, body.width.saturating_sub(18), &format!("‹ {value} ›"), style.patch(t.panel));
        y += 1;
    }
    y += 1;
    wrapped(buf, body, &mut y, &tr(FOOTER), t.secondary);
    "↵ add · tab next field · ←→ change · esc back"
}
