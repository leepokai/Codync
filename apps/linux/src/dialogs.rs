//! Bot and group editors, full-conversation trace, settings/pairing and the bot menu.

use crate::client;
use crate::rows::{icon_button, label};
use crate::ui::{self, App, MenuItem};
use crate::{avatar, markup};
use adw::prelude::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

fn header_dialog(
    title: &str,
    width: i32,
    height: i32,
) -> (adw::Dialog, adw::ToolbarView, adw::HeaderBar) {
    let header = adw::HeaderBar::new();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .content_height(height)
        .child(&view)
        .build();
    (dialog, view, header)
}

// MARK: editor

pub fn editor(ui: &App, bot: Option<Value>) {
    let is_new = bot.is_none();
    let backends: Vec<Value> = ui.state.borrow().hello["backends"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let d = Rc::new(RefCell::new(bot.clone().unwrap_or_else(|| {
        let n = client_seed();
        json!({
            "name": "", "description": "", "permission": "ask", "model": null, "command": null, "cwd": "",
            "avatarShape": avatar::SHAPES[n % avatar::SHAPES.len()],
            "avatarColor": avatar::COLORS[n % avatar::COLORS.len()].0,
            "backend": backends.iter().find(|b| b["available"] == true).and_then(|b| b["id"].as_str()).unwrap_or("claude"),
        })
    })));

    let (dialog, view, header) =
        header_dialog(if is_new { "New Bot" } else { "Edit Bot" }, 560, 720);
    let save = gtk::Button::builder()
        .icon_name("object-select-symbolic")
        .tooltip_text(if is_new { "Create" } else { "Save" })
        .css_classes(["accent-fill", "circular"])
        .build();
    header.pack_end(&save);

    let page = adw::PreferencesPage::new();
    // Profile
    let preview = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .margin_bottom(8)
        .build();
    let redraw_preview = {
        let (preview, d) = (preview.clone(), d.clone());
        move || {
            while let Some(c) = preview.first_child() {
                preview.remove(&c);
            }
            let b = d.borrow();
            preview.append(&avatar::shape(
                b["avatarShape"].as_str().unwrap_or("blob"),
                b["avatarColor"].as_str().unwrap_or("blue"),
                84,
                avatar::Mood::Working,
            ));
        }
    };
    redraw_preview();
    let profile = adw::PreferencesGroup::new();
    profile.add(&preview);
    let name = adw::EntryRow::builder()
        .title("Name")
        .text(d.borrow()["name"].as_str().unwrap_or(""))
        .build();
    profile.add(&name);
    let shapes = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(8)
        .margin_top(8)
        .build();
    for s in avatar::SHAPES {
        let btn = gtk::Button::builder()
            .child(&avatar::shape(
                s,
                d.borrow()["avatarColor"].as_str().unwrap_or("blue"),
                30,
                avatar::Mood::Idle,
            ))
            .css_classes(["flat"])
            .tooltip_text(*s)
            .build();
        let (d2, redraw) = (d.clone(), redraw_preview.clone());
        btn.connect_clicked(move |_| {
            d2.borrow_mut()["avatarShape"] = (*s).into();
            redraw();
        });
        shapes.insert(&btn, -1);
    }
    profile.add(&shapes);
    let colors = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(11)
        .build();
    for (cname, hex) in avatar::COLORS {
        let swatch = gtk::DrawingArea::builder()
            .content_width(22)
            .content_height(22)
            .build();
        let hex = *hex;
        swatch.set_draw_func(move |_, cr, w, h| {
            let (r, g, b) = avatar::rgb(hex);
            cr.arc(
                w as f64 / 2.0,
                h as f64 / 2.0,
                w as f64 / 2.0,
                0.0,
                2.0 * std::f64::consts::PI,
            );
            cr.set_source_rgb(r, g, b);
            cr.fill().ok();
        });
        let btn = gtk::Button::builder()
            .child(&swatch)
            .css_classes(["flat", "circular"])
            .tooltip_text(*cname)
            .build();
        let (d2, redraw) = (d.clone(), redraw_preview.clone());
        btn.connect_clicked(move |_| {
            d2.borrow_mut()["avatarColor"] = (*cname).into();
            redraw();
        });
        colors.insert(&btn, -1);
    }
    profile.add(&colors);
    page.add(&profile);

    // Standing instructions
    let instructions_group = adw::PreferencesGroup::builder()
        .title("Standing instructions")
        .description("Rules that always apply. Put task-specific requests in the chat.")
        .build();
    let instructions = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    instructions
        .buffer()
        .set_text(d.borrow()["description"].as_str().unwrap_or(""));
    instructions_group.add(
        &gtk::Frame::builder()
            .child(
                &gtk::ScrolledWindow::builder()
                    .min_content_height(90)
                    .child(&instructions)
                    .build(),
            )
            .build(),
    );
    page.add(&instructions_group);

    // Agent: installed harnesses first, then the registry, then a custom command.
    let agent = adw::PreferencesGroup::builder()
        .title("Coding agent")
        .build();
    let mut ids: Vec<String> = vec![];
    let names = gtk::StringList::new(&[]);
    for b in &backends {
        let tag = if b["installed"] == true {
            ""
        } else if b["available"] == true {
            " — installs automatically"
        } else {
            " — needs setup"
        };
        names.append(&format!("{}{tag}", b["name"].as_str().unwrap_or("")));
        ids.push(b["id"].as_str().unwrap_or("").to_owned());
    }
    names.append("Custom ACP command");
    ids.push("custom".into());
    let current = d.borrow()["backend"].as_str().unwrap_or("").to_owned();
    let harness = adw::ComboRow::builder()
        .title("Runs on")
        .model(&names)
        .enable_search(true)
        .selected(ids.iter().position(|i| *i == current).unwrap_or(0) as u32)
        .build();
    let hint = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(["small", "dim-label"])
        .margin_top(6)
        .build();
    let command = adw::EntryRow::builder()
        .title("ACP command")
        .text(d.borrow()["command"].as_str().unwrap_or(""))
        .build();
    let model = adw::EntryRow::builder()
        .title("Model (optional)")
        .text(d.borrow()["model"].as_str().unwrap_or(""))
        .build();
    agent.add(&harness);
    agent.add(&command);
    agent.add(&model);
    agent.add(&hint);
    let sync_harness = {
        let (ids, backends, d, hint, command, harness) = (
            ids.clone(),
            backends.clone(),
            d.clone(),
            hint.clone(),
            command.clone(),
            harness.clone(),
        );
        move || {
            let id = ids
                .get(harness.selected() as usize)
                .cloned()
                .unwrap_or_default();
            command.set_visible(id == "custom");
            let b = backends.iter().find(|b| b["id"] == id.as_str());
            hint.set_label(
                b.filter(|b| b["installed"] != true)
                    .and_then(|b| b["installHint"].as_str())
                    .unwrap_or(""),
            );
            d.borrow_mut()["backend"] = id.into();
        }
    };
    sync_harness();
    harness.connect_selected_notify(move |_| sync_harness());
    page.add(&agent);

    // Project folder (native folder chooser: the host runs on this computer)
    let project = adw::PreferencesGroup::builder()
        .title("Project folder")
        .build();
    let folder_row = adw::ActionRow::builder()
        .title(
            d.borrow()["cwd"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("Choose a folder…"),
        )
        .activatable(true)
        .build();
    let choose = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Choose folder")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    folder_row.add_suffix(&choose);
    project.add(&folder_row);
    {
        let (d2, row, dialog2) = (d.clone(), folder_row.clone(), dialog.clone());
        let pick = move || {
            let chooser = gtk::FileDialog::builder().title("Project folder").build();
            let (d3, row2) = (d2.clone(), row.clone());
            let parent = dialog2.root().and_downcast::<gtk::Window>();
            chooser.select_folder(parent.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
                if let Ok(f) = res
                    && let Some(path) = f.path()
                {
                    let p = path.to_string_lossy().into_owned();
                    row2.set_title(&p);
                    d3.borrow_mut()["cwd"] = p.into();
                }
            });
        };
        let pick2 = pick.clone();
        choose.connect_clicked(move |_| pick());
        folder_row.connect_activated(move |_| pick2());
    }
    page.add(&project);

    // Permissions
    let perms = adw::PreferencesGroup::builder()
        .title("Permissions")
        .build();
    let auto = adw::SwitchRow::builder()
        .title("Approve automatically")
        .subtitle("Otherwise you get an approval card and a notification each time the agent asks.")
        .active(d.borrow()["permission"] == "auto")
        .build();
    perms.add(&auto);
    let notify = adw::SwitchRow::builder()
        .title("Notifications")
        .active(d.borrow()["notify"].as_bool().unwrap_or(true))
        .build();
    perms.add(&notify);
    let computer = adw::SwitchRow::builder()
        .title("Use the computer")
        .subtitle("Let this bot see the screen and use the mouse and keyboard (needs Remote screen in Settings).")
        .active(d.borrow()["computer"].as_bool().unwrap_or(false))
        .build();
    perms.add(&computer);
    page.add(&perms);

    view.set_content(Some(&page));
    let ui2 = ui.clone();
    let dialog2 = dialog.clone();
    save.connect_clicked(move |btn| {
        let mut body = d.borrow().clone();
        let buf = instructions.buffer();
        body["name"] = name.text().trim().into();
        body["description"] = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .trim()
            .into();
        body["permission"] = if auto.is_active() { "auto" } else { "ask" }.into();
        body["notify"] = notify.is_active().into();
        body["computer"] = computer.is_active().into();
        let m = model.text().trim().to_owned();
        body["model"] = if m.is_empty() { Value::Null } else { m.into() };
        body["command"] = if body["backend"] == "custom" {
            command.text().trim().into()
        } else {
            Value::Null
        };
        for k in [
            "status",
            "activity",
            "unread",
            "lastMessage",
            "lastAt",
            "rev",
            "deleted",
            "startedAt",
            "workingChat",
            "workingThread",
        ] {
            body.as_object_mut().unwrap().remove(k);
        }
        btn.set_sensitive(false);
        let (ui3, dialog3, btn2) = (ui2.clone(), dialog2.clone(), btn.clone());
        client::call(
            if is_new { "createBot" } else { "updateBot" },
            body,
            move |r| match r {
                Ok(v) => {
                    let bot = v["bot"].clone();
                    let id = bot["id"].as_str().unwrap_or_default().to_owned();
                    {
                        let mut st = ui3.state.borrow_mut();
                        st.bots.insert(id.clone(), bot);
                        if is_new {
                            st.current = Some(id);
                        }
                    }
                    dialog3.close();
                    ui3.split.set_show_content(true);
                    ui::schedule(&ui3);
                }
                Err(e) => {
                    btn2.set_sensitive(true);
                    ui::toast(&ui3, &e);
                }
            },
        );
    });
    dialog.present(Some(&ui.window));
}

// MARK: group editor

/// Create a group chat, or change its name, what it's for and who's in it (GroupEditorView).
/// Bots are picked like recipients: chips on top, a search, and the bots not in it yet.
pub fn group_editor(ui: &App, group: Option<Value>) {
    let is_new = group.is_none();
    let group_id = group
        .as_ref()
        .and_then(|g| g["id"].as_str())
        .map(str::to_owned);
    let members: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
        group
            .as_ref()
            .and_then(|g| g["members"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| m.as_str().map(str::to_owned))
            .collect(),
    ));
    let (dialog, body, save) = modal(
        if is_new {
            "New group chat"
        } else {
            "Group chat"
        },
        if is_new { "Create" } else { "Save" },
        420,
        560,
    );
    let preview = gtk::Box::builder().halign(gtk::Align::Center).build();
    body.append(&preview);

    let bots_label = field_label("Bots");
    let chips = gtk::Box::builder().spacing(6).build();
    let chips_scroll = gtk::ScrolledWindow::builder()
        .child(&chips)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hscrollbar_policy(gtk::PolicyType::External)
        .build();
    let search = gtk::Entry::builder().css_classes(["field-box"]).build();
    let candidates = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    let bots_field = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    bots_field.append(&bots_label);
    bots_field.append(&chips_scroll);
    bots_field.append(&search);
    bots_field.append(&candidates);
    let help = label(
        "Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools.",
        &["footnote", "tertiary"],
    );
    help.set_wrap(true);
    bots_field.append(&help);
    body.append(&bots_field);

    let name = gtk::Entry::builder()
        .css_classes(["field-box"])
        .text(
            group
                .as_ref()
                .and_then(|g| g["name"].as_str())
                .unwrap_or(""),
        )
        .build();
    body.append(&field("Name", &name));
    let about = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    about.buffer().set_text(
        group
            .as_ref()
            .and_then(|g| g["description"].as_str())
            .unwrap_or(""),
    );
    body.append(&field(
        "About",
        &text_area(&about, "What this group works on (optional)", 2, 5),
    ));
    if let Some(g) = group.as_ref() {
        let del = gtk::Button::builder()
            .label("Delete group chat")
            .css_classes(["secondary-pill", "danger-text"])
            .halign(gtk::Align::Start)
            .margin_top(8)
            .build();
        let (ui2, dialog2, id, gname) = (
            ui.clone(),
            dialog.clone(),
            g["id"].as_str().unwrap_or_default().to_owned(),
            g["name"].as_str().unwrap_or("").to_owned(),
        );
        del.connect_clicked(move |_| {
            dialog2.close();
            confirm_delete(&ui2, &id, &gname, true);
        });
        body.append(&del);
    }

    let names_of = {
        let ui2 = ui.clone();
        move |ids: &[String]| -> Vec<String> {
            let st = ui2.state.borrow();
            ids.iter()
                .filter_map(|id| st.bots.get(id)?["name"].as_str().map(str::to_owned))
                .collect()
        }
    };
    // Redraws the avatar, chips, candidates and default name from `members` and the search.
    let sync: Rc<RefCell<Box<dyn Fn()>>> = Rc::new(RefCell::new(Box::new(|| {})));
    let toggle = {
        let (members, sync, search) = (members.clone(), Rc::downgrade(&sync), search.clone());
        move |id: &str| {
            {
                let mut m = members.borrow_mut();
                if m.iter().any(|x| x == id) {
                    m.retain(|x| x != id);
                } else {
                    m.push(id.to_owned());
                }
            }
            search.set_text("");
            if let Some(sync) = sync.upgrade() {
                (sync.borrow())();
            }
        }
    };
    let toggle = Rc::new(toggle);
    *sync.borrow_mut() = {
        let (ui2, members, chips, chips_scroll, candidates, search, name, save, toggle) = (
            ui.clone(),
            members.clone(),
            chips.clone(),
            chips_scroll.clone(),
            candidates.clone(),
            search.clone(),
            name.clone(),
            save.clone(),
            toggle.clone(),
        );
        let names_of = names_of.clone();
        let preview = preview.clone();
        Box::new(move || {
            let m = members.borrow().clone();
            let st = ui2.state.borrow();
            ui::clear(&preview);
            if m.is_empty() {
                let icon = gtk::Image::builder()
                    .icon_name("system-users-symbolic")
                    .pixel_size(29)
                    .width_request(72)
                    .height_request(72)
                    .css_classes(["secondary"])
                    .build();
                preview.append(&icon);
            } else {
                preview.append(&avatar::of(
                    &st.bots,
                    &json!({"kind": "group", "members": m}),
                    72,
                    false,
                ));
            }
            bots_label.set_label(&format!("Bots · {}", m.len()));
            ui::clear(&chips);
            chips_scroll.set_visible(!m.is_empty());
            for id in &m {
                if let Some(b) = st.bots.get(id) {
                    let (t, id) = (toggle.clone(), id.clone());
                    chips.append(&crate::compose::bot_chip(&st.bots, b, move || t(&id)));
                }
            }
            search.set_placeholder_text(Some(if m.is_empty() {
                "Search bots"
            } else {
                "Add another bot"
            }));
            let q = search.text().trim().to_lowercase();
            ui::clear(&candidates);
            for b in ui::roster(&st).into_iter().filter(|b| {
                !ui::is_group(b)
                    && !m.iter().any(|x| b["id"] == x.as_str())
                    && (q.is_empty()
                        || b["name"]
                            .as_str()
                            .is_some_and(|n| n.to_lowercase().contains(&q)))
            }) {
                let row = gtk::Box::builder().spacing(12).build();
                row.append(&avatar::of(&st.bots, b, 26, false));
                let text = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(1)
                    .hexpand(true)
                    .build();
                let bname = b["name"].as_str().unwrap_or("");
                text.append(&label(bname, &["body13"]));
                text.append(&label(
                    &ui::folder(b["cwd"].as_str().unwrap_or("")),
                    &["footnote", "tertiary"],
                ));
                row.append(&text);
                let plus = gtk::Image::from_icon_name("list-add-symbolic");
                plus.add_css_class("tertiary");
                row.append(&plus);
                let btn = gtk::Button::builder()
                    .child(&row)
                    .css_classes(["candidate"])
                    .tooltip_text(format!("Add {bname}"))
                    .build();
                let (t, id) = (toggle.clone(), b["id"].as_str().unwrap_or("").to_owned());
                btn.connect_clicked(move |_| t(&id));
                candidates.append(&btn);
            }
            drop(st);
            let names = names_of(&m);
            name.set_placeholder_text(Some(&if names.is_empty() {
                "Name".to_owned()
            } else {
                names.join(", ")
            }));
            save.set_sensitive(!m.is_empty());
        })
    };
    {
        let sync = sync.clone();
        search.connect_changed(move |_| (sync.borrow())());
    }
    {
        let (ui2, members, toggle) = (ui.clone(), members.clone(), toggle.clone());
        search.connect_activate(move |s| {
            let q = s.text().trim().to_lowercase();
            let first = {
                let st = ui2.state.borrow();
                let m = members.borrow();
                ui::roster(&st)
                    .into_iter()
                    .find(|b| {
                        !ui::is_group(b)
                            && !m.iter().any(|x| b["id"] == x.as_str())
                            && b["name"]
                                .as_str()
                                .is_some_and(|n| n.to_lowercase().contains(&q))
                    })
                    .and_then(|b| b["id"].as_str().map(str::to_owned))
            };
            if let Some(id) = first {
                toggle(&id);
            }
        });
    }
    (sync.borrow())();

    let (ui2, dialog2) = (ui.clone(), dialog.clone());
    save.connect_clicked(move |btn| {
        let m = members.borrow().clone();
        let typed = name.text().trim().to_owned();
        let final_name = if typed.is_empty() {
            names_of(&m).join(", ")
        } else {
            typed
        };
        let buf = about.buffer();
        let description = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .trim()
            .to_owned();
        let (method, body) = match &group_id {
            Some(id) => (
                "updateBot",
                json!({"id": id, "name": final_name, "members": m, "description": description}),
            ),
            None => (
                "createBot",
                json!({"id": "", "kind": "group", "name": final_name, "members": m, "description": description}),
            ),
        };
        btn.set_sensitive(false);
        let (ui3, dialog3, btn2) = (ui2.clone(), dialog2.clone(), btn.clone());
        client::call(method, body, move |r| match r {
            Ok(v) => {
                let bot = v["bot"].clone();
                let id = bot["id"].as_str().unwrap_or_default().to_owned();
                ui3.state.borrow_mut().bots.insert(id.clone(), bot);
                dialog3.close();
                if is_new {
                    ui::select(&ui3, &id);
                } else {
                    ui::schedule(&ui3);
                }
            }
            Err(e) => {
                btn2.set_sensitive(true);
                ui::toast(&ui3, &e);
            }
        });
    });
    dialog.present(Some(&ui.window));
}

