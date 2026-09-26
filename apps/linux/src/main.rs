//! Codync for Linux — the same bot chat as the iPhone and Mac apps, in GTK 4 + libadwaita.

mod avatar;
mod client;
mod compose;
mod dialogs;
mod markup;
mod orb;
mod rows;
mod ui;

use adw::prelude::*;

/// The Apple apps' `Palette` (CodyncKit/Design/Theme.swift), light then dark.
struct Palette {
    background: &'static str,
    surface: &'static str,
    bubble_agent: &'static str,
    bubble_user: &'static str,
    border: &'static str,
    text: &'static str,
    secondary: &'static str,
    tertiary: &'static str,
    accent_fill: &'static str,
    on_accent: &'static str,
    accent_dim: &'static str,
    danger: &'static str,
    code: &'static str,
    panel: &'static str,
}

const LIGHT: Palette = Palette {
    background: "#FFFFFF",
    surface: "#F4F4F4",
    bubble_agent: "#F0F0F0",
    bubble_user: "#E2E2E2",
    border: "#E6E6E6",
    text: "#141414",
    secondary: "#6B6B6B",
    tertiary: "#9B9B9B",
    accent_fill: "#000000",
    on_accent: "#FFFFFF",
    accent_dim: "#D9D9D9",
    danger: "#C23A2B",
    code: "#F4F4F4",
    panel: "#FAFAFA",
};

const DARK: Palette = Palette {
    background: "#0A0A0A",
    surface: "#141414",
    bubble_agent: "#1C1C1C",
    bubble_user: "#3A3A3A",
    border: "#262626",
    text: "#F2F2F2",
    secondary: "#9A9A9A",
    tertiary: "#6E6E6E",
    accent_fill: "#FFFFFF",
    on_accent: "#000000",
    accent_dim: "#333333",
    danger: "#F0A7A7",
    code: "#111111",
    panel: "#1B1B1B",
};

fn colors(p: &Palette) -> String {
    format!(
        "@define-color cd_bg {};\n@define-color cd_surface {};\n@define-color cd_agent {};\n@define-color cd_user {};\n\
         @define-color cd_border {};\n@define-color cd_text {};\n@define-color cd_secondary {};\n@define-color cd_tertiary {};\n\
         @define-color cd_accent {};\n@define-color cd_on_accent {};\n@define-color cd_accent_dim {};\n@define-color cd_danger {};\n\
         @define-color cd_code {};\n@define-color cd_panel {};\n@define-color cd_warning #F0A030;\n",
        p.background,
        p.surface,
        p.bubble_agent,
        p.bubble_user,
        p.border,
        p.text,
        p.secondary,
        p.tertiary,
        p.accent_fill,
        p.on_accent,
        p.accent_dim,
        p.danger,
        p.code,
        p.panel
    )
}

/// Sizes follow the Mac app (SwiftUI points = GTK px): body 12, secondary 11,
/// footnote/caption 10, headline 13, title3 15, title2 17.
const CSS: &str = r#"
@define-color window_bg_color @cd_bg;
@define-color window_fg_color @cd_text;
@define-color view_bg_color @cd_bg;
@define-color view_fg_color @cd_text;
@define-color headerbar_bg_color @cd_bg;
@define-color headerbar_fg_color @cd_text;
@define-color headerbar_border_color @cd_border;
@define-color headerbar_shade_color transparent;
@define-color sidebar_bg_color @cd_surface;
@define-color sidebar_fg_color @cd_text;
@define-color sidebar_border_color @cd_border;
@define-color dialog_bg_color @cd_bg;
@define-color dialog_fg_color @cd_text;
@define-color popover_bg_color @cd_panel;
@define-color popover_fg_color @cd_text;
@define-color card_bg_color @cd_agent;
@define-color accent_bg_color #0A84FF;
@define-color accent_color @cd_text;

window { font-size: 12px; }
.surface { background: @cd_surface; }
.chat-bg { background: @cd_bg; }
headerbar.flat-header { background: transparent; box-shadow: none; border: none; min-height: 44px; padding: 0; }
headerbar.flat-header windowhandle > box { padding: 0 6px; }
.hline { background: @cd_border; min-height: 1px; }
.vline { background: @cd_border; min-width: 1px; }

