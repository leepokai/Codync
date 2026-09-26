//! The "new message" page (kit NewChatView): a To: field that searches your bots or creates a
//! new one, with Ctrl+1…9 shortcuts, and the message box underneath. Picked bots become chips;
//! one opens its chat, several start a group chat with them (or open the one they share).

use crate::client;
use crate::rows::{hline, icon_button, label};
use crate::ui::{self, App, flat_header};
use crate::{avatar, dialogs};
use adw::prelude::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

pub struct Page {
    pub root: gtk::Box,
    chips: gtk::Box,
    to: gtk::Entry,
    list: gtk::Box,
    draft: gtk::Entry,
    close_btn: gtk::Button,
    recipients: RefCell<Vec<String>>,
    creating: Cell<bool>,
    /// The chat that was open before, restored when the page closes empty-handed.
    previous: RefCell<Option<String>>,
}

pub fn build() -> Page {
    let chips = gtk::Box::builder().spacing(6).build();
    let to = gtk::Entry::builder()
        .css_classes(["plain", "title3"])
        .hexpand(true)
        .width_chars(20)
        .build();
    chips.append(&to);
    let to_row = gtk::Box::builder()
        .spacing(8)
        .hexpand(true)
        .margin_start(16)
        .margin_end(8)
        .build();
    to_row.append(&label("To:", &["title3", "secondary"]));
    to_row.append(
        &gtk::ScrolledWindow::builder()
            .child(&chips)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hscrollbar_policy(gtk::PolicyType::External)
            .hexpand(true)
            .build(),
    );
    let close = icon_button("window-close-symbolic", "Close (Esc)");
    to_row.append(&close);
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    let card = gtk::Box::builder()
        .css_classes(["pick-card"])
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .margin_start(22)
        .margin_end(22)
        .margin_top(10)
        .build();
    card.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .propagate_natural_width(true)
            .max_content_height(460)
            .hexpand(true)
            .build(),
    );
    let draft = gtk::Entry::builder()
        .css_classes(["plain"])
        .hexpand(true)
        .margin_top(11)
        .margin_bottom(11)
        .build();
    let pill = gtk::Box::builder()
        .css_classes(["composer"])
        .margin_start(16)
        .margin_end(16)
        .margin_bottom(16)
        .margin_top(16)
        .build();
    pill.append(&draft);
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["chat-bg"])
        .build();
    let header = flat_header(&to_row);
    root.append(&header);
    root.append(&hline());
    root.append(&card);
    root.append(&gtk::Box::builder().vexpand(true).build());
    root.append(&pill);
    Page {
        root,
        chips,
        to,
        list,
        draft,
        close_btn: close,
        recipients: RefCell::new(vec![]),
        creating: Cell::new(false),
        previous: RefCell::new(None),
    }
}

pub fn open(ui: &App) {
    {
        let mut st = ui.state.borrow_mut();
        if !st.composing {
            *ui.compose_page.previous.borrow_mut() = st.current.take();
        }
        st.composing = true;
        st.open_thread = None;
    }
    let p = &ui.compose_page;
    p.recipients.borrow_mut().clear();
    p.to.set_text("");
    p.draft.set_text("");
    ui.roster.unselect_all();
    ui.split.set_show_content(true);
    ui::render(ui);
    p.to.grab_focus();
}

fn close(ui: &App) {
    {
        let mut st = ui.state.borrow_mut();
        st.composing = false;
        if st.current.is_none() {
            st.current = ui.compose_page.previous.borrow_mut().take();
        }
    }
    ui::schedule(ui);
}

/// Bots not picked yet; a group can only be opened on its own, so groups go once someone's picked.
fn matches(ui: &App) -> Vec<Value> {
    let st = ui.state.borrow();
    let q = ui.compose_page.to.text().trim().to_lowercase();
    let picked = ui.compose_page.recipients.borrow();
    ui::roster(&st)
        .into_iter()
        .filter(|b| {
            let id = b["id"].as_str().unwrap_or_default();
            !picked.iter().any(|p| p == id)
                && (!ui::is_group(b) || picked.is_empty())
                && (q.is_empty()
                    || b["name"]
                        .as_str()
                        .is_some_and(|n| n.to_lowercase().contains(&q)))
        })
        .cloned()
        .collect()
}

fn keycaps(n: usize) -> gtk::Box {
    let b = gtk::Box::builder().spacing(3).build();
    for k in ["Ctrl", &n.to_string()] {
        let l = gtk::Label::builder()
            .label(k)
            .css_classes(["keycap"])
            .build();
        b.append(&l);
    }
    b
}

