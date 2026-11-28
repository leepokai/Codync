//! Permission cards and their decided lines.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::app::{App, Bot, Click, Entry, tilde};
use super::super::l10n::{fill, tr};
use super::super::md::{self, truncate, width as w};
use super::buffer::u;
use super::style::{spin, theme};
use super::time::clock;
use super::transcript::Built;
use crate::chat::decision::{Answers, Status};

fn has_duplicate_kind(opts: &[(String, String, String)], kind: &str) -> bool {
    opts.iter().filter(|(_, _, k)| k == kind).count() > 1
}

/// An answer's label: Codync's own words when each kind appears once, or the agent's own option name when multiple
/// options share a kind so their scopes stay distinguishable.
fn option_label(name: &str, kind: &str, duplicate_kind: bool) -> String {
    let trimmed = name.trim();
    if duplicate_kind && !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    let label = match kind {
        "allow_once" => "Allow once",
        "allow_always" => "Always allow",
        "reject_once" => "Deny",
        "reject_always" => "Never",
        _ => return name.to_owned(),
    };
    tr(label).into_owned()
}

/// What a decided card says was answered; keeps the agent's own option name when multiple options share the chosen kind.
fn outcome_label(name: &str, kind: &str, duplicate_kind: bool) -> String {
    let trimmed = name.trim();
    if duplicate_kind && !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    let label = match kind {
        "allow_once" => "Allowed once",
        "allow_always" => "Always allowed",
        "reject_once" => "Denied",
        "reject_always" => "Never allowed",
        _ => return name.to_owned(),
    };
    tr(label).into_owned()
}

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

/// A bot's choice card (`request_decision`): the bot's name heads it, its question in bold, then a button per option
/// (the suggested one marked) and a pointer to the message box for the user's own words. A thread's card says where it
/// comes from and has no such pointer: a typed message is for the bot in the chat, and only a pick answers the thread.
pub(super) fn decision_card(out: &mut Built, app: &App, e: &Entry, b: &Bot, width: usize) {
    let t = theme();
    let answering = app.answering.get(&e.id);
    let question = e.data["question"].as_str().unwrap_or_default();
    let bw = width.saturating_sub(1).max(20);
    let inner = bw.saturating_sub(4);
    card_head(out, &format!(" ◆ {} ", truncate(&b.name, inner.saturating_sub(4))), bw);
    let from = background_line(e);
    if let Some(from) = &from {
        card_row(out, vec![Span::styled(truncate(from, inner), t.dim)], inner);
    }
    for l in md::wrap(&[Span::styled(question.to_owned(), t.text.add_modifier(Modifier::BOLD))], inner, &[], &[]) {
        card_row(out, l.spans, inner);
    }
    card_row(out, vec![], inner);
    let recommended = e.data["recommended"].as_u64().and_then(|i| usize::try_from(i).ok());
    for (i, option) in e.choices().iter().enumerate() {
        let number = format!("F{}", i + 1);
        let key = format!(" {} ", if answering == Some(option) { spin(app) } else { number.as_str() });
        let suggested = recommended == Some(i);
        let label = if suggested { format!("{option} · {} ", tr("Suggested")) } else { format!("{option} ") };
        let style = if answering.is_some_and(|a| a != option) {
            t.dim.patch(t.btn)
        } else if suggested || i == 0 {
            t.btn_primary
        } else {
            t.text.patch(t.btn)
        };
        let content = [Span::styled(key, style.add_modifier(Modifier::BOLD)), Span::styled(label, style)];
        for line in md::wrap(&content, inner, &[], &[]) {
            let li = out.lines.len();
            card_row(out, line.spans, inner);
            out.buttons.push((li, 3, u(inner), Click::Option { entry: e.id.clone(), option: option.clone() }));
        }
    }
    // The message box below takes the user's own words, unless only an option answers the card.
    if from.is_none() && Answers::of(&e.data) == Answers::OptionsOrOwnWords {
        card_row(out, vec![Span::styled(format!(" ↓ {}", tr("Something else")), t.dim)], inner);
    }
    card_foot(out, bw);
}