/// A sheet with Codync's modal header (title, a check to save, close) over a scrolling body.
fn modal(
    title: &str,
    save_tip: &str,
    width: i32,
    height: i32,
) -> (adw::Dialog, gtk::Box, gtk::Button) {
    let save = icon_button("object-select-symbolic", save_tip);
    let close = icon_button("window-close-symbolic", "Close");
    let head = gtk::Box::builder()
        .spacing(8)
        .margin_start(16)
        .margin_end(10)
        .height_request(48)
        .build();
    let t = label(title, &["semibold"]);
    t.set_hexpand(true);
    head.append(&t);
    head.append(&save);
    head.append(&close);
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(16)
        .margin_end(16)
        .margin_top(4)
        .margin_bottom(16)
        .build();
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    col.append(&head);
    col.append(
        &gtk::ScrolledWindow::builder()
            .child(&body)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .content_height(height)
        .child(&col)
        .build();
    {
        let d = dialog.clone();
        close.connect_clicked(move |_| {
            d.close();
        });
    }
    (dialog, body, save)
}

fn field_label(text: &str) -> gtk::Label {
    label(text, &["field-label"])
}

/// A labelled field (the editors' `Field`).
fn field(title: &str, content: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    b.append(&field_label(title));
    b.append(content);
    b
}

/// A multi-line field box with a placeholder, `min`…`max` lines tall.
fn text_area(view: &gtk::TextView, placeholder: &str, min: i32, max: i32) -> gtk::Box {
    let hint = label(placeholder, &["tertiary"]);
    hint.set_valign(gtk::Align::Start);
    hint.set_can_target(false);
    let buf = view.buffer();
    hint.set_visible(buf.char_count() == 0);
    {
        let hint = hint.clone();
        buf.connect_changed(move |b| hint.set_visible(b.char_count() == 0));
    }
    let scroll = gtk::ScrolledWindow::builder()
        .child(view)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(min * 16)
        .max_content_height(max * 16)
        .propagate_natural_height(true)
        .build();
    let overlay = gtk::Overlay::builder().child(&scroll).build();
    overlay.add_overlay(&hint);
    let b = gtk::Box::builder().css_classes(["field-box-area"]).build();
    overlay.set_hexpand(true);
    b.append(&overlay);
    b
}

fn confirm(
    ui: &App,
    title: &str,
    body: &str,
    action: &str,
    destructive: bool,
    f: impl Fn() + 'static,
) {
    let alert = adw::AlertDialog::new(Some(title), Some(body));
    alert.add_responses(&[("cancel", "Cancel"), ("ok", action)]);
    if destructive {
        alert.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    }
    alert.connect_response(None, move |_, r| {
        if r == "ok" {
            f();
        }
    });
    alert.present(Some(&ui.window));
}

fn confirm_delete(ui: &App, id: &str, name: &str, group: bool) {
    let id = id.to_owned();
    confirm(
        ui,
        &format!("Delete {name}?"),
        if group {
            "The group chat is removed. Its bots stay."
        } else {
            "Files it changed on your computer stay as they are."
        },
        if group {
            "Delete group chat"
        } else {
            "Delete bot and its conversation"
        },
        true,
        move || client::call("deleteBot", json!({"botId": id}), |_| {}),
    );
}

fn client_seed() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
}

