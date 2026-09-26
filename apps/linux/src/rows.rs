//! The rows a conversation is made of: bubbles, notices and approval cards.

use crate::client;
use crate::ui::{self, App, State, ago, folder, toast};
use crate::{avatar, markup};
use adw::prelude::*;
use serde_json::{Value, json};

pub fn user_bubble(e: &Value, bot_working: bool, start: bool) -> gtk::Widget {
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::End)
        .margin_start(60)
        .margin_top(if start { 12 } else { 2 })
        .build();
    col.append(
        &gtk::Label::builder()
            .label(e["data"]["text"].as_str().unwrap_or(""))
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .selectable(true)
            .xalign(0.0)
            .css_classes(["bubble", "bubble-user"])
            .build(),
    );
    let status = match e["data"]["status"].as_str() {
        Some("sending") => "Sending…",
        Some("queued") if bot_working => "Waiting to send — it'll read this when it's done",
        Some("failed") => "Failed to send",
        Some("cancelled") => "Not sent — stopped",
        _ => "",
    };
    if !status.is_empty() {
        col.append(
            &gtk::Label::builder()
                .label(status)
                .css_classes(["small", "muted"])
                .halign(gtk::Align::End)
                .build(),
        );
    }
    col.upcast()
}

pub fn agent_bubble(e: &Value, bot: &Value, start: bool) -> gtk::Widget {
    let row = gtk::Box::builder()
        .spacing(8)
        .margin_end(60)
        .margin_top(if start { 12 } else { 2 })
        .build();
    if start {
        row.append(&avatar::widget(
            bot["avatarShape"].as_str().unwrap_or("blob"),
            bot["avatarColor"].as_str().unwrap_or("blue"),
            28,
            false,
            "",
        ));
    } else {
        row.append(&gtk::Box::builder().width_request(28).build());
    }
    let label = gtk::Label::builder()
        .use_markup(true)
        .label(markup::to_pango(e["data"]["text"].as_str().unwrap_or("")))
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .xalign(0.0)
        .css_classes(["bubble", "bubble-agent"])
        .valign(gtk::Align::End)
        .build();
    row.append(&label);
    row.upcast()
}

pub fn notice(e: &Value) -> gtk::Widget {
    let text = e["data"]["text"].as_str().unwrap_or("");
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .selectable(true)
        .margin_top(10)
        .build();
    match e["data"]["style"].as_str() {
        Some("error") => {
            label.add_css_class("notice-error");
            label.set_xalign(0.0);
        }
        _ => label.add_css_class("dim-label"),
    }
    label.upcast()
}

pub fn permission_card(ui: &App, st: &State, e: &Value) -> gtk::Widget {
    let d = &e["data"];
    let pending = d["status"] == "pending";
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .css_classes(["card"])
        .margin_start(36)
        .margin_top(12)
        .build();
    if pending {
        card.add_css_class("pending");
    }
    let headline = match d["toolKind"].as_str() {
        Some("execute") => "Wants to run a command",
        Some("edit" | "delete" | "move") => "Wants to change files",
        Some("fetch") => "Wants to access the web",
        Some("read" | "search") => "Wants to read files",
        _ => "Wants to use a tool",
    };
    card.append(
        &gtk::Label::builder()
            .label(format!("✋ {headline}"))
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    card.append(
        &gtk::Label::builder()
            .label(d["title"].as_str().unwrap_or(""))
            .xalign(0.0)
            .wrap(true)
            .build(),
    );
    let host = st.hello["name"].as_str().unwrap_or("this computer");
    card.append(
        &gtk::Label::builder()
            .label(format!(
                "Runs on {host} · {}",
                folder(d["cwd"].as_str().unwrap_or(""))
            ))
            .xalign(0.0)
            .css_classes(["small", "muted"])
            .build(),
    );
    let mut detail = String::new();
    if let Some(c) = d["command"].as_str() {
        detail.push_str(c);
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
        let exp = gtk::Expander::new(Some("Details"));
        exp.set_child(Some(
            &gtk::Label::builder()
                .label(detail.trim())
                .xalign(0.0)
                .selectable(true)
                .wrap(true)
                .css_classes(["codebox"])
                .build(),
        ));
        card.append(&exp);
    }
    if pending {
        let buttons = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(4)
            .column_spacing(8)
            .row_spacing(8)
            .homogeneous(true)
            .build();
        let mut opts: Vec<&Value> = d["options"].as_array().into_iter().flatten().collect();
        let rank = |k: &str| match k {
            "allow_once" => 0,
            "allow_always" => 1,
            "reject_once" => 2,
            _ => 3,
        };
        opts.sort_by_key(|o| rank(o["kind"].as_str().unwrap_or("")));
        for o in opts {
            let kind = o["kind"].as_str().unwrap_or("");
            let label = match kind {
                "allow_once" => "Allow once",
                "allow_always" => "Always allow",
                "reject_once" => "Deny",
                "reject_always" => "Never",
                _ => o["name"].as_str().unwrap_or("OK"),
            };
            let btn = gtk::Button::with_label(label);
            if kind == "allow_once" {
                btn.add_css_class("accent-fill");
            } else if kind.starts_with("reject") {
                btn.add_css_class("destructive-action");
            }
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
            buttons.insert(&btn, -1);
        }
        card.append(&buttons);
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
            (Some("answered"), _) => "Answered",
            (Some("cancelled"), _) => "Cancelled",
            _ => "Expired — the agent moved on",
        };
        card.append(
            &gtk::Label::builder()
                .label(outcome)
                .xalign(0.0)
                .css_classes(["small", "dim-label"])
                .build(),
        );
    }
    card.upcast()
}

