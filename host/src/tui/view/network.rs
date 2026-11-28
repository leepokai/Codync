//! The network sheet: this computer's network of the user's computers.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use serde_json::Value;

use super::super::app::Editor;
use super::super::l10n::{fill, tr};
use super::super::manage::{CONNECTORS, NetworkSheet, Recovery, Setup, SetupField, Step};
use super::super::md;
use super::buffer::{centered, frame_box, hline, put, put_line, restyle, rput};
use super::network_rules::{rule_form, rules};
use super::overlay::input_line;
use super::style::theme;
use super::time::clock;

pub(super) fn network(buf: &mut Buffer, area: Rect, n: &NetworkSheet) {
    let t = theme();
    let r = centered(area, 84, area.height.saturating_sub(4).min(26));
    let title = format!("{} · {}", tr("Network"), tr("Beta"));
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 8 {
        return;
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    let keys = match &n.step {
        Step::List => list(buf, body, n),
        Step::Create(s) => setup(buf, body, s, false),
        Step::Join(s) => setup(buf, body, s, true),
        Step::Recovery(r) => recovery(buf, body, r),
        Step::Invite(invite) => invite_step(buf, body, n, invite.as_ref()),
        Step::Recover(code) => recover(buf, body, code),
        Step::Remove { computer, cascade } => remove(buf, body, computer, *cascade),
        Step::Leave => leave(buf, body, n),
        Step::Rules(at) => rules(buf, body, n, *at),
        Step::NewRule(form) => rule_form(buf, body, n, form),
    };
    if let Some(e) = &n.error {
        put(buf, inner.x, inner.bottom() - 3, inner.width, e, t.red.patch(t.panel));
    } else if n.busy {
        put(buf, inner.x, inner.bottom() - 3, inner.width, &tr("Working…"), t.dim.patch(t.panel));
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, &format!(" {}", tr(keys)), t.dim.patch(t.panel));
}

fn recovery(buf: &mut Buffer, body: Rect, r: &Recovery) -> &'static str {
    let t = theme();
    let mut y = body.y;
    line(buf, body, &mut y, &tr("Save your recovery code"), t.bold);
    y += 1;
    line(buf, body, &mut y, &r.code, t.bold);
    y += 1;
    let note = tr(
        "It makes another computer the owner of {network} if {name} is lost. It's shown only once and kept on no computer.",
    );
    wrapped(buf, body, &mut y, &fill(&note, &[("network", &r.network), ("name", &r.computer)]), t.secondary);
    "y copy · s I saved it"
}

fn invite_step(buf: &mut Buffer, body: Rect, n: &NetworkSheet, invite: Option<&(String, i64)>) -> &'static str {
    let t = theme();
    let mut y = body.y;
    line(buf, body, &mut y, &tr("Invite a computer"), t.bold);
    y += 1;
    match invite {
        None => line(buf, body, &mut y, &tr("Getting an invite…"), t.dim),
        Some((code, until)) => {
            wrapped(buf, body, &mut y, code, t.bold);
            y += 1;
            let note = fill(
                &tr("One use. Works until {time} while {name} stays on."),
                &[("time", &clock(*until)), ("name", &n.computer())],
            );
            line(buf, body, &mut y, &note, t.secondary);
            line(buf, body, &mut y, &tr("On the other computer, choose Join with invite under Network."), t.secondary);
        }
    }
    "y copy · n new invite · esc back"
}

fn recover(buf: &mut Buffer, body: Rect, code: &Editor) -> &'static str {
    let t = theme();
    let mut y = body.y;
    line(buf, body, &mut y, &tr("Recover ownership"), t.bold);
    wrapped(buf, body, &mut y, &tr("For when the owner is lost: this computer becomes the owner."), t.secondary);
    y += 1;
    input_line(buf, Rect::new(body.x, y, body.width, 1), y, code, "Recovery code", true, false);
    "↵ recover · esc back"
}

