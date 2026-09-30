//! Native widget integration tests, run under Xvfb with tests/ui_fixture.py.
use super::*;
use std::time::{Duration, Instant};

#[track_caller]
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        if ready() {
            return;
        }
        if Instant::now() >= deadline {
            if let Ok(dir) = std::env::var("CODYNC_UI_ARTIFACTS") {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("import")
                    .args(["-window", "root", &format!("{dir}/failure.png")])
                    .status();
            }
            panic!("native UI did not reach the expected state");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn widgets(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let root = root.as_ref();
    let mut result = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(widget) = child {
        result.extend(widgets(&widget));
        child = widget.next_sibling();
    }
    result
}
fn button(root: &impl IsA<gtk::Widget>, name: &str) -> Option<gtk::Button> {
    widgets(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(name) || b.tooltip_text().as_deref() == Some(name))
}
fn text(root: &impl IsA<gtk::Widget>, value: &str) -> bool {
    widgets(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .any(|l| l.text().contains(value))
}
#[track_caller]
fn click(root: &impl IsA<gtk::Widget>, name: &str) {
    wait(|| button(root, name).is_some_and(|b| b.is_sensitive()));
    let button = button(root, name).unwrap();
    wait(|| button.is_mapped());
    // libadwaita 1.5 opens a newly mapped dialog on its second frame.
    // Emitting clicks before that can save and close a dialog before it opens.
    let frames = Rc::new(std::cell::Cell::new(0));
    let frames2 = frames.clone();
    button.add_tick_callback(move |_, _| {
        frames2.set(frames2.get() + 1);
        if frames2.get() >= 3 {
            gtk::glib::ControlFlow::Break
        } else {
            gtk::glib::ControlFlow::Continue
        }
    });
    wait(|| frames.get() >= 3);
    button.emit_clicked();
}
fn close(ui: &App) {
    if let Some(dialog) = ui.window.visible_dialog() {
        dialog.force_close();
        wait(|| ui.window.visible_dialog().as_ref() != Some(&dialog));
    }
}
fn screenshot(name: &str) {
    if let Ok(dir) = std::env::var("CODYNC_UI_ARTIFACTS") {
        let deadline = Instant::now() + Duration::from_millis(400);
        wait(|| Instant::now() > deadline);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            std::process::Command::new("import")
                .args(["-window", "root", &format!("{dir}/{name}.png")])
                .status()
                .unwrap()
                .success()
        );
    }
}
fn requests() -> Value {
    let result = Rc::new(RefCell::new(None));
    let result2 = result.clone();
    client::call("_testState", json!({}), move |r| {
        *result2.borrow_mut() = Some(r.unwrap());
    });
    wait(|| result.borrow().is_some());
    result.borrow_mut().take().unwrap()
}

#[test]
#[ignore = "run under Xvfb using tests/ui_fixture.py"]
fn native_ui_flows() {
    assert_eq!(std::env::var("CODYNC_UI_TEST").as_deref(), Ok("1"));
    adw::init().unwrap();
    // Exercise actions after their dialog is ready, without racing presentation animations.
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let app = adw::Application::builder()
        .application_id("com.pokai.Codync.ParityTest")
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let display = gtk::gdk::Display::default().unwrap();
    let css = gtk::CssProvider::new();
    css.load_from_string(&format!("{}\n{}", crate::colors(&crate::DARK), crate::CSS));
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    let ui = build(&app);
    wait(|| {
        ui.state
            .borrow()
            .entries
            .get("bot")
            .is_some_and(|e| e.len() == 2)
    });
    select(&ui, "bot");
    wait(|| text(&ui.chat_list, "Streaming fixture answer"));
    click(&ui.chat_list, "Load earlier messages");
    wait(|| text(&ui.chat_list, "Older fixture message"));
    assert!(text(&ui.chat_list, "Streaming fixture answer"));
    screenshot("linux-chat-streaming");

    send_text(&ui, "bot", "Retry this message", None, vec![]);
    wait(|| text(&ui.chat_list, "Failed to send"));
    click(&ui.chat_list, "Resend");
    wait(|| ui.state.borrow().outbox.is_empty());
    assert_eq!(
        ui.state.borrow().entries["bot"]
            .iter()
            .filter(|e| e["data"]["text"] == "Retry this message")
            .count(),
        1
    );
    let sent: Vec<_> = requests()["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r[0] == "send")
        .cloned()
        .collect();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0][1]["clientNonce"], sent[1][1]["clientNonce"]);

    click(&ui.chat_list, "Remove reaction");
    wait(|| {
        requests()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0] == "react")
    });
    click(&ui.chat_list, "Save attachment");
    wait(|| {
        requests()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0] == "readUpload")
    });

    let downloads = std::path::PathBuf::from(std::env::var("CODYNC_UI_DOWNLOADS").unwrap());
    wait(|| std::fs::read(downloads.join("parity.txt")).ok().as_deref() == Some(b"fixture"));
    click(&ui.chat_list, "Save attachment");
    wait(|| {
        std::fs::read(downloads.join("1-parity.txt"))
            .ok()
            .as_deref()
            == Some(b"fixture")
    });
    assert_eq!(
        std::fs::read(downloads.join("parity.txt")).unwrap(),
        b"fixture"
    );
    send_text(&ui, "bot", "Discard this message", None, vec![]);
    wait(|| text(&ui.chat_list, "Failed to send"));
    click(&ui.chat_list, "Delete");
    wait(|| !text(&ui.chat_list, "Discard this message"));

    crate::manage::memory(&ui, "bot");
    let memory = ui.window.visible_dialog().unwrap();
    wait(|| text(&memory, "Prefers native interfaces"));
    screenshot("linux-memory");
    click(&memory, "Forget");
    wait(|| text(&memory, "Nothing yet"));
    close(&ui);

    crate::manage::routines(&ui, "bot");
    let routines = ui.window.visible_dialog().unwrap();
    wait(|| text(&routines, "No routines yet"));
    click(&routines, "Set up a routine");
    let editor = ui.window.visible_dialog().unwrap();
    let entries: Vec<_> = widgets(&editor)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Entry>().ok())
        .collect();
    entries[0].set_text("Morning review");
    let instruction = widgets(&editor)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::TextView>().ok())
        .unwrap();
    instruction.buffer().set_text("Review the latest changes");
    click(&editor, "Preview schedule");
    wait(|| text(&editor, "Daily at 09:00 UTC"));
    screenshot("linux-routine-editor");
    click(&editor, "Save");
    wait(|| text(&routines, "Morning review"));
    click(&routines, "Pause");
    wait(|| text(&routines, "Paused"));
    screenshot("linux-routines");
    close(&ui);

    crate::market::open(&ui);
    let market = ui.window.visible_dialog().unwrap();
    wait(|| button(&market, "Add").is_some());
    screenshot("linux-marketplace");
    click(&market, "Add");
    wait(|| button(&market, "Remove skill").is_some());
    click(&market, "Fixture Agent");
    let agent = ui.window.visible_dialog().unwrap();
    click(&agent, "API key");
    let credentials = ui.window.visible_dialog().unwrap();
    let entry = widgets(&credentials)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap();
    assert!(!gtk::prelude::EntryExt::is_visible(&entry)); // GtkEditable visibility masks the credential.
    entry.set_text("fixture-key");
    click(&credentials, "Save and sign in");
    wait(|| ui.window.visible_dialog().as_ref() == Some(&agent));
    click(&agent, "Sign in in terminal");
    let terminal = ui.window.visible_dialog().unwrap();
    wait(|| text(&terminal, "Running on your computer"));
    screenshot("linux-agent-terminal");
    close(&ui);
    close(&ui);
    close(&ui);
    let log = requests();
    for method in [
        "history",
        "react",
        "readUpload",
        "forgetMemory",
        "saveRoutine",
        "setRoutineEnabled",
        "installSkill",
        "setAgentEnv",
        "agentSetup",
        "termClose",
    ] {
        assert!(
            log["requests"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == method),
            "missing {method}"
        );
    }
    ui.window.close();
}