/// Who wrote a message in a group: their name in their color.
pub fn author_label(st: &State, id: Option<&str>) -> gtk::Widget {
    let bot = id.and_then(|id| st.bots.get(id));
    let name = bot
        .and_then(|b| b["name"].as_str())
        .unwrap_or("A deleted bot");
    let hex = avatar::color_of(
        bot.and_then(|b| b["avatarColor"].as_str())
            .unwrap_or("gray"),
    );
    gtk::Label::builder()
        .use_markup(true)
        .label(format!(
            "<span foreground=\"#{hex:06X}\">{}</span>",
            gtk::glib::markup_escape_text(name)
        ))
        .xalign(0.0)
        .css_classes(["small", "heading"])
        .margin_start(36)
        .margin_top(12)
        .build()
        .upcast()
}

/// A main-chat message with a "Reply in thread" button that shows on hover or focus.
pub fn with_reply(ui: &App, w: &gtk::Widget, end: bool, root: &str) -> gtk::Widget {
    let host = gtk::Box::builder()
        .spacing(4)
        .css_classes(["reply-host"])
        .build();
    if end {
        host.set_halign(gtk::Align::End);
    }
    let btn = gtk::Button::builder()
        .icon_name("mail-reply-sender-symbolic")
        .tooltip_text("Reply in thread")
        .css_classes(["flat", "circular", "reply-btn"])
        .valign(gtk::Align::Center)
        .build();
    let (ui2, root) = (ui.clone(), root.to_owned());
    btn.connect_clicked(move |_| ui::open_thread(&ui2, &root));
    if end {
        host.append(&btn);
        host.append(w);
    } else {
        host.append(w);
        host.append(&btn);
    }
    host.upcast()
}

/// Under a message with a thread (Slack's): who replied, how many, how recently.
pub fn thread_chip(ui: &App, st: &State, e: &Value, end: bool) -> Option<gtk::Widget> {
    let t = &e["data"]["thread"];
    let count = t["count"].as_i64().filter(|c| *c > 0)?;
    let inner = gtk::Box::builder().spacing(6).build();
    let faces = gtk::Box::builder().spacing(2).build();
    for b in t["authors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| st.bots.get(a.as_str()?))
        .take(3)
    {
        faces.append(&avatar::of(&st.bots, b, 18, false, ""));
    }
    inner.append(&faces);
    inner.append(
        &gtk::Label::builder()
            .label(if count == 1 {
                "1 reply".to_owned()
            } else {
                format!("{count} replies")
            })
            .css_classes(["small", "heading"])
            .build(),
    );
    inner.append(
        &gtk::Label::builder()
            .label(ago(t["lastAt"].as_i64().unwrap_or(0)))
            .css_classes(["small", "muted"])
            .build(),
    );
    inner.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    let btn = gtk::Button::builder()
        .child(&inner)
        .tooltip_text("View thread")
        .css_classes(["flat", "thread-chip"])
        .halign(if end {
            gtk::Align::End
        } else {
            gtk::Align::Start
        })
        .margin_start(if end { 0 } else { 36 })
        .margin_top(4)
        .build();
    let (ui2, root) = (ui.clone(), e["id"].as_str().unwrap_or_default().to_owned());
    btn.connect_clicked(move |_| ui::open_thread(&ui2, &root));
    Some(btn.upcast())
}