fn leave(buf: &mut Buffer, body: Rect, n: &NetworkSheet) -> &'static str {
    let t = theme();
    let mut y = body.y;
    let network = n.name();
    line(buf, body, &mut y, &tr("Leave {network}?").replace("{network}", network), t.red);
    let note = tr(
        "This computer stops being in {network}. The others keep listing it until the owner or an admin removes it there; rejoining needs a new invite.",
    );
    wrapped(buf, body, &mut y, &note.replace("{network}", network), t.text);
    "↵ leave · esc cancel"
}

fn remove(buf: &mut Buffer, body: Rect, computer: &str, cascade: bool) -> &'static str {
    let t = theme();
    let mut y = body.y;
    line(buf, body, &mut y, &tr("Remove {name} from the network?").replace("{name}", computer), t.red);
    line(buf, body, &mut y, &tr("It can't come back with its current key."), t.text);
    y += 1;
    let mark = if cascade { "◉" } else { "○" };
    line(buf, body, &mut y, &format!("{mark} {}", tr("Also remove the computers it invited")), t.text);
    line(buf, body, &mut y, &format!("  {}", tr("For a stolen computer.")), t.secondary);
    "↵ remove · c switches · esc cancel"
}

/// The computers, or how to start; then the cloud assistants switch. Returns the key hints.
fn list(buf: &mut Buffer, body: Rect, n: &NetworkSheet) -> &'static str {
    let t = theme();
    let mut y = body.y;
    if n.view.is_none() {
        put(buf, body.x, y, body.width, &tr("Loading…"), t.dim.patch(t.panel));
        return "esc close";
    }
    if let Some(current) = n.current() {
        y = networks_line(buf, body, y, n);
        let me = n.computer();
        let role = role_name(n.role());
        let connector = current["connector"].as_str().unwrap_or("public");
        put(buf, body.x, y, body.width, &format!("{me} · {role} · {connector}"), t.secondary.patch(t.panel));
        y += 2;
        let rule_rows = n.rules().len().min(3);
        // A gap, the sessions' title, at least one of them and, with more, how many.
        let session_rows = match n.sessions().len() {
            0 => 0,
            1 => 3,
            _ => 4,
        };
        let room = usize::from(body.height).saturating_sub(9 + rule_rows + session_rows).max(1);
        let off = n.cursor.saturating_sub(room.saturating_sub(1));
        for (i, c) in n.computers().iter().enumerate().skip(off).take(room) {
            computer_row(buf, body, y, c, i == n.cursor);
            y += 1;
        }
        if rule_rows > 0 {
            y += 1;
            put(buf, body.x, y, body.width, &tr("Who may message bots here"), t.bold.patch(t.panel));
            y += 1;
            for rule in n.rules().iter().take(rule_rows) {
                line(buf, body, &mut y, &format!("  {}", n.rule_text(rule)), t.secondary);
            }
            if n.rules().len() > rule_rows {
                line(buf, body, &mut y, &format!("  {}", tr("w shows them all")), t.dim);
            }
        }
    } else {
        put(buf, body.x, y, body.width, &tr("Not in a network."), t.bold.patch(t.panel));
        y += 1;
        let note = tr("Connect your computers so bots on one can message bots on another.");
        wrapped(buf, body, &mut y, &note, t.secondary);
    }
    // The cloud switch keeps a gap, its line and one of its note's.
    let switch = n.says("assistantsAvailable");
    y = sessions(buf, body, y + 1, if switch { 3 } else { 0 }, n);
    if switch {
        assistants(buf, body, y + 1, n.says("assistants"));
    }
    match (n.joined(), n.may("canInvite"), n.may("canRecover")) {
        (false, ..) => "c create network · J join with invite · esc close",
        (true, true, false) => "i invite · p make admin · x remove · w rules · c new · J join · L leave · esc",
        (true, true, true) => "i invite · x remove · o recover · w rules · c new · J join · L leave · esc",
        (true, false, _) => "o recover ownership · w rules · c new · J join · L leave · esc",
    }
}