/// The head line of a card, in the approval card's amber box.
fn card_head(out: &mut Built, head: &str, bw: usize) {
    let t = theme();
    let side = bw.saturating_sub(w(head) + 3);
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("╭─", t.amber),
        Span::styled(head.to_owned(), t.amber.add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}╮", "─".repeat(side)), t.amber),
    ]));
}

/// One line inside a card.
fn card_row(out: &mut Built, content: Vec<Span<'static>>, inner: usize) {
    let t = theme();
    let used: usize = content.iter().map(|s| w(&s.content)).sum();
    let mut spans = vec![Span::raw(" "), Span::styled("│ ", t.amber)];
    spans.extend(content);
    spans.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
    spans.push(Span::styled(" │", t.amber));
    out.lines.push(Line::from(spans));
}

/// The card's last line.
fn card_foot(out: &mut Built, bw: usize) {
    let t = theme();
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("╰{}╯", "─".repeat(bw.saturating_sub(2))), t.amber),
    ]));
}

/// A choice card's line once it is decided: what the user picked, or that it went before they did.
pub(super) fn chosen_line(out: &mut Built, e: &Entry) {
    let t = theme();
    let question = e.data["question"].as_str().unwrap_or_default();
    let (g, gs, what) = match (Status::of(&e.data), e.data["selected"].as_str()) {
        (Some(Status::Answered), Some(chosen)) => ("✓", t.green, chosen.to_owned()),
        _ => ("·", t.dim, tr("No answer — the bot moved on").into_owned()),
    };
    out.lines.push(Line::from(vec![
        Span::styled(format!(" {g} "), gs),
        Span::styled(what, t.secondary),
        Span::styled(format!(" · {}", truncate(question, 60)), t.dim),
        Span::styled(format!(" · {}", clock(e.created_at)), t.dim),
    ]));
}

/// Where what a background thread posts in the chat (a card, a file, an artifact) comes from: `Work ·
/// <its task's title>`; none for the chat's own.
pub(super) fn background_line(e: &Entry) -> Option<String> {
    let thread = e.data["thread"]["title"].as_str()?;
    Some(format!("{} · {thread}", tr("Work")))
}

