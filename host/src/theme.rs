//! Palettes from `~/.config/Codync/themes/*.json`, such as the one a terminal theme writes: a few named colors the
//! desktop app wears instead of its own. Files only; a bad file is skipped, never half applied.

use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

const KV_SELECTED: &str = "appearance.palette";
const MAX_FILE: u64 = 64 * 1024;
const MAX_THEMES: usize = 200;

/// The keys a palette may set, and the color each becomes.
const KEYS: &[&str] = &[
    "background",
    "foreground",
    "accent",
    "color0",
    "color1",
    "color2",
    "color3",
    "color4",
    "color5",
    "color6",
    "color7",
    "color8",
    "color9",
    "color10",
    "color11",
    "color12",
    "color13",
    "color14",
    "color15",
];

fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Codync").join("themes"))
}

/// A theme's name must stay a file name.
fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 80
        || name.contains(['/', '\\', ':'])
        || name.starts_with('.')
        || name.chars().any(char::is_control)
    {
        bail!("Invalid theme name.");
    }
    Ok(())
}

fn path(name: &str) -> Result<PathBuf> {
    check_name(name)?;
    let dir = dir().context("No config folder on this computer.")?;
    Ok(dir.join(format!("{name}.json")))
}

fn is_color(value: &str) -> bool {
    matches!(value.len(), 4 | 7 | 9) && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The palette a file holds: only known keys with `#rgb`, `#rrggbb` or `#rrggbbaa` colors.
fn read_file(name: &str) -> Result<Map<String, Value>> {
    let file = path(name)?;
    let size = std::fs::metadata(&file).with_context(|| format!("Unknown theme {name}."))?.len();
    if size > MAX_FILE {
        bail!("That theme file is too large.");
    }
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&file)?).context("Theme is not valid JSON.")?;
    let mut out = Map::new();
    for key in KEYS {
        if let Some(color) = raw[key].as_str().filter(|c| is_color(c)) {
            out.insert((*key).to_owned(), json!(color));
        }
    }
    if out.is_empty() {
        bail!("That theme has no colors.");
    }
    Ok(out)
}

/// The theme files this computer offers, newest name last.
pub fn list() -> Vec<String> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_str()?.to_owned();
            (e.path().extension()? == "json" && check_name(&name).is_ok()).then_some(name)
        })
        .collect();
    names.sort();
    names.truncate(MAX_THEMES);
    names
}

pub fn selected(store: &Store) -> Option<String> {
    store.kv_read(KV_SELECTED).ok().flatten().filter(|n| path(n).is_ok_and(|p| p.is_file()))
}

/// What the app wears now: the palette of the chosen theme, when one is chosen and readable.
pub fn active(store: &Store) -> Value {
    match selected(store).and_then(|name| read_file(&name).ok().map(|palette| (name, palette))) {
        Some((name, palette)) => json!({"name": name, "palette": palette}),
        None => json!({"name": null, "palette": {}}),
    }
}

/// Chooses a theme (`None` goes back to the app's own colors).
pub fn select(store: &Store, name: Option<&str>) -> Result<()> {
    match name.filter(|n| !n.is_empty()) {
        Some(name) => {
            read_file(name)?;
            store.kv_set(KV_SELECTED, name)
        }
        None => store.kv_delete(KV_SELECTED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_color_keys_survive() {
        let raw = json!({"background": "#112233", "foreground": "red", "color4": "#abc", "extra": "#fff"});
        let mut kept = Map::new();
        for key in KEYS {
            if let Some(color) = raw[key].as_str().filter(|c| is_color(c)) {
                kept.insert((*key).to_owned(), json!(color));
            }
        }
        assert_eq!(kept.len(), 2);
        assert!(kept.contains_key("color4"));
        assert!(!kept.contains_key("extra"));
    }

    #[test]
    fn names_stay_file_names() {
        assert!(check_name("gruvbox").is_ok());
        assert!(check_name("my theme").is_ok());
        for bad in ["", "../x", "a/b", ".hidden", "a:b"] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn colors_have_a_hex_shape() {
        for ok in ["#abc", "#aabbcc", "#aabbccdd"] {
            assert!(is_color(ok), "{ok}");
        }
        for bad in ["red", "#ab", "#gggggg", "aabbcc"] {
            assert!(!is_color(bad), "{bad}");
        }
    }
}