/// The networks this computer is in, the shown one bracketed; switching only when there are several.
fn networks_line(buf: &mut Buffer, body: Rect, y: u16, n: &NetworkSheet) -> u16 {
    let t = theme();
    let mut x = body.x;
    for (i, net) in n.networks().iter().enumerate() {
        let name = net["name"].as_str().unwrap_or_default();
        let (text, style) =
            if i == n.net { (format!("‹ {name} ›"), t.bold) } else { (format!("  {name}  "), t.secondary) };
        x = put(buf, x, y, body.right().saturating_sub(x), &text, style.patch(t.panel));
    }
    if n.networks().len() > 1 {
        rput(buf, body.right(), y, &tr("←→ network"), t.dim.patch(t.panel));
    }
    y + 1
}

/// This computer's coding agents' sessions, one line each in the host's order (running, then the
/// most recent, the older ones dimmed last) as many as fit above `below` rows, then how many more
/// there are.
fn sessions(buf: &mut Buffer, body: Rect, mut y: u16, below: u16, n: &NetworkSheet) -> u16 {
    let t = theme();
    let all = n.sessions();
    if all.is_empty() {
        return y.saturating_sub(1);
    }
    // The heading only with room for a session under it, above what keeps its place below.
    if body.bottom().saturating_sub(y + below) < 2 {
        return y.saturating_sub(1);
    }
    line(buf, body, &mut y, &tr("Agents"), t.bold);
    let room = usize::from(body.bottom().saturating_sub(y + below));
    // The last row says how many more when they don't all fit.
    let fit = if all.len() > room { room.saturating_sub(1) } else { all.len() };
    for s in &all[..fit] {
        let runs = s["runs"] == true;
        let address = s["address"].as_str().unwrap_or_default();
        let title = s["title"].as_str();
        let state = match s["state"].as_str() {
            Some("working") => Some(tr("Working")),
            Some("idle") => Some(tr("Idle")),
            _ => None,
        };
        let tier = match s["tier"].as_str() {
            Some("pull") => Some(tr("Checks its mail")),
            Some("turn") => Some(tr("Told of mail at its next step")),
            Some("push") => Some(tr("Told of mail as it comes")),
            _ => None,
        };
        let mut detail: Vec<String> = [title.map(|_| address.into()), state.map(Into::into), tier.map(Into::into)]
            .into_iter()
            .flatten()
            .collect();
        let name = if s["older"] == true { t.dim } else { t.text };
        // Two columns for the dot and its space, two between title and details: the address goes in whole first, then
        // the state and the mail text as room allows, and the title gets what is left (12 to 34).
        let fixed = 2 + 2 + 12;
        let width = |d: &[String]| u16::try_from(md::width(&d.join(" · "))).unwrap_or(u16::MAX);
        while detail.len() > 1 && fixed + width(&detail) > body.width {
            detail.pop();
        }
        let detail = detail.join(" · ");
        // No state, no dot: unknown isn't offline.
        let dot = if s["state"].is_null() {
            " "
        } else if runs {
            "●"
        } else {
            "○"
        };
        let x = put(buf, body.x, y, 2, dot, if runs { t.text } else { t.dim }.patch(t.panel)) + 1;
        let rest = width(std::slice::from_ref(&detail));
        let room = body.width.saturating_sub(2 + 2 + rest).clamp(12, 34);
        let x =
            put(buf, x, y, room + 2, &md::truncate(title.unwrap_or(address), usize::from(room)), name.patch(t.panel));
        let x = x + 2;
        put(buf, x, y, body.right().saturating_sub(x), &detail, t.secondary.patch(t.panel));
        y += 1;
    }
    if fit < all.len() {
        line(
            buf,
            body,
            &mut y,
            &fill(&tr("More sessions: {count}"), &[("count", &(all.len() - fit).to_string())]),
            t.dim,
        );
    }
    y
}

