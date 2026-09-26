//! Main window, laid out like the Mac chat window (apps/macos/Views/ChatWindow.swift and
//! kit ThreadView): a roster sidebar (+, Search, rows, Account), and the conversation with
//! its header, the details panel or an open thread on the right, and the composer.

use crate::client::{self, Event};
use crate::rows::{
    self, Reply, agent_bubble, hline, icon_button, label, notice, permission_card, thread_chip,
    user_bubble,
};
use crate::{avatar, compose, dialogs, orb};
use adw::prelude::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    pub bots: HashMap<String, Value>,
    pub entries: HashMap<String, Vec<Value>>,
    pub usage: Value,
    pub hello: Value,
    /// Remote screen status (`screenStatus`), for the details panel.
    pub screen: Value,
    pub current: Option<String>,
    /// Root entry of the thread open beside the current chat.
    pub open_thread: Option<String>,
    /// The To: page is showing instead of a chat.
    pub composing: bool,
    /// The details panel beside the chat is closed.
    pub hide_details: bool,
    pub search: String,
    pub online: bool,
}

/// A message box: send, or stop while the chat (or thread) is working. In a group,
/// typing `@` suggests its bots.
pub struct Composer {
    view: gtk::TextView,
    placeholder: gtk::Label,
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
        .top_margin(6)
        .bottom_margin(6)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .max_content_height(150)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .hexpand(true)
        .build();
    let placeholder = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["tertiary"])
        .can_target(false)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .valign(gtk::Align::Center)
        .build();
    let field = gtk::Overlay::builder().child(&scroll).hexpand(true).build();
    field.add_overlay(&placeholder);
    let send_btn = gtk::Button::builder()
        .icon_name("go-up-symbolic")
        .tooltip_text("Send")
        .css_classes(["send"])
        .valign(gtk::Align::End)
        .margin_bottom(2)
        .build();
    let stop_btn = gtk::Button::builder()
        .icon_name("media-playback-stop-symbolic")
        .tooltip_text("Stop")
        .css_classes(["send"])
        .valign(gtk::Align::End)
        .margin_bottom(2)
        .build();
    let pill = gtk::Box::builder()
        .spacing(8)
        .css_classes(["composer"])
        .margin_start(16)
        .margin_end(16)
        .margin_top(6)
        .margin_bottom(16)
        .build();
    pill.append(&field);
    pill.append(&stop_btn);
    pill.append(&send_btn);
    let mentions = gtk::Box::builder()
        .spacing(6)
        .margin_start(16)
        .margin_end(16)
        .margin_top(6)
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
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    col.append(&mentions_rev);
    col.append(&pill);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(
        &adw::Clamp::builder()
            .maximum_size(820)
            .tightening_threshold(820)
            .child(&col)
            .build(),
    );
    Composer {
        view,
        placeholder,
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
    pub roster: gtk::ListBox,
    roster_empty: gtk::Label,
    banner: adw::Banner,
    pub search: gtk::SearchEntry,
    content_stack: gtk::Stack,
    chat_header: adw::HeaderBar,
    head_avatar: gtk::Box,
    head_name: gtk::Label,
    details_btn: gtk::Button,
    chat_list: gtk::Box,
    scroller: gtk::ScrolledWindow,
    compose: Composer,
    convo: gtk::Box,
    side: gtk::Revealer,
    side_stack: gtk::Stack,
    details_content: gtk::Box,
    details_gear: gtk::Button,
    thread_back: gtk::Button,
    close_thread: gtk::Button,
    /// Below the breakpoint an open thread covers the chat instead of sitting beside it.
    narrow: Cell<bool>,
    pane_sub: gtk::Label,
    pane_box: gtk::Box,
    pane_list: gtk::Box,
    pane_scroller: gtk::ScrolledWindow,
    reply: Composer,
    pub compose_page: compose::Page,
    /// Bot shown in the last render; switching bots jumps to the newest message.
    shown_bot: RefCell<String>,
    /// Thread shown in the last render; opening one jumps to its newest reply.
    shown_thread: RefCell<String>,
    pub state: RefCell<State>,
    rendering: Cell<bool>,
    /// Debug builds: `CODYNC_DEBUG_OPEN=compose | group | <bot name>[/thread]` opens a screen
    /// once the bots arrive (screenshot checks, like the Mac's).
    debug_open: RefCell<Option<String>>,
    account: gtk::Button,
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

/// A flat header bar (44 px) whose title area is `title`.
pub fn flat_header(title: &impl IsA<gtk::Widget>) -> adw::HeaderBar {
    adw::HeaderBar::builder()
        .title_widget(title)
        .css_classes(["flat-header"])
        .build()
}

/// The header of a panel right of the chat: it carries the window's end buttons.
fn side_header(content: &gtk::Box) -> adw::HeaderBar {
    content.set_hexpand(true);
    let h = flat_header(content);
    h.set_show_start_title_buttons(false);
    h
}

fn sidebar(
    new_btn: &gtk::Button,
    search: &gtk::SearchEntry,
    banner: &adw::Banner,
) -> (gtk::Box, gtk::ListBox, gtk::Label, gtk::Button) {
    let header = flat_header(&gtk::Box::builder().hexpand(true).build());
    header.pack_end(new_btn);
    let roster = gtk::ListBox::builder()
        .css_classes(["roster"])
        .selection_mode(gtk::SelectionMode::Single)
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .build();
    let roster_empty = gtk::Label::builder()
        .justify(gtk::Justification::Center)
        .use_markup(true)
        .can_target(false)
        .valign(gtk::Align::Center)
        .build();
    let list = gtk::Overlay::builder()
        .child(
            &gtk::ScrolledWindow::builder()
                .vexpand(true)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&roster)
                .build(),
        )
        .build();
    list.add_overlay(&roster_empty);
    let face = gtk::Image::builder()
        .icon_name("avatar-default-symbolic")
        .pixel_size(15)
        .width_request(28)
        .height_request(28)
        .css_classes(["person-circle"])
        .build();
    let acct_inner = gtk::Box::builder().spacing(10).build();
    acct_inner.append(&face);
    acct_inner.append(&gtk::Label::new(Some("Account")));
    let account = gtk::Button::builder()
        .child(&acct_inner)
        .css_classes(["footer-row"])
        .tooltip_text("Account menu")
        .build();
    let footer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(10)
        .build();
    footer.append(&account);
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["surface"])
        .build();
    col.append(&header);
    search.set_margin_start(12);
    search.set_margin_end(12);
    search.set_margin_top(4);
    search.set_margin_bottom(8);
    col.append(search);
    col.append(banner);
    col.append(&list);
    col.append(&footer);
    (col, roster, roster_empty, account)
}

