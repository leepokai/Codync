//! Codync for Linux — the same bot chat as the iPhone and Mac apps, in GTK 4 + libadwaita.

mod avatar;
mod client;
mod dialogs;
mod markup;
mod rows;
mod ui;

use adw::prelude::*;

const CSS: &str = r#"
.bubble { border-radius: 18px; padding: 8px 13px; }
.bubble-user { background: @window_fg_color; color: @window_bg_color; }
.bubble-agent { padding-left: 0; padding-right: 0; }
.card { border-radius: 16px; padding: 14px; border: 1px solid alpha(currentColor, 0.15); background: alpha(currentColor, 0.03); }
.card.pending { border-color: alpha(#ff9800, 0.7); }
.accent-fill { background: @window_fg_color; color: @window_bg_color; }
.accent-fill:hover { background: alpha(@window_fg_color, 0.85); }
.accent-text { color: @window_fg_color; }
.needs-text { color: #e08600; }
.muted { opacity: 0.6; }
.small { font-size: 0.85em; }
.badge { background: @window_fg_color; color: @window_bg_color; border-radius: 999px; padding: 0 6px; font-weight: bold; font-size: 0.8em; min-width: 12px; }
.chip { border-radius: 999px; padding: 3px 10px; border: 1px solid alpha(currentColor, 0.15); }
.codebox { font-family: monospace; font-size: 0.9em; background: alpha(currentColor, 0.06); border-radius: 8px; padding: 8px; }
.composer { border-radius: 20px; padding: 8px 12px; border: 1px solid alpha(currentColor, 0.18); }
.composer textview, .composer text { background: transparent; }
.notice-error { background: alpha(#e0443e, 0.12); border-radius: 12px; padding: 10px 12px; }
.round { border-radius: 999px; min-width: 38px; min-height: 38px; padding: 0; }
"#;

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder().application_id("com.pokai.Codync").build();
    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("no display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    app.connect_activate(|app| {
        if let Some(win) = app.active_window() {
            win.present();
            return;
        }
        ui::build(app);
    });
    // Our own flags (`--bot <id>`) are read in ui::connect; keep GTK from rejecting them.
    app.run_with_args(&std::env::args().take(1).collect::<Vec<_>>())
}
