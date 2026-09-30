//! Agent setup and skill management alongside the connector registry.
use crate::{
    client,
    manage::dialog,
    rows::{icon_button, label},
    ui::{self, App, toast},
};
use adw::prelude::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use vte::prelude::*;

pub fn open(ui: &App) {
    let (_, body) = dialog(ui, "Marketplace");
    let connectors = gtk::Button::with_label("Connectors");
    body.append(&connectors);
    let ui2 = ui.clone();
    connectors.connect_clicked(move |_| crate::connections::marketplace(&ui2));
    let credentials = gtk::Button::with_label("Credentials");
    body.append(&credentials);
    let ui2 = ui.clone();
    credentials.connect_clicked(move |_| crate::connections::credentials(&ui2));
    body.append(&label("Agents", &["headline"]));
    let agents = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.append(&agents);
    let ui2 = ui.clone();
    client::call("refreshBackends", json!({}), move |r| match r {
        Ok(v) => {
            ui2.state.borrow_mut().hello["backends"] = v["backends"].clone();
            for backend in v["backends"].as_array().into_iter().flatten().filter(|b| {
                b["available"] == true || b["installed"] == true || b["curated"] == true
            }) {
                let button = gtk::Button::with_label(backend["name"].as_str().unwrap_or("Agent"));
                agents.append(&button);
                let (ui, backend) = (ui2.clone(), backend.clone());
                button.connect_clicked(move |_| agent(&ui, &backend));
            }
        }
        Err(e) => agents.append(&label(&e, &["danger-text"])),
    });
    body.append(&label("Skills", &["headline"]));
    let search = gtk::SearchEntry::new();
    body.append(&search);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.append(&list);
    let refresh = icon_button("view-refresh-symbolic", "Refresh skills");
    body.append(&refresh);
    let (ui2, list2, search2) = (ui.clone(), list.clone(), search.clone());
    refresh.connect_clicked(move |_| skills(&ui2, &list2, &search2.text()));
    let (ui2, list2) = (ui.clone(), list.clone());
    search.connect_activate(move |s| skills(&ui2, &list2, &s.text()));
    skills(ui, &list, "");
}

fn skills(ui: &App, list: &gtk::Box, query: &str) {
    ui::clear(list);
    list.append(&label("Loading…", &["secondary"]));
    let (ui, list, query) = (ui.clone(), list.clone(), query.to_lowercase());
    client::call("skills", json!({}), move |r| {
        ui::clear(&list);
        let installed = match r {
            Ok(v) => v["items"].as_array().cloned().unwrap_or_default(),
            Err(e) => {
                list.append(&label(&e, &["danger-text"]));
                return;
            }
        };
        for item in &installed {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let text = label(item["name"].as_str().unwrap_or("Skill"), &[]);
            text.set_hexpand(true);
            row.append(&text);
            let remove = icon_button("user-trash-symbolic", "Remove skill");
            row.append(&remove);
            list.append(&row);
            let (ui, list, id, query) =
                (ui.clone(), list.clone(), item["id"].clone(), query.clone());
            remove.connect_clicked(move |_| {
                let (ui2, list, id, query) = (ui.clone(), list.clone(), id.clone(), query.clone());
                crate::dialogs::confirm(
                    &ui,
                    "Remove skill?",
                    "Bots will no longer have this skill.",
                    "Remove",
                    true,
                    move || {
                        client::call("removeSkill", json!({"id":id}), move |r| {
                            if let Err(e) = r {
                                toast(&ui2, &e);
                            }
                            skills(&ui2, &list, &query);
                        });
                    },
                );
            });
        }
        client::call("marketSkills", json!({}), move |r| match r {
            Ok(v) => {
                let items: Vec<_> = v["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|i| {
                        format!("{} {}", i["name"], i["description"])
                            .to_lowercase()
                            .contains(&query)
                    })
                    .collect();
                if items.is_empty() {
                    list.append(&label("No matching skills", &["secondary"]));
                }
                for item in items {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let text = label(
                        &format!(
                            "{}\n{}",
                            item["name"].as_str().unwrap_or("Skill"),
                            item["description"].as_str().unwrap_or("")
                        ),
                        &[],
                    );
                    text.set_wrap(true);
                    text.set_hexpand(true);
                    row.append(&text);
                    let added = item["installed"] == true
                        || installed
                            .iter()
                            .any(|s| s["source"] == item["source"] && s["source"].is_string());
                    let add = gtk::Button::with_label(if added { "Added" } else { "Add" });
                    add.set_sensitive(!added);
                    row.append(&add);
                    list.append(&row);
                    let (ui, list, source, query) = (
                        ui.clone(),
                        list.clone(),
                        item["source"].clone(),
                        query.clone(),
                    );
                    add.connect_clicked(move |button| {
                        button.set_sensitive(false);
                        let (ui, list, query) = (ui.clone(), list.clone(), query.clone());
                        client::call("installSkill", json!({"source":source}), move |r| {
                            if let Err(e) = r {
                                toast(&ui, &e);
                            }
                            skills(&ui, &list, &query);
                        });
                    });
                }
            }
            Err(e) => list.append(&label(&e, &["danger-text"])),
        });
    });
}