fn empty_page(ui_new: &gtk::Button) -> gtk::Box {
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["chat-bg"])
        .build();
    page.append(&flat_header(&gtk::Box::new(
        gtk::Orientation::Horizontal,
        0,
    )));
    let center = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .margin_start(32)
        .margin_end(32)
        .margin_bottom(44)
        .build();
    use avatar::Mood;
    center.append(&avatar::row(
        &[
            ("blob", "blue", Mood::Working),
            ("squircle", "orange", Mood::Idle),
            ("teardrop", "violet", Mood::Working),
        ],
        60,
        10,
    ));
    let title = gtk::Label::builder()
        .label("Your coding agents, as teammates.")
        .css_classes(["title2"])
        .build();
    center.append(&title);
    center.append(
        &gtk::Label::builder()
            .label("Pick a bot, or create one for each kind of work and point it at a project.")
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(52)
            .css_classes(["body13", "secondary"])
            .build(),
    );
    ui_new.set_halign(gtk::Align::Center);
    center.append(ui_new);
    page.append(&center);
    page
}

pub fn build(app: &adw::Application) {
    let new_btn = icon_button("list-add-symbolic", "New chat (Ctrl+N)");
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search")
        .css_classes(["search-field"])
        .build();
    let banner = adw::Banner::new("Reconnecting to the Codync host…");
    let (sidebar_box, roster, roster_empty, account_btn) = sidebar(&new_btn, &search, &banner);
    let sidebar_page = adw::NavigationPage::new(&sidebar_box, "Codync");

    // Chat header: avatar + name (toggles details), details toggle, more.
    let head_avatar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let head_name = gtk::Label::builder()
        .css_classes(["body13", "semibold"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let head_inner = gtk::Box::builder().spacing(7).build();
    head_inner.append(&head_avatar);
    head_inner.append(&head_name);
    let head_btn = gtk::Button::builder()
        .child(&head_inner)
        .css_classes(["plain"])
        .tooltip_text("View conversation details")
        .build();
    let details_btn = icon_button("sidebar-show-right-symbolic", "Conversation details");
    let menu_btn = icon_button("view-more-symbolic", "More");
    let head_row = gtk::Box::builder()
        .spacing(4)
        .hexpand(true)
        .margin_start(10)
        .build();
    head_row.append(&head_btn);
    head_row.append(&gtk::Box::builder().hexpand(true).build());
    head_row.append(&details_btn);
    head_row.append(&menu_btn);
    let chat_header = flat_header(&head_row);

    let chat_list = entry_list();
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &adw::Clamp::builder()
                .maximum_size(820)
                .tightening_threshold(820)
                .child(&chat_list)
                .build(),
        )
        .build();
    let compose = composer();
    let convo = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .css_classes(["chat-bg"])
        .build();
    convo.append(&chat_header);
    convo.append(&hline());
    convo.append(&scroller);
    convo.append(&compose.root);

    // Details panel (the Mac inspector).
    let details_gear = icon_button("emblem-system-symbolic", "Bot settings");
    let details_close = icon_button("sidebar-show-right-symbolic", "Close details");
    let dh = gtk::Box::builder()
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .height_request(44)
        .build();
    dh.append(&gtk::Box::builder().hexpand(true).build());
    dh.append(&details_gear);
    dh.append(&details_close);
    let details_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(22)
        .margin_start(16)
        .margin_end(16)
        .margin_bottom(16)
        .build();
    let details_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(292)
        .css_classes(["chat-bg"])
        .build();
    details_box.append(&side_header(&dh));
    details_box.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&details_content)
            .build(),
    );

    // Thread pane (RepliesView).
    let pane_sub = label("", &["footnote", "tertiary"]);
    pane_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let titles = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    titles.append(&label("Thread", &["semibold"]));
    titles.append(&pane_sub);
    let thread_back = icon_button("go-previous-symbolic", "Back to chat");
    thread_back.set_visible(false);
    let close_thread = icon_button("window-close-symbolic", "Close thread (Esc)");
    let ph = gtk::Box::builder()
        .spacing(8)
        .margin_start(16)
        .margin_end(10)
        .height_request(48)
        .build();
    ph.append(&thread_back);
    ph.append(&titles);
    ph.append(&close_thread);
    let pane_list = entry_list();
    let pane_scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&pane_list)
        .build();
    let reply = composer();
    let pane_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(420)
        .css_classes(["chat-bg"])
        .build();
    pane_box.append(&side_header(&ph));
    pane_box.append(&hline());
    pane_box.append(&pane_scroller);
    pane_box.append(&reply.root);

    let side_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .hhomogeneous(false)
        .build();
    side_stack.add_named(&details_box, Some("details"));
    side_stack.add_named(&pane_box, Some("thread"));
    let side_outer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    side_outer.append(&gtk::Box::builder().css_classes(["vline"]).build());
    side_outer.append(&side_stack);
    let side = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideLeft)
        .child(&side_outer)
        .build();
    let chat_area = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chat_area.append(&convo);
    chat_area.append(&side);

    let new_bot = gtk::Button::builder()
        .label("New Bot")
        .css_classes(["primary-pill"])
        .build();
    let compose_page = compose::build();
    let content_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(150)
        .build();
    content_stack.add_named(&empty_page(&new_bot), Some("empty"));
    content_stack.add_named(&chat_area, Some("chat"));
    content_stack.add_named(&compose_page.root, Some("compose"));
    let content_page = adw::NavigationPage::new(&content_stack, "Chat");

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(260.0)
        .max_sidebar_width(296.0)
        .sidebar_width_fraction(0.27)
        .sidebar_width_unit(adw::LengthUnit::Px)
        .build();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&split));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Codync")
        .default_width(1100)
        .default_height(760)
        .width_request(360)
        .height_request(400)
        .content(&toasts)
        .build();
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        640.0,
        adw::LengthUnit::Px,
    ));
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp.clone());
    // Below this the details panel starts closed (the Mac's 680 pt conversation width).
    let bp_details = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        980.0,
        adw::LengthUnit::Px,
    ));
    window.add_breakpoint(bp_details.clone());

    let ui: App = Rc::new(Ui {
        window,
        toasts,
        split,
        roster,
        roster_empty,
        banner,
        search,
        content_stack,
        chat_header,
        head_avatar,
        head_name,
        details_btn: details_btn.clone(),
        chat_list,
        scroller,
        compose,
        convo,
        side,
        side_stack,
        details_content,
        details_gear: details_gear.clone(),
        thread_back: thread_back.clone(),
        close_thread: close_thread.clone(),
        narrow: Cell::new(false),
        pane_sub,
        pane_box,
        pane_list,
        pane_scroller,
        reply,
        compose_page,
        shown_bot: RefCell::new(String::new()),
        shown_thread: RefCell::new(String::new()),
        state: RefCell::new(State::default()),
        rendering: Cell::new(false),
        account: account_btn.clone(),
        debug_open: RefCell::new(if cfg!(debug_assertions) {
            std::env::var("CODYNC_DEBUG_OPEN").ok()
        } else {
            None
        }),
    });

    // Wiring
    {
        let ui2 = ui.clone();
        ui.roster.connect_row_activated(move |_, row| {
            select(&ui2, row.widget_name().as_str());
        });
    }
    {
        let ui2 = ui.clone();
        ui.search.connect_search_changed(move |s| {
            ui2.state.borrow_mut().search = s.text().to_string();
            render_roster(&ui2);
        });
    }
    {
        let ui2 = ui.clone();
        ui.search.connect_stop_search(move |s| {
            s.set_text("");
            ui2.roster.grab_focus();
        });
    }
    for b in [&new_btn, &new_bot] {
        let ui2 = ui.clone();
        b.connect_clicked(move |_| compose::open(&ui2));
    }
    {
        let ui2 = ui.clone();
        account_btn.connect_clicked(move |b| dialogs::account_menu(&ui2, b));
    }
    for b in [&close_thread, &thread_back] {
        let ui2 = ui.clone();
        b.connect_clicked(move |_| close_open_thread(&ui2));
    }
    for b in [&head_btn, &details_btn, &details_close] {
        let ui2 = ui.clone();
        b.connect_clicked(move |_| {
            let mut st = ui2.state.borrow_mut();
            st.hide_details = !st.hide_details;
            drop(st);
            schedule(&ui2);
        });
    }
    {
        let ui2 = ui.clone();
        details_gear.connect_clicked(move |_| {
            let bot = {
                let st = ui2.state.borrow();
                st.current.as_ref().and_then(|id| st.bots.get(id)).cloned()
            };
            if let Some(b) = bot {
                if is_group(&b) {
                    dialogs::group_editor(&ui2, Some(b));
                } else {
                    dialogs::editor(&ui2, Some(b));
                }
            }
        });
    }
    {
        let ui2 = ui.clone();
        menu_btn.connect_clicked(move |b| {
            if let Some(id) = ui2.state.borrow().current.clone() {
                dialogs::bot_menu(&ui2, b.upcast_ref(), None, &id, false);
            }
        });
    }
    for narrow in [true, false] {
        let ui2 = ui.clone();
        let f = move |_: &adw::Breakpoint| {
            ui2.narrow.set(narrow);
            schedule(&ui2);
        };
        if narrow {
            bp.connect_apply(f);
        } else {
            bp.connect_unapply(f);
        }
    }
    for compact in [true, false] {
        let ui2 = ui.clone();
        let f = move |_: &adw::Breakpoint| {
            ui2.state.borrow_mut().hide_details = compact;
            schedule(&ui2);
        };
        if compact {
            bp_details.connect_apply(f);
        } else {
            bp_details.connect_unapply(f);
        }
    }
    // Ctrl+N: new chat; Ctrl+F: search; Esc closes an open thread.
    {
        let actions = gtk::gio::SimpleActionGroup::new();
        let new_chat = gtk::gio::SimpleAction::new("new-chat", None);
        let ui2 = ui.clone();
        new_chat.connect_activate(move |_, _| compose::open(&ui2));
        actions.add_action(&new_chat);
        let find = gtk::gio::SimpleAction::new("search", None);
        let ui2 = ui.clone();
        find.connect_activate(move |_, _| {
            ui2.split.set_show_content(false);
            ui2.search.grab_focus();
        });
        actions.add_action(&find);
        ui.window.insert_action_group("codync", Some(&actions));
        app.set_accels_for_action("codync.new-chat", &["<Control>n"]);
        app.set_accels_for_action("codync.search", &["<Control>f"]);
        let keys = gtk::EventControllerKey::new();
        let ui2 = ui.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape && ui2.state.borrow().open_thread.is_some() {
                close_open_thread(&ui2);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        ui.window.add_controller(keys);
    }
    wire_composer(&ui, false);
    wire_composer(&ui, true);
    compose::wire(&ui);

    ui.window.present();
    connect(&ui);
}