fn pick_row(icon: &impl IsA<gtk::Widget>, text: &str, shortcut: Option<usize>) -> gtk::Button {
    let row = gtk::Box::builder().spacing(12).build();
    row.append(icon);
    let l = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        // Wide enough to fill the Mac's 620 pt card when there's room.
        .width_chars(62)
        .max_width_chars(62)
        .build();
    row.append(&l);
    if let Some(n) = shortcut {
        row.append(&keycaps(n));
    }
    gtk::Button::builder()
        .child(&row)
        .css_classes(["pick-row"])
        .build()
}

pub fn render(ui: &App) {
    let p = &ui.compose_page;
    // Chips before the field.
    while let Some(c) = p.chips.first_child() {
        if c == p.to.clone().upcast::<gtk::Widget>() {
            break;
        }
        p.chips.remove(&c);
    }
    let st = ui.state.borrow();
    let picked: Vec<Value> = p
        .recipients
        .borrow()
        .iter()
        .filter_map(|id| st.bots.get(id).cloned())
        .collect();
    for b in &picked {
        let id = b["id"].as_str().unwrap_or_default().to_owned();
        let ui2 = ui.clone();
        let chip = bot_chip(&st.bots, b, move || remove(&ui2, &id));
        p.chips
            .insert_child_after(&chip, p.to.prev_sibling().as_ref());
    }
    p.to.set_placeholder_text(Some(if picked.is_empty() {
        "Search or create bots"
    } else {
        "Add another bot"
    }));
    p.draft.set_placeholder_text(Some(&match picked.len() {
        0 => "Message Bot".to_owned(),
        _ => format!(
            "Message {}",
            picked
                .iter()
                .filter_map(|b| b["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }));
    drop(st);

    ui::clear(&p.list);
    let q = p.to.text().trim().to_owned();
    let plus_box = gtk::Image::builder()
        .icon_name(if p.creating.get() {
            "content-loading-symbolic"
        } else {
            "list-add-symbolic"
        })
        .pixel_size(13)
        .width_request(26)
        .height_request(26)
        .css_classes(["plus-circle"])
        .build();
    let create_row = pick_row(
        &plus_box,
        &if q.is_empty() {
            "Create new Bot".to_owned()
        } else {
            format!("Create “{q}”")
        },
        Some(1),
    );
    {
        let ui2 = ui.clone();
        create_row.connect_clicked(move |_| create(&ui2));
    }
    p.list.append(&create_row);
    let st = ui.state.borrow();
    for (i, b) in matches(ui).iter().enumerate() {
        let n = i + 2;
        let row = pick_row(
            &avatar::of(&st.bots, b, 26, false),
            b["name"].as_str().unwrap_or(""),
            (n <= 9).then_some(n),
        );
        let (ui2, b2) = (ui.clone(), b.clone());
        row.connect_clicked(move |_| choose(&ui2, &b2));
        p.list.append(&row);
    }
}

/// A picked bot (To: field, group editor); its x takes it back out.
pub fn bot_chip(
    bots: &std::collections::HashMap<String, Value>,
    b: &Value,
    remove: impl Fn() + 'static,
) -> gtk::Box {
    let chip = gtk::Box::builder()
        .spacing(6)
        .css_classes(["chip"])
        .valign(gtk::Align::Center)
        .build();
    chip.append(&avatar::of(bots, b, 20, false));
    let name = b["name"].as_str().unwrap_or("");
    let l = label(name, &["body13"]);
    l.set_max_width_chars(24);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    chip.append(&l);
    let x = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .css_classes(["chip-x"])
        .tooltip_text(format!("Remove {name}"))
        .build();
    x.connect_clicked(move |_| remove());
    chip.append(&x);
    chip
}

/// A bot becomes a chip; a group (with nobody picked) opens right away.
fn choose(ui: &App, b: &Value) {
    let id = b["id"].as_str().unwrap_or_default().to_owned();
    if ui::is_group(b) {
        open_chat(ui, &id);
        return;
    }
    ui.compose_page.recipients.borrow_mut().push(id);
    ui.compose_page.to.set_text("");
    render(ui);
    ui.compose_page.to.grab_focus();
}

fn remove(ui: &App, id: &str) {
    ui.compose_page.recipients.borrow_mut().retain(|x| x != id);
    render(ui);
}

/// Return in To: picks the top match, or with the field empty, starts the chat.
fn submit_to(ui: &App) {
    if ui.compose_page.to.text().trim().is_empty() {
        start(ui);
    } else if let Some(first) = matches(ui).first() {
        choose(ui, first);
    } else {
        create(ui);
    }
}

/// One bot: its chat. Several: their group chat.
fn start(ui: &App) {
    let ids = ui.compose_page.recipients.borrow().clone();
    match ids.len() {
        0 => {}
        1 => open_chat(ui, &ids[0]),
        _ => {
            let names = {
                let st = ui.state.borrow();
                ids.iter()
                    .filter_map(|id| st.bots.get(id)?["name"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let ui2 = ui.clone();
            client::call(
                "createBot",
                json!({"id": "", "kind": "group", "name": names, "members": ids}),
                move |r| match r {
                    Ok(v) => {
                        let bot = v["bot"].clone();
                        let id = bot["id"].as_str().unwrap_or_default().to_owned();
                        ui2.state.borrow_mut().bots.insert(id.clone(), bot);
                        open_chat(&ui2, &id);
                    }
                    Err(e) => ui::toast(&ui2, &e),
                },
            );
        }
    }
}

/// Opens the chat and sends whatever was typed.
fn open_chat(ui: &App, id: &str) {
    let text = ui.compose_page.draft.text().trim().to_owned();
    ui.compose_page.draft.set_text("");
    ui.compose_page.previous.borrow_mut().take();
    ui::select(ui, id);
    if !text.is_empty() {
        ui::send_text(ui, id, &text, None);
    }
}

/// "New Bot" (or the typed name) with sensible defaults: the first available agent, home folder.
fn create(ui: &App) {
    if ui.compose_page.creating.replace(true) {
        return;
    }
    render(ui);
    let typed = ui.compose_page.to.text().trim().to_owned();
    let name = if typed.is_empty() {
        "New Bot".to_owned()
    } else {
        typed
    };
    let body = {
        let st = ui.state.borrow();
        let backends = st.hello["backends"].as_array().cloned().unwrap_or_default();
        let n = uuid::Uuid::new_v4().as_u128() as usize;
        json!({
            "name": name, "description": "", "permission": "ask", "model": null, "command": null,
            "cwd": st.hello["home"].as_str().unwrap_or(""),
            "avatarShape": avatar::SHAPES[n % avatar::SHAPES.len()],
            "avatarColor": avatar::COLORS[(n / 8) % avatar::COLORS.len()].0,
            "backend": backends.iter().find(|b| b["available"] == true).and_then(|b| b["id"].as_str()).unwrap_or("claude"),
        })
    };
    let ui2 = ui.clone();
    client::call("createBot", body, move |r| {
        ui2.compose_page.creating.set(false);
        match r {
            Ok(v) => {
                let bot = v["bot"].clone();
                let id = bot["id"].as_str().unwrap_or_default().to_owned();
                ui2.state.borrow_mut().bots.insert(id.clone(), bot.clone());
                if ui2.compose_page.recipients.borrow().is_empty() {
                    open_chat(&ui2, &id);
                    // A fresh "New Bot" opens on its settings, like the Mac's details panel.
                    if bot["name"] == "New Bot" {
                        dialogs::editor(&ui2, Some(bot));
                    }
                } else {
                    choose(&ui2, &bot);
                }
            }
            Err(e) => {
                ui::toast(&ui2, &e);
                render(&ui2);
            }
        }
    });
}

pub fn wire(ui: &App) {
    let p = &ui.compose_page;
    {
        let ui2 = ui.clone();
        p.to.connect_changed(move |_| render(&ui2));
    }
    {
        let ui2 = ui.clone();
        p.to.connect_activate(move |_| submit_to(&ui2));
    }
    {
        let ui2 = ui.clone();
        p.draft.connect_activate(move |_| start(&ui2));
    }
    // Backspace in an empty To: takes the last chip back out.
    {
        let ui2 = ui.clone();
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        keys.connect_key_pressed(move |_, key, _, _| {
            let p = &ui2.compose_page;
            if key == gtk::gdk::Key::BackSpace && p.to.text().is_empty() {
                let last = p.recipients.borrow().last().cloned();
                if let Some(id) = last {
                    remove(&ui2, &id);
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::glib::Propagation::Proceed
        });
        p.to.add_controller(keys);
    }
    // Ctrl+1 creates, Ctrl+2…9 pick; Esc closes.
    {
        let ui2 = ui.clone();
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        keys.connect_key_pressed(move |_, key, _, mods| {
            if key == gtk::gdk::Key::Escape {
                close(&ui2);
                return gtk::glib::Propagation::Stop;
            }
            if mods.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && let Some(d) = key.to_unicode().and_then(|c| c.to_digit(10))
            {
                match d {
                    1 => create(&ui2),
                    2..=9 => {
                        if let Some(b) = matches(&ui2).get(d as usize - 2) {
                            choose(&ui2, b);
                        }
                    }
                    _ => return gtk::glib::Propagation::Proceed,
                }
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        p.root.add_controller(keys);
    }
    {
        let ui2 = ui.clone();
        p.close_btn.connect_clicked(move |_| close(&ui2));
    }
}
