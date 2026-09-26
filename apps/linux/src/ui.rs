//! Main window: roster sidebar + conversation (Grok Bot's desktop layout).

use crate::client::{self, Event};
use crate::rows::{
    agent_bubble, author_label, notice, permission_card, thread_chip, user_bubble, with_reply,
};
use crate::{avatar, dialogs};
use adw::prelude::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    pub bots: HashMap<String, Value>,
    pub entries: HashMap<String, Vec<Value>>,
    pub usage: Value,
    pub hello: Value,
    pub current: Option<String>,
    /// Root entry of the thread open beside the current chat.
    pub open_thread: Option<String>,
    pub online: bool,
}

/// A message box: send, or stop while the chat (or thread) is working. In a group,
/// typing `@` suggests its bots.
struct Composer {
    view: gtk::TextView,
    send_btn: gtk::Button,
    stop_btn: gtk::Button,
    mentions: gtk::Box,
    mentions_rev: gtk::Revealer,
    root: gtk::Box,
}

fn composer() -> Composer {
    let view = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .accepts_tab(false)
        .hexpand(true)
        .top_margin(4)
        .bottom_margin(4)
        .build();
    let frame = gtk::ScrolledWindow::builder()
        .child(&view)
        .max_content_height(160)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .css_classes(["composer"])
        .hexpand(true)
        .build();
    let send_btn = gtk::Button::builder()
        .icon_name("go-up-symbolic")
        .tooltip_text("Send")
        .css_classes(["round", "accent-fill"])
        .valign(gtk::Align::End)
        .build();
    let stop_btn = gtk::Button::builder()
        .icon_name("media-playback-stop-symbolic")
        .tooltip_text("Stop")
        .css_classes(["round"])
        .valign(gtk::Align::End)
        .build();
    let row = gtk::Box::builder()
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(12)
        .build();
    row.append(&frame);
    row.append(&stop_btn);
    row.append(&send_btn);
    let mentions = gtk::Box::builder()
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    let mentions_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .child(
            &gtk::ScrolledWindow::builder()
                .vscrollbar_policy(gtk::PolicyType::Never)
                .child(&mentions)
                .build(),
        )
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&mentions_rev);
    root.append(&row);
    Composer {
        view,
        send_btn,
        stop_btn,
        mentions,
        mentions_rev,
        root,
    }
}

pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    pub split: adw::NavigationSplitView,
    roster: gtk::ListBox,
    usage_box: gtk::Box,
    banner: adw::Banner,
    title: adw::WindowTitle,
    head_avatar: gtk::Box,
    chat_list: gtk::Box,
    scroller: gtk::ScrolledWindow,
    compose: Composer,
    pane: gtk::Revealer,
    pane_title: adw::WindowTitle,
    pane_list: gtk::Box,
    pane_scroller: gtk::ScrolledWindow,
    reply: Composer,
    content_stack: gtk::Stack,
    chat_buttons: [gtk::Widget; 2],
    /// Bot shown in the last render; switching bots jumps to the newest message.
    shown_bot: RefCell<String>,
    /// Thread shown in the last render; opening one jumps to its newest reply.
    shown_thread: RefCell<String>,
    pub state: RefCell<State>,
    rendering: std::cell::Cell<bool>,
}

pub type App = Rc<Ui>;

impl Ui {
    fn composer(&self, in_thread: bool) -> &Composer {
        if in_thread {
            &self.reply
        } else {
            &self.compose
        }
    }
}

