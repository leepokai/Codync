//! The rows a conversation is made of (kit/Sources/CodyncUI/Thread/ChatRows.swift and
//! PermissionCard.swift): bubbles, author labels, thread chips, notices, approval cards and
//! the working indicator.

use crate::client;
use crate::ui::{self, App, State, folder, toast};
use crate::{avatar, markup, orb};
use adw::prelude::*;
use gtk::glib::DateTime;
use serde_json::{Value, json};

// MARK: time

fn local(ms: i64) -> Option<DateTime> {
    DateTime::from_unix_local(ms / 1000).ok()
}

fn fmt(d: &DateTime, f: &str) -> String {
    d.format(f)
        .map(|s| s.trim().replace("  ", " "))
        .unwrap_or_default()
}

/// Days between the date and today (0 = today).
fn days_ago(d: &DateTime) -> i32 {
    let Ok(now) = DateTime::now_local() else {
        return 0;
    };
    let day = |x: &DateTime| {
        DateTime::from_local(x.year(), x.month(), x.day_of_month(), 0, 0, 0.0)
            .map(|d| d.to_unix())
            .unwrap_or(0)
    };
    i32::try_from((day(&now) - day(d)) / 86_400).unwrap_or(i32::MAX)
}

/// 3:27 PM
pub fn clock(ms: i64) -> String {
    local(ms).map(|d| fmt(&d, "%l:%M %p")).unwrap_or_default()
}

/// Roster/thread stamp: 3:45 AM · Yesterday · Wednesday · Sep 16
pub fn day(ms: i64) -> String {
    let Some(d) = local(ms) else {
        return String::new();
    };
    match days_ago(&d) {
        ..=0 => fmt(&d, "%l:%M %p"),
        1 => "Yesterday".into(),
        2..6 => fmt(&d, "%A"),
        _ => fmt(&d, "%b %e"),
    }
}

/// Chat separator: Today 3:27 AM · Yesterday 5:20 PM · Sep 16 9:02 AM
pub fn separator(ms: i64) -> String {
    let Some(d) = local(ms) else {
        return String::new();
    };
    let when = match days_ago(&d) {
        ..=0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => fmt(&d, "%b %e"),
    };
    format!("{when} {}", fmt(&d, "%l:%M %p"))
}

// MARK: small pieces

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
        .build()
}

/// An icon-only button with its tooltip (IconButton).
pub fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .css_classes(["icon-btn"])
        .valign(gtk::Align::Center)
        .build()
}

pub fn hline() -> gtk::Box {
    gtk::Box::builder().css_classes(["hline"]).build()
}

fn bubble(text: &str, markup: bool, class: &str) -> gtk::Label {
    let l = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .xalign(0.0)
        .css_classes(["bubble", class])
        .build();
    if markup {
        l.set_markup(&markup::to_pango(text));
    } else {
        l.set_label(text);
    }
    l
}

// MARK: messages

/// Main-chat context for a message: the entry id it starts a thread on.
pub struct Reply<'a> {
    pub ui: &'a App,
    pub root: Option<&'a str>,
}

pub fn user_bubble(r: &Reply, e: &Value, bot_working: bool, start: bool) -> gtk::Widget {
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .halign(gtk::Align::End)
        .margin_start(56)
        .margin_top(if start { 12 } else { 4 })
        .build();
    let text = e["data"]["text"].as_str().unwrap_or("");
    let b = bubble(text, false, "bubble-user");
    col.append(&with_menu(r, b.upcast_ref(), true, text, false));
    let status = match e["data"]["status"].as_str() {
        Some("sending") => "Sending…".to_owned(),
        Some("queued") if bot_working => "Waiting to send — it'll read this when it's done".into(),
        Some("failed") => "Failed to send".into(),
        Some("cancelled") => "Not sent — stopped".into(),
        _ => clock(e["createdAt"].as_i64().unwrap_or(0)),
    };
    let s = label(&status, &["time"]);
    s.set_halign(gtk::Align::End);
    if e["data"]["status"] == "failed" {
        s.add_css_class("danger-text");
    }
    col.append(&s);
    col.upcast()
}