fn close_open_thread(ui: &App) {
    ui.state.borrow_mut().open_thread = None;
    schedule(ui);
}

/// Opens a bot's (or group's) chat.
pub fn select(ui: &App, id: &str) {
    {
        let mut st = ui.state.borrow_mut();
        st.current = Some(id.to_owned());
        st.open_thread = None;
        st.composing = false;
    }
    ui.split.set_show_content(true);
    mark_read(ui);
    render(ui);
}

fn entry_list() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(16)
        .margin_end(16)
        .margin_top(8)
        .margin_bottom(8)
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
            refresh_screen(&ui2);
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

pub fn refresh_screen(ui: &App) {
    let ui2 = ui.clone();
    client::call("screenStatus", client::empty(), move |r| {
        ui2.state.borrow_mut().screen = r.unwrap_or(Value::Null);
        schedule(&ui2);
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
                "hello" | "usage" => st.usage = m["usage"].clone(),
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
    debug_open(ui);
}

fn debug_open(ui: &App) {
    let Some(target) = ui.debug_open.borrow().clone() else {
        return;
    };
    // Bots arrive one event at a time: give the catch-up a moment.
    if ui.state.borrow().bots.len() < 2 {
        return;
    }
    let ui2 = ui.clone();
    let target = target.clone();
    ui.debug_open.replace(None);
    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
        debug_open_now(&ui2, &target);
    });
}

