#!/usr/bin/env python3
"""Regenerates the Android stand-ins for the SF Symbols the iOS app names.

SF Symbols may only ship on Apple platforms, so Android draws Material Symbols Rounded
(Apache-2.0) at weight 500, the closest match to SF's medium weight. Each SF name maps to
one Material symbol; `.fill` names take the filled variant. Output:
  design/src/main/res/drawable/sf_*.xml   vector drawables
  design/src/main/kotlin/.../Symbols.kt   `symbolRes("chevron.down")` lookup

Run from apps/android: python3 tools/symbols.py
"""
import pathlib
import re
import urllib.request

# SF Symbol name -> Material Symbols name ("+" suffix: filled variant).
MAP = {
    "app": "apps",
    "apple.logo": None,  # drawn from the bundled brand mark, not a symbol
    "arrow.clockwise": "refresh",
    "arrow.counterclockwise": "replay",
    "arrow.down": "arrow_downward",
    "arrow.down.circle": "arrow_circle_down",
    "arrow.down.circle.fill": "arrow_circle_down+",
    "arrow.down.doc.on.clipboard": "content_paste_go",
    "arrow.down.right.and.arrow.up.left": "close_fullscreen",
    "arrow.down.to.line": "vertical_align_bottom",
    "arrow.left": "arrow_back",
    "arrow.left.arrow.right": "swap_horiz",
    "arrow.right": "arrow_forward",
    "arrow.right.doc.on.clipboard": "content_paste_go",
    "arrow.right.to.line": "keyboard_tab",
    "arrow.triangle.2.circlepath": "sync",
    "arrow.triangle.branch": "call_split",
    "arrow.turn.down.right": "subdirectory_arrow_right",
    "arrow.up": "arrow_upward",
    "arrow.up.doc.on.clipboard": "content_paste_go",
    "arrow.up.right": "arrow_outward",
    "arrowshape.turn.up.left": "reply",
    "battery.100percent": "battery_full_alt",
    "battery.75percent": "battery_horiz_075",
    "bell": "notifications",
    "bolt": "bolt",
    "book.fill": "book+",
    "book.pages": "menu_book",
    "books.vertical": "library_books",
    "brain": "psychology",
    "bubble.left.and.bubble.right": "forum",
    "bubble.left.and.bubble.right.fill": "forum+",
    "calendar": "calendar_today",
    "camera.fill": "photo_camera+",
    "chart.bar": "bar_chart",
    "chart.bar.doc.horizontal": "lab_profile",
    "checklist": "checklist",
    "checkmark": "check",
    "checkmark.circle": "check_circle",
    "checkmark.circle.fill": "check_circle+",
    "checkmark.message": "mark_chat_read",
    "checkmark.seal": "verified",
    "chevron.down": "keyboard_arrow_down",
    "chevron.left": "chevron_left",
    "chevron.right": "chevron_right",
    "chevron.up": "keyboard_arrow_up",
    "circle": "radio_button_unchecked",
    "circle.dotted": "motion_photos_on",
    "clock": "schedule",
    "clock.arrow.circlepath": "history",
    "clock.fill": "schedule+",
    "cloud": "cloud",
    "cloud.sun.fill": "partly_cloudy_day+",
    "command": "keyboard_command_key",
    "control": "keyboard_control_key",
    "crop": "crop",
    "cursorarrow.motionlines": "arrow_selector_tool",
    "desktopcomputer": "desktop_mac",
    "desktopcomputer.trianglebadge.exclamationmark": "desktop_access_disabled",
    "display": "monitor",
    "display.2": "desktop_windows",
    "doc": "draft",
    "doc.badge.plus": "note_add",
    "doc.on.clipboard": "content_paste",
    "doc.on.clipboard.fill": "content_paste+",
    "doc.on.doc": "content_copy",
    "doc.richtext": "article",
    "doc.text": "description",
    "doc.zipper": "folder_zip",
    "ellipsis": "more_horiz",
    "ellipsis.bubble.fill": "sms+",
    "envelope.fill": "mail+",
    "escape": "keyboard_return",
    "exclamationmark": "priority_high",
    "exclamationmark.circle": "error",
    "exclamationmark.circle.fill": "error+",
    "exclamationmark.triangle": "warning",
    "exclamationmark.triangle.fill": "warning+",
    "eye": "visibility",
    "eye.slash": "visibility_off",
    "figure.run": "directions_run",
    "film": "movie",
    "folder": "folder",
    "function": "function",
    "gearshape": "settings",
    "gearshape.fill": "settings+",
    "globe": "language",
    "hand.point.up.left": "touch_app",
    "hand.raised": "front_hand",
    "hand.raised.fill": "front_hand+",
    "hand.raised.slash": "do_not_touch",
    "heart.fill": "favorite+",
    "info.circle": "info",
    "italic": "format_italic",
    "key": "key",
    "key.fill": "key+",
    "keyboard": "keyboard",
    "keyboard.chevron.compact.down": "keyboard_hide",
    "laptopcomputer": "laptop_mac",
    "list.bullet": "format_list_bulleted",
    "list.bullet.rectangle": "list_alt",
    "lock.display": "screen_lock_landscape",
    "lock.fill": "lock+",
    "lock.rectangle": "password",
    "lock.shield": "shield_lock",
    "lock.slash": "lock_open",
    "macmini": "hard_drive",
    "macpro.gen3": "host",
    "macstudio": "hard_drive_2",
    "magnifyingglass": "search",
    "map.fill": "map+",
    "mic": "mic",
    "mic.fill": "mic+",
    "mic.slash": "mic_off",
    "moon.zzz.fill": "bedtime+",
    "move": "open_with",
    "music.note": "music_note",
    "network": "public",
    "note.text": "sticky_note_2",
    "option": "keyboard_option_key",
    "paintbrush": "brush",
    "paintpalette": "palette",
    "pause.fill": "pause+",
    "pencil": "edit",
    "person.2": "group",
    "person.crop.circle": "account_circle",
    "person.crop.circle.badge.checkmark": "how_to_reg",
    "person.fill": "person+",
    "phone.fill": "call+",
    "photo": "image",
    "photo.fill": "image+",
    "photo.on.rectangle": "photo_library",
    "pin": "keep",
    "pin.fill": "keep+",
    "pin.slash": "keep_off",
    "play.circle": "play_circle",
    "play.circle.fill": "play_circle+",
    "play.fill": "play_arrow+",
    "plus": "add",
    "plus.circle": "add_circle",
    "plus.rectangle.on.rectangle": "library_add",
    "point.3.connected.trianglepath.dotted": "hub",
    "power": "power_settings_new",
    "qrcode": "qr_code_2",
    "qrcode.viewfinder": "qr_code_scanner",
    "rectangle.and.hand.point.up.left": "touch_app",
    "rectangle.portrait.and.arrow.right": "logout",
    "rectangle.stack.fill": "stacks+",
    "return": "keyboard_return",
    "rotate.right": "rotate_right",
    "safari": "explore",
    "shift": "shift",
    "shippingbox": "package_2",
    "square.and.arrow.down": "download",
    "square.and.arrow.up": "ios_share",
    "square.and.pencil": "edit_square",
    "square.grid.2x2": "grid_view",
    "square.grid.3x3": "apps",
    "square.on.square": "content_copy",
    "star.fill": "star+",
    "stop.fill": "stop+",
    "tab": "keyboard_tab",
    "terminal": "terminal",
    "text.bubble": "chat",
    "trash": "delete",
    "waveform": "graphic_eq",
    "wifi": "wifi",
    "wifi.exclamationmark": "signal_wifi_bad",
    "wifi.slash": "wifi_off",
    "wrench.and.screwdriver": "build",
    "xmark": "close",
    "xmark.circle": "cancel",
    "xmark.circle.fill": "cancel+",
}