pub fn build(app: &adw::Application) {
    // Sidebar
    let roster = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .build();
    let usage_box = gtk::Box::builder()
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(6)
        .build();
    let usage_scroll = gtk::ScrolledWindow::builder()
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&usage_box)
        .build();
    let banner = adw::Banner::new("Reconnecting to the Codync host…");
    let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_box.append(&banner);
    sidebar_box.append(&usage_scroll);
    sidebar_box.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&roster)
            .build(),
    );
    let new_btn = gtk::Button::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text("New bot")
        .build();
    let group_btn = gtk::Button::builder()
        .icon_name("system-users-symbolic")
        .tooltip_text("New group chat")
        .build();
    let settings_btn = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings & pairing")
        .build();
    let side_header = adw::HeaderBar::new();
    side_header.pack_start(&settings_btn);
    side_header.pack_end(&new_btn);
    side_header.pack_end(&group_btn);
    let side_view = adw::ToolbarView::new();
    side_view.add_top_bar(&side_header);
    side_view.set_content(Some(&sidebar_box));
    let sidebar_page = adw::NavigationPage::new(&side_view, "Codync");

    // Conversation
    let title = adw::WindowTitle::new("", "");
    let head_avatar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let head = gtk::Box::builder().spacing(8).build();
    head.append(&head_avatar);
    head.append(&title);
    let trace_btn = gtk::Button::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text("Full conversation")
        .build();
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("More")
        .build();
    let content_header = adw::HeaderBar::builder().title_widget(&head).build();
    content_header.pack_end(&menu_btn);
    content_header.pack_end(&trace_btn);
    let chat_list = entry_list();
    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .child(&chat_list)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&clamp)
        .build();
    let compose = composer();
    let convo = gtk::Box::new(gtk::Orientation::Vertical, 0);
    convo.append(&scroller);
    convo.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    convo.append(&compose.root);
    convo.set_hexpand(true);

    // Thread pane beside the chat (Slack's).
    let pane_title = adw::WindowTitle::new("Thread", "");
    let close_thread = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close thread")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build();
    let pane_head = gtk::Box::builder()
        .spacing(8)
        .margin_start(12)
        .margin_end(8)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    pane_title.set_hexpand(true);
    pane_head.append(&pane_title);
    pane_head.append(&close_thread);
    let pane_list = entry_list();
    let pane_scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&pane_list)
        .build();
    let reply = composer();
    let pane_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(380)
        .build();
    pane_box.append(&pane_head);
    pane_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    pane_box.append(&pane_scroller);
    pane_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    pane_box.append(&reply.root);
    let pane_outer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pane_outer.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    pane_outer.append(&pane_box);
    let pane = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideLeft)
        .child(&pane_outer)
        .build();
    let chat_area = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chat_area.append(&convo);
    chat_area.append(&pane);
    let empty = adw::StatusPage::builder()
        .title("Your coding agents, as teammates")
        .description("Pick a bot, or create one for each kind of work and point it at a project.")
        .icon_name("user-available-symbolic")
        .build();
    let content_stack = gtk::Stack::new();
    content_stack.add_named(&empty, Some("empty"));
    content_stack.add_named(&chat_area, Some("chat"));
    let content_view = adw::ToolbarView::new();
    content_view.add_top_bar(&content_header);
    content_view.set_content(Some(&content_stack));
    let content_page = adw::NavigationPage::new(&content_view, "Chat");

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(260.0)
        .build();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&split));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Codync")
        .default_width(1100)
        .default_height(760)
        .content(&toasts)
        .build();
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        640.0,
        adw::LengthUnit::Sp,
    ));
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);

    let ui: App = Rc::new(Ui {
        window,
        toasts,
        split,
        roster,
        usage_box,
        banner,
        title,
        head_avatar,
        chat_list,
        scroller,
        compose,
        pane,
        pane_title,
        pane_list,
        pane_scroller,
        reply,
        content_stack,
        chat_buttons: [trace_btn.clone().upcast(), menu_btn.clone().upcast()],
        shown_bot: RefCell::new(String::new()),
        shown_thread: RefCell::new(String::new()),
        state: RefCell::new(State::default()),
        rendering: std::cell::Cell::new(false),
    });

    // Wiring
    {
        let ui2 = ui.clone();
        ui.roster.connect_row_activated(move |_, row| {
            {
                let mut st = ui2.state.borrow_mut();
                st.current = Some(row.widget_name().to_string());
                st.open_thread = None;
            }
            ui2.split.set_show_content(true);
            mark_read(&ui2);
            render(&ui2);
        });
    }
    {
        let ui2 = ui.clone();
        new_btn.connect_clicked(move |_| dialogs::editor(&ui2, None));
    }
    {
        let ui2 = ui.clone();
        group_btn.connect_clicked(move |_| dialogs::group_editor(&ui2, None));
    }
    {
        let ui2 = ui.clone();
        close_thread.connect_clicked(move |_| {
            ui2.state.borrow_mut().open_thread = None;
            schedule(&ui2);
        });
    }
    {
        let ui2 = ui.clone();
        settings_btn.connect_clicked(move |_| dialogs::settings(&ui2));
    }
    {
        let ui2 = ui.clone();
        trace_btn.connect_clicked(move |_| dialogs::trace(&ui2));
    }
    {
        let ui2 = ui.clone();
        menu_btn.set_create_popup_func(move |btn| btn.set_popover(Some(&dialogs::bot_menu(&ui2))));
    }
    wire_composer(&ui, false);
    wire_composer(&ui, true);
    // Keep relative times fresh.
    {
        let ui2 = ui.clone();
        gtk::glib::timeout_add_seconds_local(30, move || {
            render_roster(&ui2);
            gtk::glib::ControlFlow::Continue
        });
    }

    ui.window.present();
    connect(&ui);
}