pub fn agent_bubble(r: &Reply, st: &State, e: &Value, group: bool, start: bool) -> gtk::Widget {
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_end(64)
        .margin_top(if start { 12 } else { 4 })
        .build();
    if group && start {
        col.append(&author_label(st, e["data"]["author"].as_str()));
    }
    let text = e["data"]["text"].as_str().unwrap_or("");
    let b = bubble(text, true, "bubble-agent");
    b.set_halign(gtk::Align::Start);
    col.append(&with_menu(r, b.upcast_ref(), false, text, true));
    let t = label(&clock(e["createdAt"].as_i64().unwrap_or(0)), &["time"]);
    t.set_margin_start(12);
    col.append(&t);
    col.upcast()
}

pub fn notice(e: &Value) -> gtk::Widget {
    let text = e["data"]["text"].as_str().unwrap_or("");
    match e["data"]["style"].as_str() {
        Some("divider") => {
            let row = gtk::Box::builder()
                .spacing(10)
                .margin_top(16)
                .margin_bottom(6)
                .build();
            let l = gtk::Label::builder()
                .label(text)
                .wrap(true)
                .justify(gtk::Justification::Center)
                .max_width_chars(40)
                .css_classes(["footnote", "tertiary"])
                .build();
            let (a, b) = (hline(), hline());
            for w in [&a, &b] {
                w.set_hexpand(true);
                w.set_valign(gtk::Align::Center);
            }
            row.append(&a);
            row.append(&l);
            row.append(&b);
            row.upcast()
        }
        Some("error") => {
            let row = gtk::Box::builder()
                .spacing(8)
                .css_classes(["notice-error"])
                .margin_top(10)
                .build();
            let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
            icon.add_css_class("danger-text");
            icon.set_valign(gtk::Align::Start);
            row.append(&icon);
            let l = label(text, &["small"]);
            l.set_wrap(true);
            l.set_selectable(true);
            l.set_hexpand(true);
            row.append(&l);
            row.upcast()
        }
        _ => {
            let l = gtk::Label::builder()
                .label(text)
                .wrap(true)
                .justify(gtk::Justification::Center)
                .css_classes(["footnote", "secondary"])
                .margin_top(10)
                .build();
            l.upcast()
        }
    }
}

