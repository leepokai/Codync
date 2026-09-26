//! Bot and group editors, full-conversation trace, settings/pairing and the bot menu.

use crate::client;
use crate::ui::{self, App};
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
            preview.append(&avatar::widget(
                b["avatarShape"].as_str().unwrap_or("blob"),
                b["avatarColor"].as_str().unwrap_or("blue"),
                84,
                true,
                "",
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
            .child(&avatar::widget(
                s,
                d.borrow()["avatarColor"].as_str().unwrap_or("blue"),
                30,
                false,
                "",
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

const MAX_MEMBERS: usize = 6;

/// Create a group chat, or rename one and change who's in it (up to six bots).
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
    let (dialog, view, header) = header_dialog(
        if is_new {
            "New group chat"
        } else {
            "Group chat"
        },
        480,
        620,
    );
    let save = gtk::Button::builder()
        .icon_name("object-select-symbolic")
        .tooltip_text(if is_new { "Create" } else { "Save" })
        .css_classes(["accent-fill", "circular"])
        .build();
    header.pack_end(&save);

    let page = adw::PreferencesPage::new();
    let profile = adw::PreferencesGroup::new();
    let name = adw::EntryRow::builder()
        .title("Name")
        .text(
            group
                .as_ref()
                .and_then(|g| g["name"].as_str())
                .unwrap_or(""),
        )
        .build();
    profile.add(&name);
    page.add(&profile);

    let bots_group = adw::PreferencesGroup::builder()
        .description("Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools.")
        .build();
    let st = ui.state.borrow();
    let mut candidates: Vec<&Value> = st
        .bots
        .values()
        .filter(|b| {
            b["kind"] != "group"
                && (b["hidden"] != true
                    || b["id"]
                        .as_str()
                        .is_some_and(|id| members.borrow().iter().any(|m| m == id)))
        })
        .collect();
    candidates.sort_by_key(|b| b["name"].as_str().unwrap_or("").to_lowercase());
    let mut checks: Vec<(String, String, gtk::CheckButton)> = vec![];
    for b in candidates {
        let id = b["id"].as_str().unwrap_or_default().to_owned();
        let bname = b["name"].as_str().unwrap_or("").to_owned();
        let check = gtk::CheckButton::builder()
            .active(members.borrow().contains(&id))
            .valign(gtk::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(gtk::glib::markup_escape_text(&bname))
            .subtitle(gtk::glib::markup_escape_text(&ui::folder(
                b["cwd"].as_str().unwrap_or(""),
            )))
            .activatable_widget(&check)
            .build();
        row.add_prefix(&avatar::of(&st.bots, b, 30, false, ""));
        row.add_suffix(&check);
        bots_group.add(&row);
        checks.push((id, bname, check));
    }
    drop(st);
    page.add(&bots_group);
    let checks = Rc::new(checks);
    // "Alice, Bob" when no name is typed; at most six; nothing to save without a bot.
    let sync = {
        let (members, checks, bots_group, name, save) = (
            members.clone(),
            checks.clone(),
            bots_group.clone(),
            name.clone(),
            save.clone(),
        );
        move || {
            let m = members.borrow();
            bots_group.set_title(&format!("Bots · {} of {MAX_MEMBERS}", m.len()));
            for (_, _, check) in checks.iter() {
                check.set_sensitive(check.is_active() || m.len() < MAX_MEMBERS);
            }
            let names: Vec<&str> = m
                .iter()
                .filter_map(|id| checks.iter().find(|c| c.0 == *id).map(|c| c.1.as_str()))
                .collect();
            name.set_title(
                if names.is_empty() {
                    "Name".to_owned()
                } else {
                    format!("Name · {}", names.join(", "))
                }
                .as_str(),
            );
            save.set_sensitive(!m.is_empty());
        }
    };
    for (id, _, check) in checks.iter() {
        let (members, sync, id) = (members.clone(), sync.clone(), id.clone());
        check.connect_toggled(move |c| {
            {
                let mut m = members.borrow_mut();
                m.retain(|x| *x != id);
                if c.is_active() {
                    m.push(id.clone());
                }
            }
            sync();
        });
    }
    sync();

    if let Some(g) = group.as_ref() {
        let danger = adw::PreferencesGroup::new();
        let del = gtk::Button::builder()
            .label("Delete group chat")
            .css_classes(["destructive-action"])
            .halign(gtk::Align::Center)
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
        danger.add(&del);
        page.add(&danger);
    }
    view.set_content(Some(&page));

    let (ui2, dialog2) = (ui.clone(), dialog.clone());
    save.connect_clicked(move |btn| {
        let m = members.borrow().clone();
        let typed = name.text().trim().to_owned();
        let final_name = if typed.is_empty() {
            m.iter()
                .filter_map(|id| checks.iter().find(|c| c.0 == *id).map(|c| c.1.clone()))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            typed
        };
        let (method, body) = match &group_id {
            Some(id) => (
                "updateBot",
                json!({"id": id, "name": final_name, "members": m}),
            ),
            None => (
                "createBot",
                json!({"id": "", "kind": "group", "name": final_name, "members": m}),
            ),
        };
        btn.set_sensitive(false);
        let (ui3, dialog3, btn2) = (ui2.clone(), dialog2.clone(), btn.clone());
        client::call(method, body, move |r| match r {
            Ok(v) => {
                let bot = v["bot"].clone();
                let id = bot["id"].as_str().unwrap_or_default().to_owned();
                {
                    let mut st = ui3.state.borrow_mut();
                    st.bots.insert(id.clone(), bot);
                    if is_new {
                        st.current = Some(id);
                        st.open_thread = None;
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
        });
    });
    dialog.present(Some(&ui.window));
}

fn confirm_delete(ui: &App, id: &str, name: &str, group: bool) {
    let alert = adw::AlertDialog::new(
        Some(&format!("Delete {name}?")),
        Some(if group {
            "The group chat is removed. Its bots stay."
        } else {
            "The conversation is removed. Files it changed stay as they are."
        }),
    );
    alert.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
    alert.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    let id = id.to_owned();
    alert.connect_response(None, move |_, r| {
        if r == "delete" {
            client::call("deleteBot", json!({"botId": id}), |_| {});
        }
    });
    alert.present(Some(&ui.window));
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
    let mut turn = -1i64;
    let mut group = adw::PreferencesGroup::new();
    for e in &entries {
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
    view.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build(),
    ));
    dialog.present(Some(&ui.window));
}

// MARK: bot menu

pub fn bot_menu(ui: &App) -> gtk::Popover {
    let pop = gtk::Popover::new();
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    let bot = {
        let st = ui.state.borrow();
        st.current.as_ref().and_then(|id| st.bots.get(id)).cloned()
    };
    let Some(bot) = bot else { return pop };
    let id = bot["id"].as_str().unwrap_or_default().to_owned();
    let group = bot["kind"] == "group";
    let add = |label: &str, destructive: bool, f: Box<dyn Fn()>| {
        let b = gtk::Button::builder()
            .label(label)
            .css_classes(["flat"])
            .build();
        if destructive {
            b.add_css_class("error");
        }
        let pop2 = pop.clone();
        b.connect_clicked(move |_| {
            pop2.popdown();
            f();
        });
        col.append(&b);
    };
    {
        let (ui2, bot2) = (ui.clone(), bot.clone());
        if group {
            add(
                "Edit group",
                false,
                Box::new(move || group_editor(&ui2, Some(bot2.clone()))),
            );
        } else {
            add(
                "Edit profile",
                false,
                Box::new(move || editor(&ui2, Some(bot2.clone()))),
            );
        }
    }
    {
        let (id2, pinned) = (id.clone(), bot["pinned"].as_bool().unwrap_or(false));
        add(
            if pinned { "Unpin" } else { "Pin" },
            false,
            Box::new(move || {
                client::call("updateBot", json!({"id": id2, "pinned": !pinned}), |_| {})
            }),
        );
    }
    if !group {
        let id2 = id.clone();
        add(
            "New session",
            false,
            Box::new(move || client::call("newSession", json!({"botId": id2}), |_| {})),
        );
    }
    {
        let (ui2, id2, name) = (
            ui.clone(),
            id.clone(),
            bot["name"].as_str().unwrap_or("").to_owned(),
        );
        add(
            if group {
                "Delete group…"
            } else {
                "Delete bot…"
            },
            true,
            Box::new(move || confirm_delete(&ui2, &id2, &name, group)),
        );
    }
    pop.set_child(Some(&col));
    pop
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
    let (dialog, view, _) = header_dialog("Settings", 520, 720);
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

    let usage = adw::PreferencesGroup::builder()
        .title("Usage limits")
        .build();
    for p in st.usage["providers"].as_array().into_iter().flatten() {
        for w in p["windows"].as_array().into_iter().flatten() {
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
            let bar = gtk::LevelBar::builder()
                .min_value(0.0)
                .max_value(100.0)
                .value(w["percent"].as_f64().unwrap_or(0.0))
                .width_request(120)
                .valign(gtk::Align::Center)
                .build();
            row.add_suffix(&bar);
            row.add_suffix(&gtk::Label::new(Some(&format!(
                "{:.0}%",
                w["percent"].as_f64().unwrap_or(0.0)
            ))));
            usage.add(&row);
        }
    }
    page.add(&usage);
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