fn debug_open_now(ui: &App, target: &str) {
    let (name, thread) = target.split_once('/').unwrap_or((target, ""));
    match name {
        "compose" => compose::open(ui),
        "group" => dialogs::group_editor(ui, None),
        "account" => {
            dialogs::account_menu(ui, &ui.account);
        }
        _ => {
            let st = ui.state.borrow();
            let Some(id) = st
                .bots
                .values()
                .find(|b| b["name"] == name)
                .and_then(|b| b["id"].as_str().map(str::to_owned))
            else {
                return;
            };
            let root = st.entries.get(&id).and_then(|l| {
                l.iter()
                    .rev()
                    .find(|e| e["data"]["thread"]["count"].as_i64().unwrap_or(0) > 0)
                    .and_then(|e| e["id"].as_str().map(str::to_owned))
            });
            if !thread.is_empty() && root.is_none() {
                return;
            }
            drop(st);
            select(ui, &id);
            if let Some(root) = root.filter(|_| !thread.is_empty()) {
                open_thread(ui, &root);
            }
        }
    }
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
    render_roster(ui);
    let composing = ui.state.borrow().composing;
    if composing {
        ui.content_stack.set_visible_child_name("compose");
        compose::render(ui);
        return;
    }
    render_chat(ui);
    render_side(ui);
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

pub fn is_group(b: &Value) -> bool {
    b["kind"] == "group"
}

pub fn members<'a>(st: &'a State, group: &Value) -> Vec<&'a Value> {
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
        .map_or_else(|| p.to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The sidebar's bots, pinned first then most recent (the Mac `roster`).
pub fn roster(st: &State) -> Vec<&Value> {
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
    bots
}

/// Live activity while working, otherwise the last message (BotRow's preview).
fn preview(st: &State, b: &Value) -> gtk::Widget {
    let row = gtk::Box::builder().spacing(6).hexpand(true).build();
    let text = |s: &str, class: &str| {
        let l = label(&s.replace('\n', " "), &["small", class]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_hexpand(true);
        l
    };
    let activity = b["activity"].as_str().filter(|s| !s.is_empty());
    match b["status"].as_str() {
        Some("needsInput") => {
            let o = orb::widget(true, 16);
            o.add_css_class("warning-text");
            row.append(&o);
            row.append(&text(
                activity.unwrap_or("Needs your approval"),
                "warning-text",
            ));
        }
        Some("working") => {
            let o = orb::widget(false, 13);
            o.add_css_class("secondary");
            row.append(&o);
            row.append(&text(activity.unwrap_or("Working…"), "secondary"));
        }
        Some("error") => row.append(&text(
            b["lastMessage"].as_str().unwrap_or("Something went wrong"),
            "danger-text",
        )),
        _ => row.append(&text(
            &b["lastMessage"].as_str().map_or_else(
                || {
                    if is_group(b) {
                        member_names(st, b)
                    } else {
                        format!(
                            "{} · {}",
                            backend_name(st, b["backend"].as_str().unwrap_or("")),
                            folder(b["cwd"].as_str().unwrap_or(""))
                        )
                    }
                },
                str::to_owned,
            ),
            "secondary",
        )),
    }
    row.upcast()
}

fn render_roster(ui: &App) {
    let st = ui.state.borrow();
    let q = st.search.trim().to_lowercase();
    let bots: Vec<&Value> = roster(&st)
        .into_iter()
        .filter(|b| {
            q.is_empty()
                || [&b["name"], &b["lastMessage"]]
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.to_lowercase().contains(&q)))
        })
        .collect();
    while let Some(c) = ui.roster.first_child() {
        ui.roster.remove(&c);
    }
    ui.roster_empty.set_visible(bots.is_empty());
    ui.roster_empty.set_markup(if q.is_empty() {
        "<span weight=\"500\" size=\"13pt\">No bots yet</span>\n<span size=\"12pt\" alpha=\"60%\">Use + to start a new chat.</span>"
    } else {
        "<span weight=\"500\" size=\"13pt\">No matching bots</span>\n<span size=\"12pt\" alpha=\"60%\">Try another name or message.</span>"
    });
    for b in bots {
        let id = b["id"].as_str().unwrap_or_default();
        let unread = b["unread"].as_i64().unwrap_or(0);
        let row_box = gtk::Box::builder().spacing(8).build();
        row_box.append(&avatar::with_status(&st.bots, b, 30));
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(1)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let top = gtk::Box::builder().spacing(4).build();
        if b["pinned"] == true {
            let pin = gtk::Image::from_icon_name("view-pin-symbolic");
            pin.set_pixel_size(10);
            pin.add_css_class("tertiary");
            top.append(&pin);
        }
        let name = label(b["name"].as_str().unwrap_or(""), &["name"]);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        top.append(&name);
        text.append(&top);
        let bottom = gtk::Box::builder().spacing(6).build();
        bottom.append(&preview(&st, b));
        if unread > 0 {
            bottom.append(&label(&unread.to_string(), &["badge"]));
        }
        text.append(&bottom);
        row_box.append(&text);
        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .name(id)
            .tooltip_text(b["name"].as_str().unwrap_or(""))
            .build();
        // Right-click: the conversation menu (Pin, Mark as Read, Edit, …).
        let click = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_SECONDARY)
            .build();
        let (ui2, id2, row2) = (ui.clone(), id.to_owned(), row.clone());
        click.connect_pressed(move |_, _, x, y| {
            dialogs::bot_menu(&ui2, row2.upcast_ref(), Some((x, y)), &id2, true);
        });
        row.add_controller(click);
        ui.roster.append(&row);
        if st.current.as_deref() == Some(id) && !st.composing {
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

pub fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn near_bottom(s: &gtk::ScrolledWindow) -> bool {
    let adj = s.vadjustment();
    adj.upper() - adj.value() - adj.page_size() < 120.0
}

fn scroll_to_end(s: &gtk::ScrolledWindow) {
    // Labels finish wrapping (and the height settles) over the next frames: follow the
    // bottom for a moment.
    let start = std::time::Instant::now();
    s.add_tick_callback(move |s, _| {
        let adj = s.vadjustment();
        adj.set_value(adj.upper() - adj.page_size());
        if start.elapsed() < std::time::Duration::from_millis(800) {
            gtk::glib::ControlFlow::Continue
        } else {
            gtk::glib::ControlFlow::Break
        }
    });
}

/// The start of an empty chat: who it is and how it works (IntroCard / GroupIntroCard).
fn intro(st: &State, bot: &Value) -> gtk::Box {
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(40)
        .margin_start(24)
        .margin_end(24)
        .build();
    col.append(&avatar::of(&st.bots, bot, 72, true));
    let centered = |text: &str, classes: &[&str]| {
        let l = label(text, classes);
        l.set_xalign(0.5);
        l.set_wrap(true);
        l.set_justify(gtk::Justification::Center);
        l
    };
    col.append(&centered(bot["name"].as_str().unwrap_or(""), &["title2"]));
    let desc = bot["description"].as_str().unwrap_or("");
    if is_group(bot) {
        col.append(&centered(&member_names(st, bot), &["small", "secondary"]));
        if !desc.is_empty() {
            col.append(&centered(desc, &["small", "secondary"]));
        }
        let tip = centered(
            "Everyone answers in turn. @mention a bot to ask just that one. Each bot works in its own folder.",
            &["footnote", "tertiary"],
        );
        tip.set_margin_top(4);
        col.append(&tip);
    } else {
        col.append(&centered(
            &format!(
                "{} in {}",
                backend_name(st, bot["backend"].as_str().unwrap_or("")),
                bot["cwd"].as_str().unwrap_or("")
            ),
            &["footnote", "mono", "tertiary"],
        ));
        if !desc.is_empty() {
            col.append(&centered(desc, &["small", "secondary"]));
        }
        let tip = centered(
            "Tell it what you need. You'll get a notification when it's done or needs you.",
            &["footnote", "tertiary"],
        );
        tip.set_margin_top(4);
        col.append(&tip);
    }
    col
}

fn render_chat(ui: &App) {
    let st = ui.state.borrow();
    let Some(bot) = st.current.as_ref().and_then(|id| st.bots.get(id)) else {
        ui.content_stack.set_visible_child_name("empty");
        return;
    };
    let id = bot["id"].as_str().unwrap_or_default();
    ui.content_stack.set_visible_child_name("chat");
    let switched = *ui.shown_bot.replace(id.to_owned()) != *id;
    let stick = switched || near_bottom(&ui.scroller);
    ui.head_name.set_label(bot["name"].as_str().unwrap_or(""));
    clear(&ui.head_avatar);
    ui.head_avatar.append(&avatar::of(&st.bots, bot, 22, true));

    clear(&ui.chat_list);
    let entries: Vec<&Value> = st
        .entries
        .get(id)
        .map(|l| l.iter().filter(|e| e["threadId"].is_null()).collect())
        .unwrap_or_default();
    if !entries.iter().any(|e| is_chat(e)) {
        ui.chat_list.append(&intro(&st, bot));
    }
    append_entries(ui, &st, &ui.chat_list, bot, &entries, true);
    if working_in(bot, id, None) && st.online {
        ui.chat_list.append(&rows::working(ui, bot));
    }
    update_composer_for(ui, &st, bot, false);
    if stick {
        scroll_to_end(&ui.scroller);
    }
    if switched {
        ui.compose.view.grab_focus();
    }
}

/// The right side of a chat: an open thread, else the details panel (unless closed).
fn render_side(ui: &App) {
    let st = ui.state.borrow();
    let Some(bot) = st.current.as_ref().and_then(|id| st.bots.get(id)) else {
        ui.shown_thread.replace(String::new());
        return;
    };
    let thread = st.open_thread.clone();
    let details = !st.hide_details && thread.is_none();
    ui.details_btn.set_visible(st.hide_details);
    let open = thread.is_some() || details;
    // The window's own buttons move into whatever is rightmost.
    ui.chat_header.set_show_end_title_buttons(!open);
    layout_side(ui, thread.is_some());
    ui.side.set_reveal_child(open);
    if details {
        ui.side_stack.set_visible_child_name("details");
        render_details(ui, &st, bot);
    }
    match thread {
        Some(root) => {
            ui.side_stack.set_visible_child_name("thread");
            render_thread(ui, &st, bot, &root);
        }
        None => {
            ui.shown_thread.replace(String::new());
        }
    }
}

fn render_details(ui: &App, st: &State, bot: &Value) {
    let c = &ui.details_content;
    clear(c);
    let group = is_group(bot);
    ui.details_gear
        .set_tooltip_text(Some(if group { "Edit group" } else { "Bot settings" }));
    if group {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();
        let ms = members(st, bot);
        let title = label(&format!("{} bots", ms.len()), &["body13", "semibold"]);
        title.set_margin_bottom(6);
        list.append(&title);
        for m in ms {
            let row = gtk::Box::builder()
                .spacing(10)
                .margin_top(6)
                .margin_bottom(6)
                .build();
            row.append(&avatar::of(&st.bots, m, 26, true));
            let text = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(1)
                .build();
            text.append(&label(m["name"].as_str().unwrap_or(""), &["body13"]));
            let sub = if working(m) {
                m["activity"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Working…")
                    .to_owned()
            } else {
                folder(m["cwd"].as_str().unwrap_or(""))
            };
            let s = label(&sub, &["small", "tertiary"]);
            s.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&s);
            row.append(&text);
            let name = m["name"].as_str().unwrap_or("");
            let btn = gtk::Button::builder()
                .child(&row)
                .css_classes(["plain"])
                .tooltip_text(format!("Open {name}'s own chat"))
                .build();
            let (ui2, mid) = (ui.clone(), m["id"].as_str().unwrap_or_default().to_owned());
            btn.connect_clicked(move |_| select(&ui2, &mid));
            list.append(&btn);
        }
        c.append(&list);
        return;
    }
    let id = bot["id"].as_str().unwrap_or_default();
    let screen = &st.screen;
    let status = if !st.online {
        "Computer is offline"
    } else if screen["enabled"] != true {
        "Remote screen is off"
    } else if screen["connected"] != true {
        "Connecting to computer…"
    } else if screen["agentBot"] == id {
        "This bot is using your computer"
    } else {
        "Ready · View this computer from your iPhone"
    };
    let computer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .css_classes(["details-box"])
        .height_request(164)
        .build();
    let icon = gtk::Image::from_icon_name("computer-symbolic");
    icon.set_pixel_size(32);
    let s = label(status, &["small", "secondary"]);
    s.set_xalign(0.5);
    s.set_wrap(true);
    s.set_justify(gtk::Justification::Center);
    // Centered in the 164 pt box.
    computer.append(&gtk::Box::builder().vexpand(true).build());
    computer.append(&icon);
    computer.append(&s);
    computer.append(&gtk::Box::builder().vexpand(true).build());
    let top = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    top.append(&computer);
    let host = label(
        st.hello["name"].as_str().unwrap_or(""),
        &["footnote", "tertiary"],
    );
    host.set_xalign(0.5);
    top.append(&host);
    c.append(&top);
    let agent = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    agent.append(&label("Agent", &["body13", "semibold"]));
    agent.append(&label(
        &backend_name(st, bot["backend"].as_str().unwrap_or("")),
        &["body13", "secondary"],
    ));
    let cwd = label(bot["cwd"].as_str().unwrap_or(""), &["secondary"]);
    cwd.set_selectable(true);
    cwd.set_wrap(true);
    cwd.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    agent.append(&cwd);
    if let Some(d) = bot["description"].as_str().filter(|d| !d.is_empty()) {
        let l = label(d, &["body13", "secondary"]);
        l.set_wrap(true);
        agent.append(&l);
    }
    c.append(&agent);
}

/// The thread open beside the chat: its root, the replies and a composer that replies there.
fn render_thread(ui: &App, st: &State, bot: &Value, root_id: &str) {
    let id = bot["id"].as_str().unwrap_or_default();
    ui.pane_sub.set_label(bot["name"].as_str().unwrap_or(""));
    let switched = *ui.shown_thread.replace(root_id.to_owned()) != *root_id;
    let stick = switched || near_bottom(&ui.pane_scroller);
    clear(&ui.pane_list);
    let all = st.entries.get(id).map(Vec::as_slice).unwrap_or_default();
    let replies: Vec<&Value> = all.iter().filter(|e| e["threadId"] == root_id).collect();
    if let Some(root) = all.iter().find(|e| e["id"] == root_id) {
        append_entries(ui, st, &ui.pane_list, bot, &[root], false);
        let n = replies.iter().filter(|e| is_chat(e)).count();
        let div = gtk::Box::builder()
            .spacing(10)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        div.append(&label(
            &match n {
                0 => "No replies yet".to_owned(),
                1 => "1 reply".to_owned(),
                _ => format!("{n} replies"),
            },
            &["footnote", "tertiary"],
        ));
        let line = hline();
        line.set_hexpand(true);
        line.set_valign(gtk::Align::Center);
        div.append(&line);
        ui.pane_list.append(&div);
    }
    append_entries(ui, st, &ui.pane_list, bot, &replies, false);
    if working_in(bot, id, Some(root_id)) && st.online {
        ui.pane_list.append(&rows::working(ui, bot));
    }
    update_composer_for(ui, st, bot, true);
    if stick {
        scroll_to_end(&ui.pane_scroller);
    }
    if switched {
        ui.reply.view.grab_focus();
    }
}

/// Wide windows keep the thread in a side pane; narrow ones let it cover the chat,
/// with a back button in place of close.
fn layout_side(ui: &App, thread: bool) {
    let cover = thread && ui.narrow.get();
    ui.convo.set_visible(!cover);
    ui.side.set_hexpand(cover);
    ui.pane_box.set_hexpand(cover);
    ui.pane_box.set_width_request(if cover { -1 } else { 420 });
    ui.thread_back.set_visible(cover);
    ui.close_thread.set_visible(!cover);
}

/// Chat-visible entries with time separators (gaps > 1 h, main chat) and author grouping.
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
    let mut last_time: Option<i64> = None;
    for e in entries.iter().filter(|e| is_chat(e)) {
        let at = e["createdAt"].as_i64().unwrap_or(0);
        if main && last_time.is_none_or(|t| at - t > 3_600_000) {
            let sep = label(&rows::separator(at), &["footnote", "tertiary"]);
            sep.set_xalign(0.5);
            sep.set_margin_top(18);
            sep.set_margin_bottom(6);
            list.append(&sep);
            last_author.clear();
        }
        last_time = Some(at);
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
        // Local echoes have no id on the host yet.
        let r = Reply {
            ui,
            root: (main && !entry_id.starts_with("local-")).then_some(entry_id),
        };
        match kind {
            "user" => list.append(&user_bubble(&r, e, working(chat), start)),
            "agent" => list.append(&agent_bubble(&r, st, e, group, start)),
            "permission" => list.append(&permission_card(ui, st, e, group)),
            _ => list.append(&notice(e)),
        }
        let indent = if group && kind == "agent" { 34 } else { 0 };
        if main && let Some(chip) = thread_chip(ui, st, e, kind == "user", indent) {
            list.append(&chip);
        }
    }
}

/// Starts (or opens) the thread on a main-chat message and pages in its replies.
pub fn open_thread(ui: &App, root: &str) {
    let Some(bot) = ui.state.borrow().current.clone() else {
        return;
    };
    ui.state.borrow_mut().open_thread = Some(root.to_owned());
    schedule(ui);
    mark_read(ui);
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
    let name = chat["name"].as_str().unwrap_or("");
    c.placeholder.set_label(&if in_thread {
        "Reply…".to_owned()
    } else if is_group(chat) {
        format!("Message {name} · @ to ask one bot")
    } else if busy {
        format!("Queue a message for {name}")
    } else {
        format!("Message {name}")
    });
    c.placeholder.set_visible(text.is_empty());
    c.stop_btn.set_visible(busy && text.is_empty());
    c.send_btn.set_visible(!(busy && text.is_empty()));
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
            inner.append(&avatar::of(&st.bots, m, 18, false));
            inner.append(&label(&name, &["small"]));
            let chip = gtk::Button::builder()
                .child(&inner)
                .tooltip_text(format!("Ask {name}"))
                .css_classes(["mention-chip"])
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
    send_text(ui, &bot, &text, thread);
}

/// Sends a message with a local echo that the host's copy replaces.
pub fn send_text(ui: &App, bot: &str, text: &str, thread: Option<String>) {
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
    let (ui2, bot) = (ui.clone(), bot.to_owned());
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

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
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

// MARK: menus

/// One row of a floating menu panel.
pub struct MenuItem {
    icon: &'static str,
    title: String,
    detail: Option<String>,
    chevron: bool,
    destructive: bool,
    divider: bool,
    action: Box<dyn Fn()>,
}

impl MenuItem {
    pub fn new(icon: &'static str, title: &str, action: Box<dyn Fn()>) -> Self {
        Self {
            icon,
            title: title.to_owned(),
            detail: None,
            chevron: false,
            destructive: false,
            divider: false,
            action,
        }
    }
    pub fn detail(mut self, d: Option<String>) -> Self {
        self.detail = d;
        self
    }
    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
    /// A line above this row.
    pub fn divider(mut self) -> Self {
        self.divider = true;
        self
    }
}

/// A floating panel of rows (DesktopActionMenu): at `point` inside `parent`, or under it.
pub fn popup_menu(parent: &impl IsA<gtk::Widget>, point: Option<(f64, f64)>, items: Vec<MenuItem>) {
    let pop = gtk::Popover::builder()
        .has_arrow(false)
        .css_classes(["panel"])
        .build();
    if let Some((x, y)) = point {
        pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        pop.set_position(gtk::PositionType::Bottom);
    }
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .width_request(246)
        .build();
    for item in items {
        if item.divider {
            col.append(&gtk::Box::builder().css_classes(["menu-divider"]).build());
        }
        let row = gtk::Box::builder().spacing(8).build();
        let icon = gtk::Image::from_icon_name(item.icon);
        icon.set_pixel_size(14);
        icon.set_width_request(20);
        row.append(&icon);
        let t = label(&item.title, &[]);
        t.set_hexpand(true);
        row.append(&t);
        if let Some(d) = &item.detail {
            row.append(&label(d, &["body13", "secondary"]));
        }
        if item.chevron {
            let c = gtk::Image::from_icon_name("go-next-symbolic");
            c.set_pixel_size(11);
            c.add_css_class("secondary");
            row.append(&c);
        }
        let btn = gtk::Button::builder()
            .child(&row)
            .css_classes(["menu-row"])
            .build();
        if item.destructive {
            btn.add_css_class("danger-text");
        }
        let pop2 = pop.clone();
        let action = item.action;
        btn.connect_clicked(move |_| {
            pop2.popdown();
            action();
        });
        col.append(&btn);
    }
    pop.set_child(Some(&col));
    // Rows re-render often; the popover lives only while shown.
    pop.connect_closed(|p| {
        let p = p.clone();
        gtk::glib::idle_add_local_once(move || p.unparent());
    });
    pop.set_parent(parent);
    pop.popup();
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