pub fn permission_card(ui: &App, st: &State, e: &Value, group: bool) -> gtk::Widget {
    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(12)
        .build();
    if group {
        outer.append(&author_label(st, e["data"]["author"].as_str()));
    }
    let d = &e["data"];
    let pending = d["status"] == "pending";
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .css_classes(["perm-card"])
        .margin_end(40)
        .build();
    let headline = match d["toolKind"].as_str() {
        Some("execute") => "Wants to run a command",
        Some("edit" | "delete" | "move") => "Wants to change files",
        Some("fetch") => "Wants to access the web",
        Some("read" | "search") => "Wants to read files",
        _ => "Wants to use a tool",
    };
    let top = gtk::Box::builder().spacing(8).build();
    top.append(&label(headline, &["headline"]));
    if pending {
        let dot = gtk::DrawingArea::builder()
            .content_width(7)
            .content_height(7)
            .valign(gtk::Align::Center)
            .build();
        dot.set_draw_func(|_, cr, w, _| {
            let r = f64::from(w) / 2.0;
            cr.arc(r, r, r, 0.0, std::f64::consts::TAU);
            let (red, g, b) = avatar::rgb(0xF0A030);
            cr.set_source_rgb(red, g, b);
            cr.fill().ok();
        });
        top.append(&dot);
    }
    card.append(&top);
    let title = label(
        d["title"].as_str().unwrap_or(""),
        &["small", "mono", "secondary"],
    );
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    card.append(&title);
    let host = st.hello["name"].as_str().unwrap_or("this computer");
    let runs = gtk::Box::builder().spacing(5).build();
    let icon = gtk::Image::from_icon_name("computer-symbolic");
    icon.add_css_class("tertiary");
    icon.set_pixel_size(12);
    runs.append(&icon);
    let cwd = d["cwd"].as_str().map(folder);
    runs.append(&label(
        &format!(
            "Runs on {host}{}",
            cwd.map(|c| format!(" · {c}")).unwrap_or_default()
        ),
        &["footnote", "tertiary"],
    ));
    card.append(&runs);

    let mut detail = String::new();
    if let Some(c) = d["command"].as_str() {
        detail.push_str(c);
    }
    if let Some(c) = d["detail"].as_str() {
        detail.push('\n');
        detail.push_str(&c.chars().take(2000).collect::<String>());
    }
    for diff in d["diffs"].as_array().into_iter().flatten() {
        detail.push_str(&format!(
            "\n{}  +{} −{}\n{}",
            diff["path"].as_str().unwrap_or(""),
            diff["added"],
            diff["removed"],
            diff["patch"].as_str().unwrap_or("")
        ));
    }
    if !detail.trim().is_empty() {
        card.append(&disclosure("Details", &{
            let l = label(detail.trim(), &["codebox"]);
            l.set_selectable(true);
            l.set_wrap(true);
            l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            l
        }));
    }
    if pending {
        let choices = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["choices"])
            .margin_top(4)
            .build();
        let mut opts: Vec<&Value> = d["options"].as_array().into_iter().flatten().collect();
        let rank = |k: &str| match k {
            "allow_once" => 0,
            "allow_always" => 1,
            "reject_once" => 2,
            "reject_always" => 3,
            _ => 9,
        };
        opts.sort_by_key(|o| rank(o["kind"].as_str().unwrap_or("")));
        for (i, o) in opts.iter().enumerate() {
            let kind = o["kind"].as_str().unwrap_or("");
            let text = match kind {
                "allow_once" => "Allow once",
                "allow_always" => "Always allow",
                "reject_once" => "Deny",
                "reject_always" => "Never",
                _ => o["name"].as_str().unwrap_or("OK"),
            };
            if i > 0 {
                choices.append(&hline());
            }
            let l = label(text, &[]);
            if kind == "allow_once" {
                l.add_css_class("semibold");
            }
            if !kind.starts_with("allow") {
                l.add_css_class("danger-text");
            }
            let btn = gtk::Button::builder()
                .child(&l)
                .css_classes(["choice"])
                .build();
            let (entry_id, opt) = (
                e["id"].as_str().unwrap_or("").to_owned(),
                o["optionId"].clone(),
            );
            let ui2 = ui.clone();
            btn.connect_clicked(move |b| {
                b.set_sensitive(false);
                let ui3 = ui2.clone();
                client::call(
                    "respondPermission",
                    json!({"entryId": entry_id, "optionId": opt}),
                    move |r| {
                        if let Err(e) = r {
                            toast(&ui3, &e);
                        }
                    },
                );
            });
            choices.append(&btn);
        }
        card.append(&choices);
    } else {
        let chosen = d["options"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|o| o["optionId"] == d["selected"]);
        let outcome = match (
            d["status"].as_str(),
            chosen.and_then(|o| o["kind"].as_str()),
        ) {
            (Some("answered"), Some("allow_once")) => "Allowed once",
            (Some("answered"), Some("allow_always")) => "Always allowed",
            (Some("answered"), Some("reject_once")) => "Denied",
            (Some("answered"), Some("reject_always")) => "Never allowed",
            (Some("answered"), _) => chosen
                .and_then(|o| o["name"].as_str())
                .unwrap_or("Answered"),
            (Some("cancelled"), _) => "Cancelled",
            _ => "Expired — the agent moved on",
        };
        card.append(&label(outcome, &["footnote", "medium", "secondary"]));
    }
    outer.append(&card);
    outer.upcast()
}