// MARK: trace

pub fn trace(ui: &App) {
    let st = ui.state.borrow();
    let Some(id) = st.current.clone() else { return };
    let (dialog, view, _) = header_dialog("Full conversation", 560, 760);
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .margin_bottom(16)
        .build();
    let entries = st.entries.get(&id).cloned().unwrap_or_default();
    // Main chat first, then each thread under its root's text.
    for (thread, lane) in lanes(&entries) {
        if let Some(root) = thread {
            let text = entries
                .iter()
                .find(|x| x["id"] == root)
                .and_then(|x| x["data"]["text"].as_str())
                .unwrap_or("");
            list.append(
                &gtk::Label::builder()
                    .label(format!("Thread · {}", short(text, 60)))
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .css_classes(["title-4"])
                    .margin_top(16)
                    .build(),
            );
        }
        trace_lane(&list, &lane);
    }
    view.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build(),
    ));
    dialog.present(Some(&ui.window));
}

fn short(text: &str, n: usize) -> String {
    let line = text.lines().next().unwrap_or("");
    if line.chars().count() > n || text.contains('\n') {
        format!("{}…", line.chars().take(n).collect::<String>())
    } else {
        line.to_owned()
    }
}

/// Entries split by lane: the main chat (`None`) first, then threads in the order
/// they first appear, each keeping its entries' order.
fn lanes(entries: &[Value]) -> Vec<(Option<String>, Vec<&Value>)> {
    let mut out: Vec<(Option<String>, Vec<&Value>)> = vec![(None, vec![])];
    for e in entries {
        let t = e["threadId"].as_str().map(str::to_owned);
        match out.iter_mut().find(|(k, _)| *k == t) {
            Some((_, l)) => l.push(e),
            None => out.push((t, vec![e])),
        }
    }
    out
}