fn entry_list() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .margin_bottom(12)
        .build()
}

fn wire_composer(ui: &App, in_thread: bool) {
    let c = ui.composer(in_thread);
    {
        let ui2 = ui.clone();
        c.send_btn.connect_clicked(move |_| send(&ui2, in_thread));
    }
    {
        let ui2 = ui.clone();
        c.stop_btn.connect_clicked(move |_| {
            if let Some(id) = ui2.state.borrow().current.clone() {
                client::call("stop", json!({"botId": id}), |_| {});
            }
        });
    }
    {
        let ui2 = ui.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, mods| {
            if (key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter)
                && !mods.contains(gtk::gdk::ModifierType::SHIFT_MASK)
            {
                send(&ui2, in_thread);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        c.view.add_controller(keys);
    }
    {
        let ui2 = ui.clone();
        c.view
            .buffer()
            .connect_changed(move |_| update_composer(&ui2, in_thread));
    }
}

fn connect(ui: &App) {
    if client::token().is_none() {
        dialogs::host_missing(ui);
        return;
    }
    let ui2 = ui.clone();
    client::call("hello", client::empty(), move |r| match r {
        Ok(hello) => {
            let mut st = ui2.state.borrow_mut();
            st.hello = hello;
            // Open a specific bot on launch: `codync --bot <id>` (also used by desktop notifications).
            let args: Vec<String> = std::env::args().collect();
            if let Some(i) = args.iter().position(|a| a == "--bot") {
                st.current = args.get(i + 1).cloned();
            }
            drop(st);
            let (tx, rx) = async_channel::unbounded();
            client::stream(tx);
            let ui3 = ui2.clone();
            gtk::glib::spawn_future_local(async move {
                while let Ok(ev) = rx.recv().await {
                    handle(&ui3, ev);
                }
            });
        }
        Err(_) => dialogs::host_missing(&ui2),
    });
}

pub fn reconnect(ui: &App) {
    client::reset_rev();
    connect(ui);
}

fn handle(ui: &App, ev: Event) {
    {
        let mut st = ui.state.borrow_mut();
        match ev {
            Event::Online(on) => st.online = on,
            Event::Message(m) => match m["type"].as_str().unwrap_or_default() {
                "hello" => st.usage = m["usage"].clone(),
                "usage" => st.usage = m["usage"].clone(),
                "bot" => {
                    let bot = &m["bot"];
                    let id = bot["id"].as_str().unwrap_or_default().to_owned();
                    if bot["deleted"].as_bool().unwrap_or(false) {
                        st.bots.remove(&id);
                        st.entries.remove(&id);
                        if st.current.as_deref() == Some(&id) {
                            st.current = None;
                            st.open_thread = None;
                        }
                    } else {
                        if let Some(old) = st.bots.get(&id) {
                            notify(ui, old, bot);
                        }
                        st.bots.insert(id, bot.clone());
                    }
                }
                "entry" => upsert(&mut st, m["entry"].clone()),
                _ => {}
            },
        }
    }
    schedule(ui);
}

/// Desktop notification when a bot needs you or finishes, unless Codync is focused.
fn notify(ui: &App, old: &Value, new: &Value) {
    if ui.window.is_active() || new["notify"] == false || new["hidden"] == true {
        return;
    }
    // A bot's turn in a group is announced by the group.
    if [old, new].iter().any(|b| {
        b["workingChat"]
            .as_str()
            .is_some_and(|c| Some(c) != b["id"].as_str())
    }) {
        return;
    }
    let name = new["name"].as_str().unwrap_or("Bot");
    let (title, body) = match (old["status"].as_str(), new["status"].as_str()) {
        (Some(o), Some("needsInput")) if o != "needsInput" => (
            format!("{name} needs you"),
            new["activity"].as_str().unwrap_or("").to_owned(),
        ),
        (Some("working" | "needsInput"), Some("idle")) => (
            name.to_owned(),
            new["lastMessage"]
                .as_str()
                .unwrap_or("Finished.")
                .to_owned(),
        ),
        _ => return,
    };
    let n = gtk::gio::Notification::new(&title);
    n.set_body(Some(&body.chars().take(140).collect::<String>()));
    if let Some(app) = ui.window.application() {
        app.send_notification(new["id"].as_str(), &n);
    }
}

pub fn upsert(st: &mut State, e: Value) {
    let bot = e["botId"].as_str().unwrap_or_default().to_owned();
    let list = st.entries.entry(bot).or_default();
    let id = e["id"].as_str().unwrap_or_default();
    if let Some(i) = list.iter().position(|x| x["id"] == id) {
        // A late command response mustn't undo a newer streamed update.
        if e["rev"].as_i64() >= list[i]["rev"].as_i64() {
            list[i] = e;
        }
        return;
    }
    if e["kind"] == "user"
        && let Some(nonce) = e["data"]["clientNonce"].as_str().filter(|n| !n.is_empty())
    {
        list.retain(|x| x["id"] != format!("local-{nonce}"));
    }
    list.push(e);
    list.sort_by_key(|x| x["seq"].as_i64().unwrap_or(i64::MAX));
    if list.len() > 400 {
        list.drain(..list.len() - 400);
    }
}

/// Coalesces bursts of events into one render per frame.
pub fn schedule(ui: &App) {
    if ui.rendering.replace(true) {
        return;
    }
    let ui2 = ui.clone();
    gtk::glib::idle_add_local_once(move || {
        ui2.rendering.set(false);
        render(&ui2);
    });
}

pub fn render(ui: &App) {
    ui.banner.set_revealed(!ui.state.borrow().online);
    render_usage(ui);
    render_roster(ui);
    render_chat(ui);
    render_pane(ui);
}

pub fn working(b: &Value) -> bool {
    matches!(b["status"].as_str(), Some("working" | "needsInput"))
}

/// Working in this lane: the chat `chat` (its main chat, or the thread on `thread`).
pub fn working_in(b: &Value, chat: &str, thread: Option<&str>) -> bool {
    working(b)
        && b["workingChat"].as_str().or(b["id"].as_str()) == Some(chat)
        && b["workingThread"].as_str() == thread
}

fn is_group(b: &Value) -> bool {
    b["kind"] == "group"
}

fn members<'a>(st: &'a State, group: &Value) -> Vec<&'a Value> {
    group["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| st.bots.get(m.as_str()?))
        .collect()
}

fn member_names(st: &State, group: &Value) -> String {
    members(st, group)
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The `@partial` being typed at the end of a group message, if any.
fn mention_query(draft: &str) -> Option<&str> {
    let at = draft.rfind('@')?;
    if draft[..at]
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace())
    {
        return None;
    }
    let q = &draft[at + 1..];
    (!q.contains('\n') && q.chars().count() <= 24).then_some(q)
}

pub fn backend_name(st: &State, id: &str) -> String {
    st.hello["backends"]
        .as_array()
        .and_then(|l| l.iter().find(|b| b["id"] == id))
        .and_then(|b| b["name"].as_str())
        .unwrap_or(if id == "custom" { "Custom agent" } else { id })
        .to_owned()
}

pub fn folder(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_owned())
}