ROOT = pathlib.Path(__file__).resolve().parent.parent
DRAWABLES = ROOT / "design/src/main/res/drawable"
KOTLIN = ROOT / "design/src/main/kotlin/com/codync/android/design/Symbols.kt"
BASE = "https://raw.githubusercontent.com/google/material-design-icons/master/symbols/web"
# Symbols iOS draws at regular weight (the toolbar's plus).
REGULAR = {"add"}


def url(base: str, filled: bool) -> str:
    variant = ("" if base in REGULAR else "wght500") + ("fill1" if filled else "")
    return f"{BASE}/{base}/materialsymbolsrounded/{base}{'_' + variant if variant else ''}_24px.svg"


def resource(name: str) -> str:
    return "sf_" + re.sub(r"[^a-z0-9]", "_", name)


def fetch(symbol: str) -> str:
    filled = symbol.endswith("+")
    base = symbol.rstrip("+")
    with urllib.request.urlopen(url(base, filled)) as response:
        svg = response.read().decode()
    paths = re.findall(r'<path d="([^"]+)"', svg)
    if not paths:
        raise SystemExit(f"no path in {base}")
    body = "".join(f'        <path android:fillColor="#FF000000" android:pathData="{d}"/>\n' for d in paths)
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f"<!-- Material Symbols Rounded `{base}`{' (filled)' if filled else ''}, Generated by tools/symbols.py. -->\n"
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
        '    android:width="24dp" android:height="24dp" android:viewportWidth="960" android:viewportHeight="960">\n'
        '    <group android:translateY="960">\n'
        f"{body}"
        "    </group>\n"
        "</vector>\n"
    )


def main() -> None:
    DRAWABLES.mkdir(parents=True, exist_ok=True)
    for old in DRAWABLES.glob("sf_*.xml"):
        old.unlink()
    cache: dict[str, str] = {}
    for name, symbol in sorted(MAP.items()):
        if symbol is None:
            continue
        if symbol not in cache:
            cache[symbol] = fetch(symbol)
        (DRAWABLES / f"{resource(name)}.xml").write_text(cache[symbol])
    cases = "\n".join(f'    "{n}" -> R.drawable.{resource(n)}' for n, s in sorted(MAP.items()) if s is not None)
    KOTLIN.write_text(
        "// Generated by tools/symbols.py: the Material Symbols standing in for SF Symbol names.\n"
        "package com.codync.android.design\n\n"
        "import androidx.annotation.DrawableRes\n\n"
        "/** The drawable for an SF Symbol name the iOS app uses, or null when Android has none. */\n"
        "@DrawableRes fun symbolRes(name: String): Int? = when (name) {\n"
        f"{cases}\n"
        "    else -> null\n"
        "}\n"
    )
    print(f"{len(cache)} symbols, {len(MAP)} names")


if __name__ == "__main__":
    main()