.name { font-weight: 600; font-size: 12px; color: @cd_text; }
.secondary { color: @cd_secondary; }
.tertiary { color: @cd_tertiary; }
.warning-text { color: @cd_warning; }
.danger-text { color: @cd_danger; }
.small { font-size: 11px; }
.footnote { font-size: 10px; }
.headline { font-size: 13px; font-weight: 700; }
.title3 { font-size: 15px; }
.title2 { font-size: 17px; font-weight: 600; }
.body13 { font-size: 13px; }
.semibold { font-weight: 600; }
.medium { font-weight: 500; }
.mono { font-family: monospace; }

/* Icon-only button (IconButtonStyle): no fill until hovered. */
button.icon-btn { min-width: 28px; min-height: 28px; padding: 0; border-radius: 8px; background: transparent;
  color: @cd_secondary; box-shadow: none; border: none; transition: background 120ms, color 120ms; }
button.icon-btn:hover { background: alpha(@cd_text, 0.08); color: @cd_text; }
button.icon-btn:active { background: alpha(@cd_text, 0.14); }
button.plain { background: transparent; box-shadow: none; border: none; padding: 0; min-height: 0; min-width: 0; }
button.plain:hover { background: transparent; }

button.primary-pill { background: @cd_accent; color: @cd_on_accent; border-radius: 999px; padding: 6px 14px;
  font-weight: 500; font-size: 13px; box-shadow: none; border: none; min-height: 0; }
button.primary-pill:hover { background: alpha(@cd_accent, 0.88); }
button.primary-pill:disabled { opacity: 0.4; }
button.secondary-pill { background: @cd_user; color: @cd_text; border-radius: 999px; padding: 6px 14px;
  font-weight: 500; box-shadow: none; border: none; min-height: 0; }
button.secondary-pill.danger-text { color: @cd_danger; }

/* Sidebar */
entry.search-field, searchbar entry, entry.search-field:focus-within { background: @cd_agent; border-radius: 8px; min-height: 28px;
  box-shadow: none; outline: none; border: none; font-size: 13px; padding: 0 9px; }
entry.search-field image { color: @cd_secondary; }
list.roster { background: transparent; }
list.roster > row { border-radius: 10px; padding: 6px 8px; margin-bottom: 2px; background: transparent;
  transition: background 120ms; outline: none; }
list.roster > row:hover { background: @cd_agent; }
list.roster > row:selected, list.roster > row:selected:hover { background: @cd_user; color: @cd_text; }
.badge { background: @cd_accent; color: @cd_on_accent; border-radius: 999px; padding: 0 6px; min-width: 6px;
  min-height: 18px; font-weight: bold; font-size: 10px; }
button.footer-row { border-radius: 10px; padding: 0 8px; min-height: 38px; background: transparent; box-shadow: none;
  border: none; font-size: 13px; font-weight: 500; color: @cd_text; transition: background 120ms; }
button.footer-row:hover, button.footer-row:checked { background: @cd_agent; }
.person-circle { background: @cd_user; border-radius: 999px; color: @cd_secondary; }

/* Menus: a floating panel of rows (DesktopActionMenu / SidebarAccountPanel). */
popover.panel > contents { background: @cd_panel; border-radius: 18px; padding: 7px; box-shadow: 0 8px 20px rgba(0,0,0,0.25); }
popover.panel > arrow { background: transparent; border: none; }
button.menu-row { min-height: 30px; padding: 0 10px; border-radius: 10px; background: transparent; box-shadow: none;
  border: none; font-size: 12px; font-weight: normal; color: @cd_text; }
button.menu-row:hover, button.menu-row:focus-visible { background: alpha(@cd_text, 0.08); }
button.menu-row.danger-text { color: @cd_danger; }
.menu-divider { background: alpha(@cd_text, 0.13); min-height: 1px; margin: 5px 10px; }

/* Conversation */
.bubble { border-radius: 22px; padding: 8px 12px; font-size: 12px; color: @cd_text; }
.bubble-user { background: @cd_user; }
.bubble-agent { background: @cd_agent; }
.time { font-size: 10px; color: @cd_tertiary; }
.author { font-size: 11px; font-weight: 500; }
button.thread-chip { border-radius: 999px; padding: 5px 10px; background: @cd_surface; box-shadow: none; border: none;
  min-height: 0; transition: background 120ms; }