/// One lane's entries grouped by turn, each group titled by that turn's message.
fn trace_lane(list: &gtk::Box, entries: &[&Value]) {
    let mut turn = -1i64;
    let mut group = adw::PreferencesGroup::new();
    for e in entries {
        let t = e["turn"].as_i64().unwrap_or(0);
        if t != turn {
            turn = t;
            let title = entries
                .iter()
                .find(|x| x["turn"] == t && x["kind"] == "user")
                .and_then(|x| x["data"]["text"].as_str())
                .unwrap_or("");
            group = adw::PreferencesGroup::builder()
                .title(gtk::glib::markup_escape_text(
                    &title.chars().take(80).collect::<String>(),
                ))
                .build();
            list.append(&group);
        }
        let d = &e["data"];
        let widget: gtk::Widget = match e["kind"].as_str().unwrap_or("") {
            "tool" => {
                let status = match d["status"].as_str() {
                    Some("completed") => "✓",
                    Some("failed") => "✕",
                    _ => "…",
                };
                let row = adw::ExpanderRow::builder()
                    .title(gtk::glib::markup_escape_text(
                        d["title"].as_str().unwrap_or("Tool"),
                    ))
                    .subtitle(status)
                    .build();
                let mut body = String::new();
                for diff in d["diffs"].as_array().into_iter().flatten() {
                    body.push_str(&format!(
                        "{}  +{} −{}\n{}\n",
                        diff["path"].as_str().unwrap_or(""),
                        diff["added"],
                        diff["removed"],
                        diff["patch"].as_str().unwrap_or("")
                    ));
                }
                body.push_str(d["output"].as_str().unwrap_or(""));
                if !body.trim().is_empty() {
                    row.add_row(
                        &gtk::Label::builder()
                            .label(body.trim())
                            .xalign(0.0)
                            .wrap(true)
                            .selectable(true)
                            .css_classes(["codebox"])
                            .margin_start(8)
                            .margin_end(8)
                            .margin_top(6)
                            .margin_bottom(6)
                            .build(),
                    );
                }
                row.upcast()
            }
            "thought" => {
                let row = adw::ExpanderRow::builder().title("Thinking").build();
                row.add_row(
                    &gtk::Label::builder()
                        .label(d["text"].as_str().unwrap_or(""))
                        .xalign(0.0)
                        .wrap(true)
                        .css_classes(["dim-label"])
                        .margin_start(8)
                        .margin_end(8)
                        .margin_top(6)
                        .margin_bottom(6)
                        .build(),
                );
                row.upcast()
            }
            "agent" => gtk::Label::builder()
                .use_markup(true)
                .label(markup::to_pango(d["text"].as_str().unwrap_or("")))
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .margin_start(12)
                .margin_end(12)
                .margin_top(8)
                .margin_bottom(8)
                .build()
                .upcast(),
            "plan" => {
                let text: Vec<String> = d["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        format!(
                            "{} {}",
                            if p["status"] == "completed" {
                                "✅"
                            } else {
                                "○"
                            },
                            p["content"].as_str().unwrap_or("")
                        )
                    })
                    .collect();
                adw::ActionRow::builder()
                    .title("Plan")
                    .subtitle(gtk::glib::markup_escape_text(&text.join("\n")))
                    .build()
                    .upcast()
            }
            "user" => adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(
                    d["text"].as_str().unwrap_or(""),
                ))
                .build()
                .upcast(),
            _ => adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(
                    d["title"].as_str().or(d["text"].as_str()).unwrap_or(""),
                ))
                .build()
                .upcast(),
        };
        group.add(&widget);
    }
}