pub fn agent(ui: &App, backend: &Value) {
    let (_, body) = dialog(ui, backend["name"].as_str().unwrap_or("Agent"));
    let id = backend["id"].as_str().unwrap_or("").to_owned();
    if backend["installed"] != true && backend["canInstall"] == true {
        let install = gtk::Button::with_label("Install");
        body.append(&install);
        let (ui, id) = (ui.clone(), id.clone());
        install.connect_clicked(move |_| setup(&ui, &id, "install", None));
    }
    let check = gtk::Button::with_label("Check sign-in");
    body.append(&check);
    let auth = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.append(&auth);
    let (ui2, id2, auth2) = (ui.clone(), id.clone(), auth.clone());
    check.connect_clicked(move |_| load_auth(&ui2, &id2, &auth2));
    if backend["installed"] == true || backend["available"] == true {
        load_auth(ui, &id, &auth);
    }
}

fn load_auth(ui: &App, id: &str, body: &gtk::Box) {
    ui::clear(body);
    body.append(&label("Checking sign-in…", &["secondary"]));
    let (ui, id, body) = (ui.clone(), id.to_owned(), body.clone());
    client::call("agentAuth", json!({"backend":id}), move |r| {
        ui::clear(&body);
        let v = match r {
            Ok(v) => v,
            Err(e) => {
                body.append(&label(&e, &["danger-text"]));
                return;
            }
        };
        let status = label(
            if v["signedIn"] == true {
                "Signed in"
            } else {
                "Sign-in required"
            },
            &["headline"],
        );
        body.append(&status);
        if let Some(detail) = v["detail"].as_str() {
            let text = label(detail, &[]);
            text.set_wrap(true);
            body.append(&text);
        }
        if v["login"] == true {
            let login = gtk::Button::with_label("Sign in in terminal");
            body.append(&login);
            let (ui, id) = (ui.clone(), id.clone());
            login.connect_clicked(move |_| setup(&ui, &id, "login", None));
        }
        for method in v["methods"].as_array().into_iter().flatten() {
            let button = gtk::Button::with_label(method["name"].as_str().unwrap_or("Sign in"));
            body.append(&button);
            let (ui, id, method, body) = (ui.clone(), id.clone(), method.clone(), body.clone());
            button.connect_clicked(move |button| match method["kind"].as_str() {
                Some("terminal") => setup(&ui, &id, "login", method["id"].as_str()),
                Some("envVar") => env_fields(&ui, &id, &method, &body),
                _ => {
                    button.set_sensitive(false);
                    let (ui, id, body) = (ui.clone(), id.clone(), body.clone());
                    client::call(
                        "agentAuthenticate",
                        json!({"backend":id,"method":method["id"]}),
                        move |r| {
                            if let Err(e) = r {
                                toast(&ui, &e);
                            }
                            load_auth(&ui, &id, &body);
                        },
                    );
                }
            });
        }
    });
}

fn env_fields(ui: &App, id: &str, method: &Value, auth: &gtk::Box) {
    let (window, body) = dialog(ui, "Sign in with credentials");
    let mut fields = vec![];
    for var in method["vars"].as_array().into_iter().flatten() {
        body.append(&label(
            var["label"].as_str().unwrap_or("Credential"),
            &["secondary"],
        ));
        let entry = gtk::Entry::builder()
            .visibility(var["secret"] == false)
            .build();
        body.append(&entry);
        fields.push((
            var["name"].as_str().unwrap_or("").to_owned(),
            var["optional"] != true,
            entry,
        ));
    }
    let error = label("", &["danger-text"]);
    body.append(&error);
    let save = gtk::Button::with_label("Save and sign in");
    body.append(&save);
    let (ui, id, auth) = (ui.clone(), id.to_owned(), auth.clone());
    save.connect_clicked(move |button| {
        if fields
            .iter()
            .any(|(_, required, e)| *required && e.text().is_empty())
        {
            error.set_label("Fill in the required credentials.");
            return;
        }
        let vars: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|(_, _, e)| !e.text().is_empty())
            .map(|(k, _, e)| (k.clone(), e.text().to_string().into()))
            .collect();
        button.set_sensitive(false);
        let (ui, id, auth, window, fields, error, button) = (
            ui.clone(),
            id.clone(),
            auth.clone(),
            window.clone(),
            fields.clone(),
            error.clone(),
            button.clone(),
        );
        client::call("setAgentEnv", json!({"backend":id,"vars":vars}), move |r| {
            button.set_sensitive(true);
            match r {
                Ok(_) => {
                    for (_, _, e) in fields {
                        e.set_text("");
                    }
                    window.close();
                    load_auth(&ui, &id, &auth);
                }
                Err(e) => error.set_label(&e),
            }
        });
    });
}