button.thread-chip:hover { background: @cd_agent; }
button.mention-chip { border-radius: 999px; padding: 6px 10px; background: @cd_surface; box-shadow: none; border: none; min-height: 0; }
button.working { background: @cd_agent; border-radius: 22px; padding: 12px; box-shadow: none; border: none; min-height: 0; }
.perm-card { background: @cd_agent; border-radius: 22px; padding: 16px; }
.choices { background: @cd_bg; border-radius: 14px; }
button.choice { min-height: 46px; padding: 0 14px; border-radius: 0; background: transparent; box-shadow: none; border: none;
  font-size: 13px; }
button.choice:first-child { border-radius: 14px 14px 0 0; }
button.choice:last-child { border-radius: 0 0 14px 14px; }
button.choice:hover { background: alpha(@cd_text, 0.05); }
.codebox { font-family: monospace; font-size: 10px; background: @cd_code; border-radius: 8px; padding: 8px; }
.notice-error { background: alpha(@cd_danger, 0.12); border-radius: 12px; padding: 12px; }
.composer { background: @cd_user; border-radius: 24px; padding: 4px 6px 4px 14px; box-shadow: 0 2px 10px rgba(0,0,0,0.06); }
.composer textview, .composer textview text, .composer scrolledwindow { background: transparent; font-size: 12px; }
button.send { min-width: 28px; min-height: 28px; padding: 0; border-radius: 999px; background: @cd_accent;
  color: @cd_on_accent; box-shadow: none; border: none; }
button.send:disabled { background: @cd_accent_dim; color: @cd_tertiary; }
.reply-btn { opacity: 0; transition: opacity 150ms ease-out; }
.reply-host:hover .reply-btn, .reply-btn:focus-visible { opacity: 1; }
.details-box { background: @cd_surface; border-radius: 8px; }

/* Compose page and editors */
floating-sheet > sheet, dialog.floating sheet, dialog sheet { outline: none; border: none; box-shadow: 0 10px 40px rgba(0,0,0,0.45); border-radius: 18px; }
entry.plain, entry.plain:focus-within { background: transparent; box-shadow: none; outline: none; border: none; padding: 0; min-height: 0; }
.chip { background: @cd_agent; border-radius: 999px; padding: 3px 4px 3px 8px; }
button.chip-x { min-width: 18px; min-height: 18px; padding: 0; background: transparent; box-shadow: none; border: none; color: @cd_secondary; }
.pick-card { background: @cd_surface; border-radius: 18px; padding: 8px; box-shadow: 0 6px 16px rgba(0,0,0,0.08); }
button.pick-row { padding: 8px 12px; border-radius: 12px; background: transparent; box-shadow: none; border: none;
  font-weight: normal; font-size: 13px; transition: background 120ms; }
button.pick-row:hover, button.pick-row.first { background: @cd_agent; }
.keycap { font-family: monospace; font-size: 10px; color: @cd_secondary; background: @cd_bg; border-radius: 4px;
  min-width: 18px; min-height: 18px; padding: 0 3px; }
.plus-circle { background: @cd_agent; border-radius: 999px; }
.field-label { font-size: 11px; color: @cd_secondary; margin-left: 4px; }
entry.field-box, entry.field-box:focus-within { background: @cd_surface; border-radius: 12px; padding: 0 10px; min-height: 32px;
  box-shadow: none; outline: none; border: none; }
.field-box-area { background: @cd_surface; border-radius: 12px; padding: 8px 10px; }
.field-box-area textview, .field-box-area textview text { background: transparent; }
button.candidate { padding: 8px 12px; border-radius: 12px; background: transparent; box-shadow: none; border: none;
  font-weight: normal; transition: background 120ms; }
button.candidate:hover { background: @cd_agent; }
"#;

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("com.pokai.Codync")
        .build();
    app.connect_startup(|_| {
        let display = gtk::gdk::Display::default().expect("no display");
        let base = gtk::CssProvider::new();
        base.load_from_string(CSS);
        let palette = gtk::CssProvider::new();
        let style = adw::StyleManager::default();
        // Black and white like the Apple apps: follow the system, default dark.
        style.set_color_scheme(adw::ColorScheme::PreferDark);
        let apply = {
            let palette = palette.clone();
            move |s: &adw::StyleManager| {
                palette.load_from_string(&colors(if s.is_dark() { &DARK } else { &LIGHT }));
            }
        };
        apply(&style);
        style.connect_dark_notify(apply);
        // The palette's named colors must exist before the rules that use them.
        gtk::style_context_add_provider_for_display(
            &display,
            &palette,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        gtk::style_context_add_provider_for_display(
            &display,
            &base,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
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