// MARK: menus

/// A bot's actions: from the roster (right-click) like the Mac sidebar's menu, or from
/// the chat header's More button.
pub fn bot_menu(ui: &App, parent: &gtk::Widget, point: Option<(f64, f64)>, id: &str, roster: bool) {
    let Some(bot) = ui.state.borrow().bots.get(id).cloned() else {
        return;
    };
    let group = bot["kind"] == "group";
    let pinned = bot["pinned"].as_bool().unwrap_or(false);
    let name = bot["name"].as_str().unwrap_or("").to_owned();
    let mut items = vec![];
    if !roster {
        let ui2 = ui.clone();
        items.push(MenuItem::new(
            "view-list-symbolic",
            "Full conversation",
            Box::new(move || trace(&ui2)),
        ));
    }
    {
        let id2 = id.to_owned();
        items.push(MenuItem::new(
            "view-pin-symbolic",
            if pinned { "Unpin" } else { "Pin" },
            Box::new(move || {
                client::call("updateBot", json!({"id": id2, "pinned": !pinned}), |_| {});
            }),
        ));
    }
    if roster {
        let id2 = id.to_owned();
        items.push(MenuItem::new(
            "mail-unread-symbolic",
            "Mark as Read",
            Box::new(move || client::call("markRead", json!({"botId": id2}), |_| {})),
        ));
    }
    {
        let (ui2, bot2) = (ui.clone(), bot.clone());
        let item = MenuItem::new(
            if group {
                "system-users-symbolic"
            } else {
                "document-edit-symbolic"
            },
            if group {
                "Edit group…"
            } else {
                "Edit profile…"
            },
            Box::new(move || {
                if group {
                    group_editor(&ui2, Some(bot2.clone()));
                } else {
                    editor(&ui2, Some(bot2.clone()));
                }
            }),
        );
        items.push(if roster { item.divider() } else { item });
    }
    if roster {
        let id2 = id.to_owned();
        items.push(
            MenuItem::new(
                "edit-copy-symbolic",
                "Copy conversation ID",
                Box::new(move || {
                    if let Some(d) = gtk::gdk::Display::default() {
                        d.clipboard().set_text(&id2);
                    }
                }),
            )
            .divider(),
        );
    }
    if !group {
        let (ui2, id2) = (ui.clone(), id.to_owned());
        items.push(MenuItem::new(
            "view-refresh-symbolic",
            "New session",
            Box::new(move || {
                let id3 = id2.clone();
                confirm(
                    &ui2,
                    "Start a new session?",
                    "The conversation stays here, but the agent starts with a fresh context.",
                    "New session",
                    false,
                    move || client::call("newSession", json!({"botId": id3}), |_| {}),
                );
            }),
        ));
    }
    if roster {
        let id2 = id.to_owned();
        items.push(
            MenuItem::new(
                "view-conceal-symbolic",
                "Hide from sidebar",
                Box::new(move || {
                    client::call("updateBot", json!({"id": id2, "hidden": true}), |_| {});
                }),
            )
            .divider(),
        );
    }
    {
        let (ui2, id2) = (ui.clone(), id.to_owned());
        let item = MenuItem::new(
            "user-trash-symbolic",
            if group { "Delete group" } else { "Delete" },
            Box::new(move || confirm_delete(&ui2, &id2, &name, group)),
        )
        .destructive();
        items.push(if roster { item } else { item.divider() });
    }
    ui::popup_menu(parent, point, items);
}