fn setup(ui: &App, backend: &str, step: &str, method: Option<&str>) {
    let (window, body) = dialog(
        ui,
        if step == "install" {
            "Install agent"
        } else {
            "Sign in"
        },
    );
    window.set_content_width(900);
    window.set_content_height(650);
    let terminal = vte::Terminal::new();
    terminal.set_vexpand(true);
    terminal.set_hexpand(true);
    terminal.set_size(90, 28);
    terminal.set_input_enabled(false);
    terminal.set_allow_hyperlink(true);
    // Only a user gesture opens URLs printed by the agent.
    const PCRE2_MULTILINE: u32 = 0x0000_0400;
    if let Ok(regex) = vte::Regex::for_match(r#"https?://[^\s<>\"']+"#, PCRE2_MULTILINE) {
        terminal.match_add_regex(&regex, 0);
    }
    let click = gtk::GestureClick::new();
    let terminal_link = terminal.downgrade();
    click.connect_pressed(move |gesture, _, x, y| {
        if !gesture
            .current_event_state()
            .contains(gtk::gdk::ModifierType::CONTROL_MASK)
        {
            return;
        }
        if let Some(terminal) = terminal_link.upgrade()
            && let Some(url) = terminal
                .check_hyperlink_at(x, y)
                .or_else(|| terminal.check_match_at(x, y).0)
            && (url.starts_with("https://") || url.starts_with("http://"))
        {
            let _ = gtk::gio::AppInfo::launch_default_for_uri(
                &url,
                None::<&gtk::gio::AppLaunchContext>,
            );
        }
    });
    terminal.add_controller(click);
    body.append(&terminal);
    let status = label("Starting…", &["secondary"]);
    body.append(&status);
    let state = Rc::new(RefCell::new(
        None::<(
            String,
            async_channel::Sender<Vec<u8>>,
            tokio::task::AbortHandle,
        )>,
    ));
    let closed = Rc::new(std::cell::Cell::new(false));
    let state2 = state.clone();
    let closed2 = closed.clone();
    window.connect_closed(move |_| {
        closed2.set(true);
        if let Some((id, keys, output)) = state2.borrow_mut().take() {
            keys.close();
            output.abort();
            client::call("termClose", json!({"term":id}), |_| {});
        }
    });
    let state2 = state.clone();
    terminal.connect_commit(move |_, text, _| {
        if let Some((_, keys, _)) = &*state2.borrow() {
            let _ = keys.try_send(text.as_bytes().to_vec());
        }
    });
    let state2 = state.clone();
    let last_size = RefCell::new((0, 0));
    terminal.add_tick_callback(move |t, _| {
        if let Some((id, _, _)) = &*state2.borrow() {
            let size = (t.column_count(), t.row_count());
            if *last_size.borrow() != size {
                *last_size.borrow_mut() = size;
                client::call(
                    "termResize",
                    json!({"term":id,"cols":size.0,"rows":size.1}),
                    |_| {},
                );
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    let args = json!({"backend":backend,"step":step,"method":method,"cols":90,"rows":28});
    client::call("agentSetup", args, move |r| {
        let id = match r {
            Ok(v) => v["term"].as_str().unwrap_or("").to_owned(),
            Err(e) => {
                status.set_label(&e);
                return;
            }
        };
        if closed.get() {
            client::call("termClose", json!({"term":id}), |_| {});
            return;
        }
        let (tx, rx) = async_channel::unbounded();
        let (keys, output) = client::terminal(id.clone(), tx);
        *state.borrow_mut() = Some((id, keys, output));
        terminal.set_input_enabled(true);
        terminal.grab_focus();
        status.set_label("Running on your computer · Ctrl+click opens a link");
        gtk::glib::spawn_future_local(async move {
            while let Ok(event) = rx.recv().await {
                match event {
                    Ok(v) if v["type"] == "output" => {
                        terminal.feed(&gtk::glib::base64_decode(v["data"].as_str().unwrap_or("")))
                    }
                    Ok(v) if v["type"] == "exit" => {
                        terminal.set_input_enabled(false);
                        status.set_label(if v["code"] == 0 {
                            "Finished"
                        } else {
                            "Ended with an error"
                        });
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        terminal.set_input_enabled(false);
                        status.set_label(&e);
                        break;
                    }
                }
            }
        });
    });
}