/// The cloud assistants switch, for a computer in a Codync account (only there can they reach it).
fn assistants(buf: &mut Buffer, body: Rect, mut y: u16, on: bool) {
    let t = theme();
    if y >= body.bottom() {
        return;
    }
    put(buf, body.x, y, body.width, &tr("Cloud assistants"), t.text.patch(t.panel));
    let x = rput(buf, body.right(), y, &format!("  {}", tr("a switches")), t.dim.patch(t.panel));
    let state = if on { format!("◉ {}", tr("On")) } else { format!("○ {}", tr("Off")) };
    rput(buf, x, y, &state, if on { t.text } else { t.secondary }.patch(t.panel));
    y += 1;
    let about = if on {
        tr(
            "ChatGPT or Claude in your Codync account can message this computer's bots. Codync's cloud can read these calls.",
        )
    } else {
        tr("Let ChatGPT or Claude message this computer's bots through your Codync account.")
    };
    wrapped(buf, body, &mut y, &about, t.secondary);
}

fn computer_row(buf: &mut Buffer, body: Rect, y: u16, c: &Value, sel: bool) {
    let t = theme();
    let base = if sel { t.sel } else { t.panel };
    if sel {
        restyle(buf, Rect::new(body.x - 1, y, body.width + 2, 1), t.sel);
    }
    let name = c["name"].as_str().unwrap_or_default();
    let state = if c["me"] == true {
        tr("This computer")
    } else if c["online"] == true {
        format!("● {}", tr("Online")).into()
    } else {
        format!("○ {}", tr("Away")).into()
    };
    let bots: Vec<&str> = c["agents"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
    let offered = if bots.is_empty() { tr("No bots").into_owned() } else { bots.join(", ") };
    put(buf, body.x, y, 2, if sel { "▸" } else { " " }, t.secondary.patch(base));
    put(buf, body.x + 2, y, 20, name, t.bold.patch(base));
    put(buf, body.x + 22, y, 8, &role_name(c["role"].as_str().unwrap_or_default()), t.text.patch(base));
    put(buf, body.x + 30, y, 16, &state, t.secondary.patch(base));
    put(buf, body.x + 46, y, body.width.saturating_sub(46), &md::truncate(&offered, 60), t.secondary.patch(base));
}

fn role_name(role: &str) -> String {
    tr(match role {
        "owner" => "Owner",
        "admin" => "Admin",
        _ => "Member",
    })
    .into_owned()
}

fn setup(buf: &mut Buffer, body: Rect, s: &Setup, join: bool) -> &'static str {
    let t = theme();
    let mut y = body.y;
    let title = if join { "Join with invite" } else { "Create network" };
    put(buf, body.x, y, body.width, &tr(title), t.bold.patch(t.panel));
    y += 2;
    for field in s.fields(join) {
        let active = field == s.field;
        let label = match field {
            SetupField::Invite => "Invite",
            SetupField::Network => "Network name",
            SetupField::Name => "Computer name",
            SetupField::Connector => "Connector",
            SetupField::Relay => "Relay address",
        };
        let style = if active { t.bold } else { t.secondary };
        put(buf, body.x, y, 18, &tr(label), style.patch(t.panel));
        let r = Rect::new(body.x + 18, y, body.width.saturating_sub(18), 1);
        match field {
            SetupField::Invite => {
                input_line(buf, r, y, &s.invite, "Paste the invite from the other computer.", active, false);
            }
            SetupField::Network => {
                let hint = if join { "From the invite" } else { "home" };
                input_line(buf, r, y, &s.network, hint, active, false);
            }
            SetupField::Name => input_line(buf, r, y, &s.name, "Host name", active, false),
            SetupField::Relay => input_line(buf, r, y, &s.relay, "https://relay.example.com", active, false),
            SetupField::Connector => {
                let shown = format!("‹ {} ›", tr(CONNECTORS[s.connector].0));
                put(buf, r.x, y, r.width, &shown, if active { t.bold } else { t.text }.patch(t.panel));
            }
        }
        y += 1;
    }
    y += 1;
    wrapped(buf, body, &mut y, &tr(CONNECTORS[s.connector].2), t.secondary);
    "↵ done · tab next field · ←→ connector · esc back"
}

pub(super) fn line(buf: &mut Buffer, body: Rect, y: &mut u16, text: &str, style: Style) {
    if *y < body.bottom() {
        put(buf, body.x, *y, body.width, text, style.patch(theme().panel));
        *y += 1;
    }
}