pub(super) fn permission_card(out: &mut Built, app: &App, e: &Entry, b: &Bot, width: usize) {
    let t = theme();
    let home = app.home.as_str();
    // The option whose answer is on its way: it spins, the others dim.
    let answering = app.answering.get(&e.id);
    let bw = width.saturating_sub(1).max(20);
    let inner = bw.saturating_sub(4);
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let template = tr("{name} wants to: {title}");
    let room = inner.saturating_sub(w(&fill(&template, &[("name", &b.name), ("title", "")])) + 4);
    let head = format!(" ◆ {} ", fill(&template, &[("name", &b.name), ("title", &truncate(title, room))]));
    card_head(out, &head, bw);
    if let Some(from) = background_line(e) {
        card_row(out, vec![Span::styled(truncate(&from, inner), t.dim)], inner);
    }
    if let Some(cmd) = e.data["command"].as_str().filter(|c| !c.is_empty()) {
        for (i, l) in md::wrap(&[Span::styled(cmd.to_owned(), t.bold)], inner.saturating_sub(2), &[], &[])
            .into_iter()
            .take(6)
            .enumerate()
        {
            let mut c = vec![Span::styled(if i == 0 { "$ " } else { "  " }, t.dim)];
            c.extend(l.spans);
            card_row(out, c, inner);
        }
        let cwd = tilde(e.data["cwd"].as_str().unwrap_or(&b.cwd), home);
        let template = tr("in {path}");
        let room = inner.saturating_sub(w(&fill(&template, &[("path", "")])) + 2);
        let row = fill(&template, &[("path", &truncate(&cwd, room))]);
        card_row(out, vec![Span::styled(format!("  {row}"), t.dim)], inner);
    }
    for d in e.data["diffs"].as_array().into_iter().flatten().take(3) {
        let path = tilde(d["path"].as_str().unwrap_or_default(), home);
        let stat = format!("+{} −{}", d["added"].as_u64().unwrap_or(0), d["removed"].as_u64().unwrap_or(0));
        let p = truncate(&path, inner.saturating_sub(w(&stat) + 2));
        let pad = inner.saturating_sub(w(&p) + w(&stat));
        card_row(
            out,
            vec![Span::styled(p, t.bold), Span::raw(" ".repeat(pad)), Span::styled(stat, t.secondary)],
            inner,
        );
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
            card_row(out, vec![Span::styled(padded, s)], inner);
        }
        if n > 6 {
            let more = format!("… {} more lines{}", n - 6, if out.thread { "" } else { " · t shows the whole diff" });
            card_row(out, vec![Span::styled(more, t.dim)], inner);
        }
    }
    if let Some(detail) = e.data["detail"].as_str().filter(|d| !d.trim().is_empty() && e.data["command"].is_null()) {
        // As the apps show it: wrapped, up to the length a team change's card is held to.
        let detail: String = detail.chars().take(crate::chat::manage::MAX_CARD_DETAIL).collect();
        for l in md::wrap(&[Span::styled(detail, t.secondary)], inner, &[], &[]) {
            card_row(out, l.spans, inner);
        }
    }
    card_row(out, vec![], inner);
    // The answers as the choice card's rows, one tone, each with its key.
    let opts = e.options();
    let keys = option_keys(&opts);
    for (i, (id, name, kind)) in opts.iter().enumerate() {
        let key = format!(" {} ", if answering == Some(id) { spin(app) } else { keys[i].as_str() });
        let style = if answering.is_some_and(|a| a != id) { t.dim.patch(t.btn) } else { t.text.patch(t.btn) };
        let content = [
            Span::styled(key, style.add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} ", option_label(name, kind, has_duplicate_kind(&opts, kind))), style),
        ];
        for line in md::wrap(&content, inner, &[], &[]) {
            let li = out.lines.len();
            card_row(out, line.spans, inner);
            out.buttons.push((li, 3, u(inner), Click::Option { entry: e.id.clone(), option: id.clone() }));
        }
    }
    card_row(out, vec![Span::styled(tr("N reject + say why").into_owned(), t.dim)], inner);
    card_foot(out, bw);
}