/// A "Details ›" line that slides its content open.
pub fn disclosure(title: &str, content: &impl IsA<gtk::Widget>) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let head = gtk::Box::builder().spacing(4).build();
    head.append(&label(title, &["footnote", "medium", "secondary"]));
    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
    chevron.set_pixel_size(10);
    chevron.add_css_class("secondary");
    head.append(&chevron);
    let btn = gtk::Button::builder()
        .child(&head)
        .css_classes(["plain"])
        .halign(gtk::Align::Start)
        .build();
    let rev = gtk::Revealer::builder()
        .child(content)
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .build();
    content.set_margin_top(6);
    {
        let rev = rev.clone();
        btn.connect_clicked(move |_| {
            let open = !rev.reveals_child();
            rev.set_reveal_child(open);
            chevron.set_icon_name(Some(if open {
                "go-down-symbolic"
            } else {
                "go-next-symbolic"
            }));
        });
    }
    col.append(&btn);
    col.append(&rev);
    col
}

/// Who wrote a message in a group: their avatar and name in their color.
pub fn author_label(st: &State, id: Option<&str>) -> gtk::Widget {
    let bot = id.and_then(|id| st.bots.get(id));
    let name = bot
        .and_then(|b| b["name"].as_str())
        .unwrap_or("A deleted bot");
    let row = gtk::Box::builder().spacing(8).build();
    if let Some(b) = bot {
        row.append(&avatar::of(&st.bots, b, 26, false));
    }
    let l = gtk::Label::builder()
        .use_markup(true)
        .label(match bot {
            Some(b) => format!(
                "<span foreground=\"#{:06X}\">{}</span>",
                avatar::color_of(b["avatarColor"].as_str().unwrap_or("gray")),
                gtk::glib::markup_escape_text(name)
            ),
            None => gtk::glib::markup_escape_text(name).to_string(),
        })
        .css_classes(["author"])
        .build();
    if bot.is_none() {
        l.add_css_class("secondary");
    }
    row.append(&l);
    row.upcast()
}

/// Copy / Reply in thread / Show what it did on right-click or long-press, and a
/// "Reply in thread" button that shows beside a main-chat message on hover.
fn with_menu(r: &Reply, w: &gtk::Widget, end: bool, text: &str, agent: bool) -> gtk::Widget {
    let host = gtk::Box::builder()
        .spacing(4)
        .css_classes(["reply-host"])
        .halign(if end {
            gtk::Align::End
        } else {
            gtk::Align::Start
        })
        .build();
    let btn = r.root.map(|root| {
        let btn = icon_button("mail-reply-sender-symbolic", "Reply in thread");
        btn.add_css_class("reply-btn");
        let (ui2, root2) = (r.ui.clone(), root.to_owned());
        btn.connect_clicked(move |_| ui::open_thread(&ui2, &root2));
        btn
    });
    if end && let Some(b) = &btn {
        host.append(b);
    }
    host.append(w);
    if !end && let Some(b) = &btn {
        host.append(b);
    }
    let menu = {
        let (ui2, root, text, host2) = (
            r.ui.clone(),
            r.root.map(str::to_owned),
            text.to_owned(),
            host.clone(),
        );
        move |x: f64, y: f64| {
            let mut items: Vec<ui::MenuItem> = vec![];
            let t = text.clone();
            items.push(ui::MenuItem::new(
                "edit-copy-symbolic",
                "Copy",
                Box::new(move || {
                    if let Some(d) = gtk::gdk::Display::default() {
                        d.clipboard().set_text(&t);
                    }
                }),
            ));
            if let Some(root) = root.clone() {
                let ui3 = ui2.clone();
                items.push(ui::MenuItem::new(
                    "mail-reply-sender-symbolic",
                    "Reply in thread",
                    Box::new(move || ui::open_thread(&ui3, &root)),
                ));
            }
            if agent {
                let ui3 = ui2.clone();
                items.push(ui::MenuItem::new(
                    "view-list-symbolic",
                    "Show what it did",
                    Box::new(move || crate::dialogs::trace(&ui3)),
                ));
            }
            ui::popup_menu(&host2, Some((x, y)), items);
        }
    };
    // Capture phase: runs before the selectable label's own context menu.
    let click = gtk::GestureClick::builder()
        .button(gtk::gdk::BUTTON_SECONDARY)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    {
        let menu = menu.clone();
        click.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            menu(x, y);
        });
    }
    host.add_controller(click);
    let hold = gtk::GestureLongPress::builder()
        .touch_only(true)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    hold.connect_pressed(move |g, x, y| {
        g.set_state(gtk::EventSequenceState::Claimed);
        menu(x, y);
    });
    host.add_controller(hold);
    host.upcast()
}