/// The sidebar's Account panel: usage, this computer and its devices, the mobile app, help.
pub fn account_menu(ui: &App, anchor: &gtk::Button) {
    let usage = {
        let st = ui.state.borrow();
        st.usage["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| p["windows"].as_array().into_iter().flatten())
            .filter_map(|w| w["percent"].as_f64())
            .fold(None, |m: Option<f64>, p| Some(m.map_or(p, |m| m.max(p))))
    };
    let open = |url: &'static str| -> Box<dyn Fn()> {
        Box::new(move || {
            gtk::UriLauncher::new(url).launch(
                None::<&gtk::Window>,
                gtk::gio::Cancellable::NONE,
                |_| {},
            );
        })
    };
    let (ui2, ui3) = (ui.clone(), ui.clone());
    let items = vec![
        MenuItem::new(
            "power-profile-balanced-symbolic",
            "Usage",
            Box::new(move || usage_sheet(&ui2)),
        )
        .detail(usage.map(|u| format!("{u:.0}%")))
        .chevron(),
        MenuItem::new(
            "computer-symbolic",
            "Computers & devices",
            Box::new(move || settings(&ui3)),
        )
        .chevron(),
        MenuItem::new(
            "phone-symbolic",
            "Get Codync for mobile",
            open("https://apps.apple.com/app/id6760984418"),
        ),
        MenuItem::new(
            "help-browser-symbolic",
            "Help & documentation",
            open("https://github.com/leepokai/codync#readme"),
        )
        .divider(),
        MenuItem::new(
            "dialog-warning-symbolic",
            "Report an issue",
            open("https://github.com/leepokai/codync/issues"),
        ),
    ];
    ui::popup_menu(anchor, None, items);
}