pub(super) fn decided_line(out: &mut Built, e: &Entry) {
    let t = theme();
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let sel = e.data["selected"].as_str();
    let opts = e.options();
    let chosen = sel.and_then(|s| opts.iter().find(|o| o.0 == s));
    let (g, gs, what) = match (e.data["status"].as_str(), chosen) {
        (Some("answered"), Some((_, name, kind))) if kind.starts_with("allow") => {
            ("✓", t.green, outcome_label(name, kind, has_duplicate_kind(&opts, kind)))
        }
        (Some("answered"), Some((_, name, kind))) => {
            ("×", t.red, outcome_label(name, kind, has_duplicate_kind(&opts, kind)))
        }
        (Some("expired"), _) => ("·", t.dim, tr("Expired — the agent moved on").into_owned()),
        _ => ("·", t.dim, tr("Cancelled").into_owned()),
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

    #[tokio::test]
    async fn a_threads_card_says_it_comes_from_the_background() {
        use serde_json::json;
        let app = crate::tui::app::test_app();
        let bot = Bot::parse(&json!({"id": "a", "name": "Ada"})).expect("a bot with an id parses");
        let card = |data: serde_json::Value| {
            let entry = json!({"id": "p", "botId": "a", "seq": 1, "kind": "permission", "data": data});
            let (_, e) = Entry::parse(&entry).expect("an entry parses");
            let mut out = Built::default();
            permission_card(&mut out, &app, &e, &bot, 60);
            out.lines.iter().map(ToString::to_string).collect::<Vec<_>>()
        };
        let own = card(json!({"status": "pending", "title": "Run tests", "options": []}));
        let thread = card(json!({"status": "pending", "title": "Run tests", "options": [],
            "thread": {"id": "thread-1", "title": "Fix login"}}));
        assert_eq!(thread.len(), own.len() + 1);
        assert!(thread[1].contains("Work · Fix login"), "{thread:?}");
        assert!(!own.iter().any(|l| l.contains("Work")));
    }

    #[tokio::test]
    async fn a_threads_choice_card_says_where_it_comes_from_and_takes_no_typed_answer() {
        use serde_json::json;
        let app = crate::tui::app::test_app();
        let bot = Bot::parse(&json!({"id": "a", "name": "Ada"})).expect("a bot with an id parses");
        let card = |more: serde_json::Value| {
            let mut data = json!({"decision": true, "status": "pending", "question": "Squash or merge?",
                "choices": ["Squash", "Merge"]});
            data.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
            let entry = json!({"id": "d", "botId": "a", "seq": 1, "kind": "notice", "data": data});
            let (_, e) = Entry::parse(&entry).expect("an entry parses");
            let mut out = Built::default();
            decision_card(&mut out, &app, &e, &bot, 60);
            (out.lines.iter().map(ToString::to_string).collect::<Vec<_>>(), out.buttons.len())
        };
        let (own, own_buttons) = card(json!({}));
        let (thread, thread_buttons) = card(json!({"thread": {"id": "thread-1", "title": "Fix login"}, "task": "i"}));
        assert!(own.iter().any(|l| l.contains("↓ Something else")), "{own:?}");
        let (options_only, _) = card(json!({"ownWords": false}));
        assert!(!options_only.iter().any(|l| l.contains("Something else")), "only an option answers it");
        assert!(!own.iter().any(|l| l.contains("Work")));
        assert!(thread[1].contains("Work · Fix login"), "{thread:?}");
        assert!(thread[2].contains("Squash or merge?"), "{thread:?}");
        assert!(!thread.iter().any(|l| l.contains("Something else")), "only a pick answers a thread's card");
        assert_eq!((own_buttons, thread_buttons), (2, 2), "the options are pressed as on any card");
    }

    #[test]
    fn labels_keep_an_agents_own_option_name() {
        assert_eq!(option_label("Custom", "other", false), "Custom");
        assert_eq!(outcome_label("Custom", "other", false), "Custom");
        assert_eq!(option_label("x", "reject_once", false), tr("Deny"));
        assert_eq!(outcome_label("x", "reject_once", false), tr("Denied"));
        assert_eq!(option_label("Allow for this session", "allow_always", true), "Allow for this session");
        assert_eq!(outcome_label("Allow for this session", "allow_always", true), "Allow for this session");
        assert_eq!(option_label("   ", "allow_always", true), tr("Always allow"));
    }

    #[tokio::test]
    async fn duplicate_allow_always_options_keep_distinct_agent_names() {
        use serde_json::json;
        let app = crate::tui::app::test_app();
        let bot = Bot::parse(&json!({"id": "a", "name": "Ada"})).expect("a bot with an id parses");
        let options = json!([
            {"optionId": "once", "name": "Allow", "kind": "allow_once"},
            {"optionId": "session", "name": "Allow for this session", "kind": "allow_always"},
            {"optionId": "always", "name": "Allow for all sessions", "kind": "allow_always"},
            {"optionId": "deny", "name": "Reject", "kind": "reject_once"}
        ]);
        let pending = json!({"id": "p", "botId": "a", "seq": 1, "kind": "permission",
            "data": {"status": "pending", "title": "mcp.chat.send_message", "options": options}});
        let (_, e) = Entry::parse(&pending).expect("an entry parses");
        let mut out = Built::default();
        permission_card(&mut out, &app, &e, &bot, 80);
        let text = out.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Allow for this session"), "{text}");
        assert!(text.contains("Allow for all sessions"), "{text}");
        assert!(text.contains(&*tr("Allow once")), "{text}");
        assert!(text.contains(&*tr("Deny")), "{text}");

        let answered = json!({"id": "p", "botId": "a", "seq": 1, "kind": "permission",
            "data": {"status": "answered", "selected": "session", "title": "mcp.chat.send_message", "options": options}});
        let (_, decided_entry) = Entry::parse(&answered).expect("an entry parses");
        let mut decided = Built::default();
        decided_line(&mut decided, &decided_entry);
        let decided_text = decided.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(decided_text.contains("Allow for this session"), "{decided_text}");
    }
}