/// Under a message with a thread (Slack's): who replied, how many, how recently.
pub fn thread_chip(ui: &App, st: &State, e: &Value, end: bool, indent: i32) -> Option<gtk::Widget> {
    let t = &e["data"]["thread"];
    let count = t["count"].as_i64().filter(|c| *c > 0)?;
    let inner = gtk::Box::builder().spacing(6).build();
    let looks: Vec<(&str, &str, avatar::Mood)> = t["authors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| st.bots.get(a.as_str()?))
        .take(3)
        .map(|b| {
            (
                b["avatarShape"].as_str().unwrap_or("blob"),
                b["avatarColor"].as_str().unwrap_or("blue"),
                avatar::Mood::Idle,
            )
        })
        .collect();
    if !looks.is_empty() {
        inner.append(&avatar::row(&looks, 18, 6));
    }
    inner.append(&label(
        &if count == 1 {
            "1 reply".to_owned()
        } else {
            format!("{count} replies")
        },
        &["footnote", "semibold"],
    ));
    inner.append(&label(
        &day(t["lastAt"].as_i64().unwrap_or(0)),
        &["footnote", "tertiary"],
    ));
    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
    chevron.set_pixel_size(10);
    chevron.add_css_class("tertiary");
    inner.append(&chevron);
    let btn = gtk::Button::builder()
        .child(&inner)
        .tooltip_text("View thread")
        .css_classes(["thread-chip"])
        .halign(if end {
            gtk::Align::End
        } else {
            gtk::Align::Start
        })
        .margin_start(indent)
        .margin_top(4)
        .build();
    let (ui2, root) = (ui.clone(), e["id"].as_str().unwrap_or_default().to_owned());
    btn.connect_clicked(move |_| ui::open_thread(&ui2, &root));
    Some(btn.upcast())
}

/// The bot's live activity line: a turning orb, what it's doing and for how long.
pub fn working(ui: &App, bot: &Value) -> gtk::Widget {
    let needs = bot["status"] == "needsInput";
    let inner = gtk::Box::builder().spacing(8).build();
    let o = orb::widget(needs, 16);
    o.add_css_class(if needs { "warning-text" } else { "secondary" });
    inner.append(&o);
    let act = bot["activity"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Working…");
    let l = label(
        act,
        &["small", if needs { "warning-text" } else { "secondary" }],
    );
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    inner.append(&l);
    if let Some(started) = bot["startedAt"].as_i64() {
        let timer = label("", &["footnote", "tertiary"]);
        let tick = {
            let timer = timer.downgrade();
            move || {
                let Some(timer) = timer.upgrade() else {
                    return gtk::glib::ControlFlow::Break;
                };
                let s = ((ui::now_ms() - started) / 1000).max(0);
                timer.set_label(&if s >= 3600 {
                    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
                } else {
                    format!("{}:{:02}", s / 60, s % 60)
                });
                gtk::glib::ControlFlow::Continue
            }
        };
        tick();
        gtk::glib::timeout_add_seconds_local(1, tick);
        inner.append(&timer);
    }
    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
    chevron.set_pixel_size(10);
    chevron.add_css_class("tertiary");
    inner.append(&chevron);
    let btn = gtk::Button::builder()
        .child(&inner)
        .css_classes(["working"])
        .halign(gtk::Align::Start)
        .tooltip_text("Show what it's doing")
        .margin_top(6)
        .build();
    let ui2 = ui.clone();
    btn.connect_clicked(move |_| crate::dialogs::trace(&ui2));
    btn.upcast()
}

#[cfg(test)]
mod tests {
    #[test]
    fn stamps() {
        let now = crate::ui::now_ms();
        assert!(super::separator(now).starts_with("Today "));
        assert!(super::separator(now - 86_400_000).starts_with("Yesterday "));
        assert!(super::clock(now).ends_with('M'));
        assert_eq!(super::day(now - 86_400_000), "Yesterday");
    }
}