/// Usage limits per provider (UsageSheet).
fn usage_sheet(ui: &App) {
    let (dialog, view, _) = header_dialog("Usage", 520, 460);
    let page = adw::PreferencesPage::new();
    let usage = adw::PreferencesGroup::new();
    let st = ui.state.borrow();
    let mut any = false;
    for p in st.usage["providers"].as_array().into_iter().flatten() {
        for w in p["windows"].as_array().into_iter().flatten() {
            any = true;
            let reset = w["resetsText"]
                .as_str()
                .map(|s| format!("resets {s}"))
                .unwrap_or_default();
            let row = adw::ActionRow::builder()
                .title(format!(
                    "{} {}",
                    p["name"].as_str().unwrap_or(""),
                    w["label"].as_str().unwrap_or("")
                ))
                .subtitle(gtk::glib::markup_escape_text(&reset))
                .build();
            let pct = w["percent"].as_f64().unwrap_or(0.0);
            row.add_suffix(
                &gtk::LevelBar::builder()
                    .min_value(0.0)
                    .max_value(100.0)
                    .value(pct)
                    .width_request(120)
                    .valign(gtk::Align::Center)
                    .build(),
            );
            row.add_suffix(&gtk::Label::new(Some(&format!("{pct:.0}%"))));
            usage.add(&row);
        }
    }
    if !any {
        usage.add(&label("No usage information yet.", &["secondary"]));
    }
    page.add(&usage);
    view.set_content(Some(&page));
    dialog.present(Some(&ui.window));
}

// MARK: settings & pairing

/// Remote screen: see and control this computer from the iPhone, and let bots use it.
/// Only this computer can turn it on (the host checks the request comes from itself).
fn remote_screen_group() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Remote screen")
        .description("See and control this computer from your iPhone, and let bots use it. The first time, your desktop asks you to allow screen sharing.")
        .build();
    let row = adw::SwitchRow::builder()
        .title("Allow remote screen")
        .build();
    let status = adw::ActionRow::builder().title("Status").build();
    status.set_visible(false);
    group.add(&row);
    group.add(&status);
    let show = {
        let status = status.clone();
        move |v: &Value| {
            let text = if v["enabled"] != true {
                None
            } else if v["connected"] != true {
                Some("Waiting for screen sharing approval…")
            } else if v["input"] != true {
                Some("Viewing only: control wasn't allowed")
            } else {
                Some("Ready")
            };
            status.set_visible(text.is_some());
            status.set_subtitle(text.unwrap_or_default());
        }
    };
    {
        let row = row.clone();
        let show = show.clone();
        client::call("screenStatus", client::empty(), move |r| {
            if let Ok(v) = r {
                row.set_active(v["enabled"] == true);
                show(&v);
            } else {
                // A host from before Remote screen.
                row.set_sensitive(false);
            }
        });
    }
    row.connect_active_notify(move |row| {
        let show = show.clone();
        client::call(
            "setScreenEnabled",
            json!({"enabled": row.is_active()}),
            move |r| {
                if let Ok(v) = r {
                    show(&v);
                }
            },
        );
    });
    group
}