pub fn ago(ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let s = ((now - ms) / 1000).max(0);
    match s {
        0..60 => "now".into(),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

fn render_usage(ui: &App) {
    while let Some(c) = ui.usage_box.first_child() {
        ui.usage_box.remove(&c);
    }
    let st = ui.state.borrow();
    for p in st.usage["providers"].as_array().into_iter().flatten() {
        for w in p["windows"].as_array().into_iter().flatten().take(2) {
            let pct = w["percent"].as_f64().unwrap_or(0.0);
            let chip = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["chip"])
                .build();
            chip.append(
                &gtk::Label::builder()
                    .label(format!(
                        "{} {}",
                        p["name"].as_str().unwrap_or(""),
                        w["label"].as_str().unwrap_or("")
                    ))
                    .css_classes(["small", "muted"])
                    .xalign(0.0)
                    .build(),
            );
            let value = gtk::Label::builder()
                .label(format!("{pct:.0}%"))
                .xalign(0.0)
                .css_classes(["heading"])
                .build();
            if pct >= 90.0 {
                value.add_css_class("error");
            } else if pct >= 70.0 {
                value.add_css_class("warning");
            }
            chip.append(&value);
            ui.usage_box.append(&chip);
        }
    }
}

fn render_roster(ui: &App) {
    let st = ui.state.borrow();
    let mut bots: Vec<&Value> = st
        .bots
        .values()
        .filter(|b| !b["hidden"].as_bool().unwrap_or(false))
        .collect();
    bots.sort_by_key(|b| {
        (
            !b["pinned"].as_bool().unwrap_or(false),
            -b["lastAt"].as_i64().unwrap_or(0),
        )
    });
    while let Some(c) = ui.roster.first_child() {
        ui.roster.remove(&c);
    }
    for b in bots {
        let id = b["id"].as_str().unwrap_or_default();
        let unread = b["unread"].as_i64().unwrap_or(0);
        let status = if b["status"] == "needsInput" {
            "needs"
        } else if unread > 0 {
            "unread"
        } else {
            ""
        };
        let row_box = gtk::Box::builder()
            .spacing(10)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        row_box.append(&avatar::of(&st.bots, b, 40, working(b), status));
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        let top = gtk::Box::builder().spacing(6).build();
        top.append(
            &gtk::Label::builder()
                .label(b["name"].as_str().unwrap_or(""))
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes(["heading"])
                .build(),
        );
        let time = gtk::Label::builder()
            .label(ago(b["lastAt"].as_i64().unwrap_or(0)))
            .css_classes(["small", "muted"])
            .build();
        top.append(&time);
        text.append(&top);
        let (preview, class) = match b["status"].as_str() {
            Some("needsInput") => (
                format!(
                    "✋ {}",
                    b["activity"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Needs your approval")
                ),
                "needs-text",
            ),
            Some("working") => (
                b["activity"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Working…")
                    .to_owned(),
                "accent-text",
            ),
            Some("error") => (
                b["lastMessage"]
                    .as_str()
                    .unwrap_or("Something went wrong")
                    .to_owned(),
                "error",
            ),
            _ => (
                b["lastMessage"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        if is_group(b) {
                            return member_names(&st, b);
                        }
                        format!(
                            "{} · {}",
                            backend_name(&st, b["backend"].as_str().unwrap_or("")),
                            folder(b["cwd"].as_str().unwrap_or(""))
                        )
                    }),
                "dim-label",
            ),
        };
        let bottom = gtk::Box::builder().spacing(6).build();
        bottom.append(
            &gtk::Label::builder()
                .label(preview.replace('\n', " "))
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes([class])
                .build(),
        );
        if unread > 0 {
            bottom.append(
                &gtk::Label::builder()
                    .label(unread.to_string())
                    .css_classes(["badge"])
                    .build(),
            );
        }
        text.append(&bottom);
        row_box.append(&text);
        let row = gtk::ListBoxRow::builder().child(&row_box).name(id).build();
        ui.roster.append(&row);
        if st.current.as_deref() == Some(id) {
            ui.roster.select_row(Some(&row));
        }
    }
}

fn is_chat(e: &Value) -> bool {
    match e["kind"].as_str() {
        Some("user" | "permission" | "notice") => true,
        Some("agent") => e["data"]["final"].as_bool().unwrap_or(false),
        _ => false,
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn near_bottom(s: &gtk::ScrolledWindow) -> bool {
    let adj = s.vadjustment();
    adj.upper() - adj.value() - adj.page_size() < 120.0
}

fn scroll_to_end(s: &gtk::ScrolledWindow) {
    let adj = s.vadjustment();
    // Twice: labels finish wrapping (and the height settles) one frame later.
    gtk::glib::idle_add_local_once(move || {
        adj.set_value(adj.upper());
        let adj2 = adj.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
            adj2.set_value(adj2.upper())
        });
    });
}

fn render_chat(ui: &App) {
    let st = ui.state.borrow();
    let Some(bot) = st.current.as_ref().and_then(|id| st.bots.get(id)) else {
        ui.content_stack.set_visible_child_name("empty");
        ui.chat_buttons.iter().for_each(|b| b.set_visible(false));
        ui.title.set_title("Codync");
        ui.title.set_subtitle("");
        return;
    };
    let id = bot["id"].as_str().unwrap_or_default();
    let group = is_group(bot);
    ui.content_stack.set_visible_child_name("chat");
    ui.chat_buttons.iter().for_each(|b| b.set_visible(true));
    let switched = *ui.shown_bot.replace(id.to_owned()) != *id;
    let stick = switched || near_bottom(&ui.scroller);
    ui.title.set_title(bot["name"].as_str().unwrap_or(""));
    let sub = match bot["status"].as_str() {
        Some("needsInput") => "Needs you".to_owned(),
        Some("working") => "Working".to_owned(),
        _ if group => member_names(&st, bot),
        _ => format!(
            "{} · {}",
            backend_name(&st, bot["backend"].as_str().unwrap_or("")),
            folder(bot["cwd"].as_str().unwrap_or(""))
        ),
    };
    ui.title.set_subtitle(&sub);
    clear(&ui.head_avatar);
    ui.head_avatar
        .append(&avatar::of(&st.bots, bot, 28, working(bot), ""));

    clear(&ui.chat_list);
    let entries: Vec<&Value> = st
        .entries
        .get(id)
        .map(|l| l.iter().filter(|e| e["threadId"].is_null()).collect())
        .unwrap_or_default();
    if !entries.iter().any(|e| is_chat(e)) {
        let intro = adw::StatusPage::builder()
            .title(bot["name"].as_str().unwrap_or(""))
            .build();
        if group {
            intro.set_description(Some(&format!(
                "{}\n\nEveryone answers in turn unless you @mention someone.",
                member_names(&st, bot)
            )));
        } else {
            intro.set_description(Some(&format!(
                "{} in {}\n\n{}\n\nTell it what you need.",
                backend_name(&st, bot["backend"].as_str().unwrap_or("")),
                bot["cwd"].as_str().unwrap_or(""),
                bot["description"].as_str().unwrap_or("")
            )));
        }
        ui.chat_list.append(&intro);
    }
    append_entries(ui, &st, &ui.chat_list, bot, &entries, true);
    if working_in(bot, id, None) {
        ui.chat_list.append(&working_row(&st, bot));
    }
    update_composer_for(ui, &st, bot, false);
    if stick {
        scroll_to_end(&ui.scroller);
    }
}

/// The thread open beside the chat: its root, the replies and a composer that replies there.
fn render_pane(ui: &App) {
    let st = ui.state.borrow();
    let (Some(bot), Some(root_id)) = (
        st.current.as_ref().and_then(|id| st.bots.get(id)),
        st.open_thread.as_deref(),
    ) else {
        ui.pane.set_reveal_child(false);
        ui.shown_thread.replace(String::new());
        return;
    };
    let id = bot["id"].as_str().unwrap_or_default();
    ui.pane.set_reveal_child(true);
    ui.pane_title
        .set_subtitle(bot["name"].as_str().unwrap_or(""));
    let switched = *ui.shown_thread.replace(root_id.to_owned()) != *root_id;
    let stick = switched || near_bottom(&ui.pane_scroller);
    clear(&ui.pane_list);
    let all = st.entries.get(id).map(Vec::as_slice).unwrap_or_default();
    if let Some(root) = all.iter().find(|e| e["id"] == root_id) {
        append_entries(ui, &st, &ui.pane_list, bot, &[root], false);
    }
    let replies: Vec<&Value> = all.iter().filter(|e| e["threadId"] == root_id).collect();
    let n = replies.iter().filter(|e| is_chat(e)).count();
    ui.pane_list.append(
        &gtk::Label::builder()
            .label(match n {
                0 => "No replies yet".to_owned(),
                1 => "1 reply".to_owned(),
                _ => format!("{n} replies"),
            })
            .xalign(0.0)
            .css_classes(["small", "muted"])
            .margin_top(12)
            .build(),
    );
    ui.pane_list
        .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    append_entries(ui, &st, &ui.pane_list, bot, &replies, false);
    if working_in(bot, id, Some(root_id)) {
        ui.pane_list.append(&working_row(&st, bot));
    }
    update_composer_for(ui, &st, bot, true);
    if stick {
        scroll_to_end(&ui.pane_scroller);
    }
    if switched {
        ui.reply.view.grab_focus();
    }
}

/// Chat-visible entries with time separators (gaps > 1 h) and author grouping.
/// `main`: the main chat, where messages can start threads and show their summary.
fn append_entries(
    ui: &App,
    st: &State,
    list: &gtk::Box,
    chat: &Value,
    entries: &[&Value],
    main: bool,
) {
    let group = is_group(chat);
    let mut last_author = String::new();
    let mut last_time = 0i64;
    for e in entries.iter().filter(|e| is_chat(e)) {
        let at = e["createdAt"].as_i64().unwrap_or(0);
        if at - last_time > 3_600_000 {
            list.append(
                &gtk::Label::builder()
                    .label(ago(at))
                    .css_classes(["small", "muted"])
                    .margin_top(12)
                    .margin_bottom(6)
                    .build(),
            );
            last_author.clear();
        }
        last_time = at;
        let kind = e["kind"].as_str().unwrap_or_default();
        // In a group each bot is its own author.
        let author = match kind {
            "agent" => format!("agent:{}", e["data"]["author"].as_str().unwrap_or("")),
            k => k.to_owned(),
        };
        let start = author != last_author;
        last_author = if kind == "user" || kind == "agent" {
            author
        } else {
            String::new()
        };
        let entry_id = e["id"].as_str().unwrap_or_default();
        let author_id = e["data"]["author"].as_str();
        let reply = |w: gtk::Widget, end: bool| {
            // Local echoes have no id on the host yet.
            if main && !entry_id.starts_with("local-") {
                with_reply(ui, &w, end, entry_id)
            } else {
                w
            }
        };
        match kind {
            "user" => list.append(&reply(user_bubble(e, working(chat), start), true)),
            "agent" => {
                let deleted = json!({"avatarColor": "gray"});
                let speaker = if group {
                    if start {
                        list.append(&author_label(st, author_id));
                    }
                    author_id.and_then(|a| st.bots.get(a)).unwrap_or(&deleted)
                } else {
                    chat
                };
                list.append(&reply(agent_bubble(e, speaker, start), false));
            }
            "permission" => {
                if group {
                    list.append(&author_label(st, author_id));
                }
                list.append(&permission_card(ui, st, e));
            }
            _ => list.append(&notice(e)),
        }
        if main && let Some(chip) = thread_chip(ui, st, e, kind == "user") {
            list.append(&chip);
        }
    }
}

fn working_row(st: &State, bot: &Value) -> gtk::Box {
    let row = gtk::Box::builder().spacing(8).margin_top(10).build();
    row.append(&avatar::of(&st.bots, bot, 28, true, ""));
    let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    col.append(
        &gtk::Label::builder()
            .label("•••")
            .css_classes(["bubble", "bubble-agent", "muted"])
            .halign(gtk::Align::Start)
            .build(),
    );
    let act = bot["activity"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Working…");
    col.append(
        &gtk::Label::builder()
            .label(act)
            .xalign(0.0)
            .css_classes([
                "small",
                if bot["status"] == "needsInput" {
                    "needs-text"
                } else {
                    "dim-label"
                },
            ])
            .build(),
    );
    row.append(&col);
    row
}

/// Starts (or opens) the thread on a main-chat message and pages in its replies.
pub fn open_thread(ui: &App, root: &str) {
    let Some(bot) = ui.state.borrow().current.clone() else {
        return;
    };
    ui.state.borrow_mut().open_thread = Some(root.to_owned());
    schedule(ui);
    let ui2 = ui.clone();
    client::call(
        "thread",
        json!({"botId": bot, "rootId": root}),
        move |r| match r {
            Ok(v) => {
                let mut st = ui2.state.borrow_mut();
                for e in v["entries"].as_array().into_iter().flatten() {
                    upsert(&mut st, e.clone());
                }
                drop(st);
                schedule(&ui2);
            }
            Err(e) => toast(&ui2, &e),
        },
    );
}

fn update_composer(ui: &App, in_thread: bool) {
    let st = ui.state.borrow();
    if let Some(bot) = st.current.as_ref().and_then(|id| st.bots.get(id)) {
        update_composer_for(ui, &st, bot, in_thread);
    }
}

fn update_composer_for(ui: &App, st: &State, chat: &Value, in_thread: bool) {
    let c = ui.composer(in_thread);
    let buf = c.view.buffer();
    let text = buf
        .text(&buf.start_iter(), &buf.end_iter(), false)
        .to_string();
    let has_text = !text.trim().is_empty();
    let lane = if in_thread {
        st.open_thread.as_deref()
    } else {
        None
    };
    let busy = working_in(chat, chat["id"].as_str().unwrap_or_default(), lane);
    c.stop_btn.set_visible(busy && !has_text);
    c.send_btn.set_visible(!busy || has_text);
    c.send_btn.set_sensitive(has_text);

    // `@` suggests the group's bots.
    clear(&c.mentions);
    let mut any = false;
    if is_group(chat)
        && let Some(q) = mention_query(&text)
    {
        let q = q.to_lowercase();
        for m in members(st, chat) {
            let name = m["name"].as_str().unwrap_or_default().to_owned();
            if !name.to_lowercase().contains(&q) {
                continue;
            }
            let inner = gtk::Box::builder().spacing(6).build();
            inner.append(&avatar::of(&st.bots, m, 18, false, ""));
            inner.append(&gtk::Label::new(Some(&name)));
            let chip = gtk::Button::builder()
                .child(&inner)
                .tooltip_text(format!("Ask {name}"))
                .css_classes(["flat", "thread-chip"])
                .build();
            let (buf2, view, text2) = (buf.clone(), c.view.clone(), text.clone());
            chip.connect_clicked(move |_| {
                if let Some(at) = text2.rfind('@') {
                    buf2.set_text(&format!("{}@{name} ", &text2[..at]));
                    buf2.place_cursor(&buf2.end_iter());
                    view.grab_focus();
                }
            });
            c.mentions.append(&chip);
            any = true;
        }
    }
    c.mentions_rev.set_reveal_child(any);
}

fn send(ui: &App, in_thread: bool) {
    let buf = ui.composer(in_thread).view.buffer();
    let text = buf
        .text(&buf.start_iter(), &buf.end_iter(), false)
        .trim()
        .to_owned();
    let (bot, thread) = {
        let st = ui.state.borrow();
        let thread = if in_thread {
            st.open_thread.clone()
        } else {
            None
        };
        (st.current.clone(), thread)
    };
    let Some(bot) = bot else {
        return;
    };
    if text.is_empty() || (in_thread && thread.is_none()) {
        return;
    }
    buf.set_text("");
    let nonce = uuid::Uuid::new_v4().to_string();
    upsert(
        &mut ui.state.borrow_mut(),
        json!({"id": format!("local-{nonce}"), "seq": i64::MAX, "botId": bot, "threadId": thread, "rev": 0, "kind": "user", "turn": 0,
               "data": {"text": text, "status": "sending", "clientNonce": nonce}, "createdAt": now_ms(), "updatedAt": 0}),
    );
    schedule(ui);
    let mut body = json!({"botId": bot, "text": text, "clientNonce": nonce});
    if let Some(t) = thread {
        body["threadId"] = t.into();
    }
    let ui2 = ui.clone();
    client::call("send", body, move |r| {
        let mut st = ui2.state.borrow_mut();
        match r {
            Ok(v) => upsert(&mut st, v["entry"].clone()),
            Err(_) => {
                if let Some(e) = st
                    .entries
                    .get_mut(&bot)
                    .and_then(|l| l.iter_mut().find(|e| e["id"] == format!("local-{nonce}")))
                {
                    e["data"]["status"] = "failed".into();
                }
            }
        }
        drop(st);
        schedule(&ui2);
    });
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub fn mark_read(ui: &App) {
    let st = ui.state.borrow();
    if let Some(bot) = st.current.as_ref().and_then(|id| st.bots.get(id))
        && bot["unread"].as_i64().unwrap_or(0) > 0
    {
        client::call("markRead", json!({"botId": bot["id"]}), |_| {});
    }
}

pub fn toast(ui: &App, text: &str) {
    ui.toasts.add_toast(adw::Toast::new(text));
}

#[cfg(test)]
mod tests {
    use super::mention_query;

    #[test]
    fn mentions() {
        assert_eq!(mention_query("hi @al"), Some("al"));
        assert_eq!(mention_query("@"), Some(""));
        assert_eq!(mention_query("mail me@home"), None);
        assert_eq!(mention_query("no at"), None);
        assert_eq!(mention_query("@al\nnext"), None);
    }
}