pub(super) fn wrapped(buf: &mut Buffer, body: Rect, y: &mut u16, text: &str, style: Style) {
    let t = theme();
    let width = usize::from(body.width);
    for l in md::wrap(&[Span::styled(text.to_owned(), style.patch(t.panel))], width, &[], &[]) {
        if *y >= body.bottom() {
            return;
        }
        put_line(buf, body.x, *y, body.width, &l);
        *y += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHORT: u16 = 17;

    #[test]
    fn sessions_list_in_the_hosts_order_and_say_how_many_more_dont_fit() {
        let all = json!([
            {"address": "billing-claude", "title": "Fix refunds", "state": "working", "runs": true, "tier": "pull"},
            {"address": "web-codex", "state": "offline", "runs": false},
            {"address": "infra-codex", "older": true},
        ]);
        let text = |height: u16| {
            let view = json!({"joined": false, "sessions": all, "assistantsAvailable": true});
            let n = NetworkSheet { view: Some(view), ..Default::default() };
            let mut buf = Buffer::empty(Rect::new(0, 0, 100, height));
            let area = buf.area;
            network(&mut buf, area, &n);
            buf.content.iter().map(ratatui::buffer::Cell::symbol).collect::<String>()
        };
        let tall = text(30);
        assert!(tall.contains("Agents") && tall.contains("Working · Checks its mail"), "{tall}");
        let at = |s: &str| tall.find(s).unwrap_or(usize::MAX);
        assert!(at("Fix refunds") < at("web-codex") && at("web-codex") < at("infra-codex"), "{tall}");
        assert!(tall.contains("● Fix refunds  billing-claude · Working"), "spaced apart: {tall}");
        assert!(tall.contains("○ web-codex") && tall.contains("  infra-codex"), "no state, no dot: {tall}");
        assert!(!tall.contains("More sessions"), "{tall}");
        let short = text(SHORT);
        assert!(short.contains("Fix refunds") && short.contains("More sessions: 2"), "{short}");
        assert!(short.contains("Cloud assistants"), "the switch keeps its row: {short}");
        // Narrow: the address stays whole, the mail text gives way first.
        let address = "a-very-long-repository-name-claude-40ch";
        let narrow = {
            let view = json!({"joined": false, "assistantsAvailable": true, "sessions": [
                {"address": address, "title": "A title long enough to be cut", "state": "working", "runs": true, "tier": "pull"},
            ]});
            let n = NetworkSheet { view: Some(view), ..Default::default() };
            let mut buf = Buffer::empty(Rect::new(0, 0, 64, 30));
            let area = buf.area;
            network(&mut buf, area, &n);
            buf.content.iter().map(ratatui::buffer::Cell::symbol).collect::<String>()
        };
        assert!(narrow.contains(address), "{narrow}");
        // Exactly enough room keeps the state: the check and the drawing count the same columns.
        let fits = |width: u16| {
            let view = json!({"joined": false, "assistantsAvailable": true, "sessions": [
                {"address": "billing-claude", "title": "A title long enough to be cut", "state": "working", "runs": true},
            ]});
            let n = NetworkSheet { view: Some(view), ..Default::default() };
            let mut buf = Buffer::empty(Rect::new(0, 0, width, 30));
            let area = buf.area;
            network(&mut buf, area, &n);
            buf.content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
                .contains("billing-claude · Working")
        };
        // Wide characters count by the columns they take: at 46 a CJK address and its state still fit whole.
        let wide = {
            let view = json!({"joined": false, "assistantsAvailable": true, "sessions": [
                {"address": "界-codex", "title": "A title long enough to be cut", "state": "working", "runs": true},
            ]});
            let n = NetworkSheet { view: Some(view), ..Default::default() };
            let mut buf = Buffer::empty(Rect::new(0, 0, 46, 30));
            let area = buf.area;
            network(&mut buf, area, &n);
            buf.content.iter().map(ratatui::buffer::Cell::symbol).collect::<String>()
        };
        assert!(wide.contains("-codex · Working"), "{wide}");
        let first = (30..90).find(|w| fits(*w)).expect("the state fits somewhere");
        assert!((first..90).all(fits), "once it fits, wider keeps it");
    }
}