pub fn settings(ui: &App) {
    let (dialog, view, _) = header_dialog("Computers & devices", 520, 720);
    let page = adw::PreferencesPage::new();
    let pair = adw::PreferencesGroup::builder()
        .title("Pair your iPhone")
        .description("Scan with the Codync app or the iPhone Camera.")
        .build();
    let qr_area = gtk::DrawingArea::builder()
        .content_width(220)
        .content_height(220)
        .halign(gtk::Align::Center)
        .margin_top(8)
        .build();
    pair.add(&qr_area);
    let copy = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy pairing link")
        .css_classes(["flat", "circular"])
        .halign(gtk::Align::Center)
        .margin_top(8)
        .build();
    pair.add(&copy);
    page.add(&pair);
    client::call("pairing", client::empty(), move |r| {
        if let Ok(v) = r
            && let Some(url) = v["pairingUrl"].as_str()
            && let Ok(code) = qrcode::QrCode::new(url.as_bytes())
        {
            let width = code.width();
            let dark: Vec<bool> = code
                .to_colors()
                .iter()
                .map(|c| *c == qrcode::Color::Dark)
                .collect();
            qr_area.set_draw_func(move |_, cr, w, _| {
                let cell = w as f64 / (width + 4) as f64;
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.paint().ok();
                cr.set_source_rgb(0.0, 0.0, 0.0);
                for (i, on) in dark.iter().enumerate() {
                    if *on {
                        cr.rectangle(
                            ((i % width) + 2) as f64 * cell,
                            ((i / width) + 2) as f64 * cell,
                            cell,
                            cell,
                        );
                    }
                }
                cr.fill().ok();
            });
            qr_area.queue_draw();
            let url = url.to_owned();
            copy.connect_clicked(move |b| b.clipboard().set_text(&url));
        }
    });

    page.add(&remote_screen_group());

    let st = ui.state.borrow();
    let agents = adw::PreferencesGroup::builder()
        .title("Coding agents on this computer")
        .build();
    for b in st.hello["backends"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["installed"] == true)
    {
        agents.add(
            &adw::ActionRow::builder()
                .title(b["name"].as_str().unwrap_or(""))
                .subtitle(gtk::glib::markup_escape_text(
                    b["path"].as_str().unwrap_or(""),
                ))
                .build(),
        );
    }
    let rescan = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Re-scan this computer")
        .css_classes(["flat"])
        .build();
    agents.set_header_suffix(Some(&rescan));
    {
        let (ui2, dialog2) = (ui.clone(), dialog.clone());
        rescan.connect_clicked(move |_| {
            let (ui3, dialog3) = (ui2.clone(), dialog2.clone());
            client::call("refreshBackends", client::empty(), move |r| {
                if let Ok(v) = r {
                    ui3.state.borrow_mut().hello["backends"] = v["backends"].clone();
                }
                dialog3.close();
                settings(&ui3);
            });
        });
    }
    page.add(&agents);

    let about = adw::PreferencesGroup::new();
    about.add(
        &adw::ActionRow::builder()
            .title("codync-host")
            .subtitle(format!(
                "{} · {}",
                st.hello["version"].as_str().unwrap_or(""),
                st.hello["os"].as_str().unwrap_or("")
            ))
            .build(),
    );
    page.add(&about);
    view.set_content(Some(&page));
    dialog.present(Some(&ui.window));
}

/// Shown when the host isn't running: offer to install/start it.
pub fn host_missing(ui: &App) {
    let status = adw::StatusPage::builder()
        .title("Start the Codync host")
        .description(
            "Codync runs your coding agents through a small background service on this computer.",
        )
        .icon_name("computer-symbolic")
        .build();
    let btn = gtk::Button::builder()
        .label("Install & start host")
        .css_classes(["pill", "accent-fill"])
        .halign(gtk::Align::Center)
        .build();
    status.set_child(Some(&btn));
    let (dialog, view, _) = header_dialog("Codync", 460, 420);
    view.set_content(Some(&status));
    let (ui2, dialog2) = (ui.clone(), dialog.clone());
    btn.connect_clicked(move |b| {
        b.set_sensitive(false);
        let (ui3, dialog3, b2) = (ui2.clone(), dialog2.clone(), b.clone());
        client::install_host(move |r| match r {
            Ok(()) => {
                dialog3.close();
                gtk::glib::timeout_add_seconds_local_once(1, move || ui::reconnect(&ui3));
            }
            Err(e) => {
                b2.set_sensitive(true);
                ui::toast(&ui3, &e);
            }
        });
    });
    dialog.present(Some(&ui.window));
}

#[cfg(test)]
mod tests {
    use super::lanes;
    use serde_json::json;

    #[test]
    fn trace_lanes() {
        let e = [
            json!({"id": "a"}),
            json!({"id": "b", "threadId": "a"}),
            json!({"id": "c"}),
            json!({"id": "d", "threadId": "c"}),
            json!({"id": "e", "threadId": "a"}),
        ];
        let ids = |l: &[&serde_json::Value]| {
            l.iter()
                .map(|x| x["id"].as_str().unwrap_or(""))
                .collect::<String>()
        };
        let l = lanes(&e);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].0.as_deref(), ids(&l[0].1).as_str()), (None, "ac"));
        assert_eq!(
            (l[1].0.as_deref(), ids(&l[1].1).as_str()),
            (Some("a"), "be")
        );
        assert_eq!((l[2].0.as_deref(), ids(&l[2].1).as_str()), (Some("c"), "d"));
    }
}
