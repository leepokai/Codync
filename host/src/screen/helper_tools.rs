//! Linux desktops the computer-use driver doesn't support (GNOME, KDE and other Wayland
//! sessions): bots act through Codync Screen's portal input on screenshot pixels instead.

use super::helper::Link;
use super::protocol::Display;
use super::{SETTLE, Screen};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// Long edge of screenshots handed to agents: sharp enough to read UI text,
/// small enough to keep every turn cheap.
const SHOT_MAX_EDGE: f64 = 1280.0;

/// A tool call, as the MCP server forwards it (`{name, arguments}`).
#[derive(Debug, Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
enum Tool {
    Screenshot {
        display: Option<u32>,
    },
    Click {
        x: f64,
        y: f64,
        #[serde(default)]
        button: Button,
        count: Option<u8>,
        #[serde(default)]
        modifiers: Vec<String>,
        display: Option<u32>,
    },
    Move {
        x: f64,
        y: f64,
        display: Option<u32>,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
        display: Option<u32>,
    },
    Scroll {
        x: f64,
        y: f64,
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
        display: Option<u32>,
    },
    Type {
        text: String,
    },
    Key {
        keys: String,
    },
    UiTree {
        display: Option<u32>,
    },
    OpenApp {
        name: String,
    },
}

pub(super) fn read_only(name: &str) -> bool {
    matches!(name, "screenshot" | "ui_tree")
}

pub(super) fn tools() -> Vec<Value> {
    let display = json!({"type": "integer", "description": "Display id; defaults to the main display."});
    let xy = |what: &str| json!({"type": "number", "description": format!("{what} in screenshot pixels.")});
    let tools = json!([
        {
            "name": "screenshot",
            "description": "Capture the screen. Returns a JPEG whose pixels are the coordinate space for every other tool.",
            "inputSchema": {"type": "object", "properties": {"display": display}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "ui_tree",
            "description": "Accessibility tree of the frontmost app: roles, titles, values and frames in screenshot pixels. Cheaper and more precise than reading pixels when you need to find a control.",
            "inputSchema": {"type": "object", "properties": {"display": display}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "click",
            "description": "Click at (x, y) in screenshot pixels. Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": xy("X"), "y": xy("Y"),
                    "button": {"type": "string", "enum": ["left", "right", "middle"]},
                    "count": {"type": "integer", "description": "2 for a double click."},
                    "modifiers": {"type": "array", "items": {"type": "string", "enum": ["cmd", "option", "ctrl", "shift"]}},
                    "display": display,
                },
                "required": ["x", "y"],
            },
        },
        {
            "name": "move",
            "description": "Move the pointer (for hover menus and tooltips). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"x": xy("X"), "y": xy("Y"), "display": display}, "required": ["x", "y"]},
        },
        {
            "name": "drag",
            "description": "Press at (x, y), drag to (to_x, to_y), release. Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {"x": xy("Start x"), "y": xy("Start y"), "to_x": xy("End x"), "to_y": xy("End y"), "display": display},
                "required": ["x", "y", "to_x", "to_y"],
            },
        },
        {
            "name": "scroll",
            "description": "Scroll with the pointer over (x, y). Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": xy("X"), "y": xy("Y"),
                    "dx": {"type": "number", "description": "Lines to scroll right (negative = left)."},
                    "dy": {"type": "number", "description": "Lines to scroll down (negative = up)."},
                    "display": display,
                },
                "required": ["x", "y"],
            },
        },
        {
            "name": "type",
            "description": "Type text into the focused field (any Unicode). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]},
        },
        {
            "name": "key",
            "description": "Press a key or shortcut, e.g. `return`, `escape`, `tab`, `ctrl+t`, `ctrl+shift+t`. `cmd` is Super. Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]},
        },
        {
            "name": "open_app",
            "description": "Open or bring an application to the front by name (e.g. `firefox`). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]},
        },
    ]);
    match tools {
        Value::Array(t) => t,
        _ => unreachable!("a JSON array literal"),
    }
}

/// Runs one tool: `{content}` for the agent.
pub(super) async fn run(screen: &Screen, name: &str, args: Value) -> Result<Value> {
    let tool: Tool = serde_json::from_value(json!({"name": name, "arguments": args}))
        .map_err(|e| anyhow!("invalid {name} call: {e}"))?;
    let link = screen.link()?;
    let display = |id| screen.display(id);
    let content = match tool {
        Tool::Screenshot { display: id } => shot(&link, &display(id)?).await?,
        Tool::Click { x, y, button, count, modifiers, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            let modifiers = parse_modifiers(&modifiers)?;
            let count = count.unwrap_or(1).clamp(1, 3);
            act(&link, &d, InputEvent::Click { x, y, button, count, modifiers }).await?
        }
        Tool::Move { x, y, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            act(&link, &d, InputEvent::Move { x, y }).await?
        }
        Tool::Drag { x, y, to_x, to_y, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            let (to_x, to_y) = d.to_points(to_x, to_y)?;
            act(&link, &d, InputEvent::Drag { x, y, to_x, to_y }).await?
        }
        Tool::Scroll { x, y, dx, dy, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            act(&link, &d, InputEvent::Scroll { x, y, dx, dy }).await?
        }
        Tool::Type { text } => act(&link, &display(None)?, InputEvent::Text { text }).await?,
        Tool::Key { keys } => {
            let (key, modifiers) = parse_keys(&keys)?;
            act(&link, &display(None)?, InputEvent::Key { key, modifiers }).await?
        }
        Tool::UiTree { display: id } => {
            let d = display(id)?;
            let mut res = link.request("uiTree", json!({"display": d.id})).await?;
            scale_frames(&mut res["tree"], d.shot_scale());
            let text = format!(
                "Accessibility tree of {} (frames are [x, y, width, height] in screenshot pixels):\n{}",
                res["app"].as_str().unwrap_or("the frontmost app"),
                res["tree"]
            );
            json!([{"type": "text", "text": text}])
        }
        Tool::OpenApp { name } => {
            link.request("openApp", json!({"name": name})).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            shot(&link, &display(None)?).await?
        }
    };
    Ok(json!({"content": content}))
}

/// Types into the focused field (`type_login`): `{content}` with a screenshot.
pub(super) async fn type_text(screen: &Screen, text: String) -> Result<Value> {
    let link = screen.link()?;
    Ok(json!({"content": act(&link, &screen.display(None)?, InputEvent::Text { text }).await?}))
}

async fn act(link: &Link, d: &Display, event: InputEvent) -> Result<Value> {
    link.request("input", json!({"display": d.id, "event": event})).await?;
    tokio::time::sleep(SETTLE).await;
    shot(link, d).await
}

async fn shot(link: &Link, d: &Display) -> Result<Value> {
    let (width, height) = d.shot_size();
    let res = link.request("screenshot", json!({"display": d.id, "width": width, "height": height})).await?;
    let data = res["data"].as_str().ok_or_else(|| anyhow!("the screen helper sent no image"))?;
    Ok(json!([
        {"type": "image", "data": data, "mimeType": "image/jpeg"},
        {"type": "text", "text": format!(
            "Display {} \"{}\", {width}×{height}. Coordinates for click/move/drag/scroll are pixels in this image.",
            d.id, d.name
        )},
    ]))
}

impl Display {
    /// Screenshot scale: points → agent image pixels.
    fn shot_scale(&self) -> f64 {
        (SHOT_MAX_EDGE / self.width.max(self.height)).min(1.0)
    }

    /// Agent image size for this display.
    fn shot_size(&self) -> (u32, u32) {
        let s = self.shot_scale();
        // Display sizes are a few thousand points: always in range.
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        ((self.width * s).round() as u32, (self.height * s).round() as u32)
    }

    /// Agent image pixels → display points, rejecting points off the image.
    fn to_points(&self, x: f64, y: f64) -> Result<(f64, f64)> {
        let (width, height) = self.shot_size();
        if !(0.0..=f64::from(width)).contains(&x) || !(0.0..=f64::from(height)).contains(&y) {
            bail!("({x}, {y}) is outside the {width}×{height} screenshot");
        }
        // Per axis: the image size is rounded, so its aspect differs slightly from the display's.
        Ok((x * self.width / f64::from(width), y * self.height / f64::from(height)))
    }
}

/// Display points → screenshot pixels, for every `frame` in an accessibility tree.
fn scale_frames(node: &mut Value, s: f64) {
    if let Some(frame) = node.get_mut("frame").and_then(Value::as_array_mut) {
        for v in frame.iter_mut() {
            if let Some(f) = v.as_f64() {
                *v = json!((f * s).round());
            }
        }
    }
    if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
        for c in children {
            scale_frames(c, s);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Button {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Modifier {
    /// Super.
    Cmd,
    Option,
    Ctrl,
    Shift,
}

/// One input action for the helper, in display points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum InputEvent {
    Move {
        x: f64,
        y: f64,
    },
    Click {
        x: f64,
        y: f64,
        button: Button,
        count: u8,
        modifiers: Vec<Modifier>,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
    },
    /// Scroll by `dx`/`dy` lines (positive = right/down) with the pointer at `x`,`y`.
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Text {
        text: String,
    },
    Key {
        key: String,
        modifiers: Vec<Modifier>,
    },
}

/// Named keys the helper understands; any other key is a single character.
const NAMED_KEYS: &[&str] = &[
    "return",
    "tab",
    "space",
    "escape",
    "delete",
    "forwardDelete",
    "left",
    "right",
    "up",
    "down",
    "home",
    "end",
    "pageUp",
    "pageDown",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// Modifier names with common aliases, deduplicated.
fn parse_modifiers(names: &[impl AsRef<str>]) -> Result<Vec<Modifier>> {
    let mut out = vec![];
    for m in names {
        let m = match m.as_ref().to_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" | "win" => Modifier::Cmd,
            "option" | "opt" | "alt" => Modifier::Option,
            "ctrl" | "control" => Modifier::Ctrl,
            "shift" => Modifier::Shift,
            other => bail!("unknown modifier `{other}` (use cmd, option, ctrl, shift)"),
        };
        if !out.contains(&m) {
            out.push(m);
        }
    }
    Ok(out)
}

/// `"ctrl+shift+t"` → key `t` with ⌃⇧. Accepts common aliases (`ctrl`, `alt`, `enter`, `esc`, …).
fn parse_keys(combo: &str) -> Result<(String, Vec<Modifier>)> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (key, mods) =
        parts.split_last().filter(|(k, _)| !k.is_empty()).ok_or_else(|| anyhow!("no key in `{combo}`"))?;
    let modifiers = parse_modifiers(mods)?;
    let lower = key.to_lowercase();
    let key = match lower.as_str() {
        "enter" => "return".to_owned(),
        "esc" => "escape".to_owned(),
        "backspace" => "delete".to_owned(),
        "del" | "forwarddelete" => "forwardDelete".to_owned(),
        "pageup" | "pgup" => "pageUp".to_owned(),
        "pagedown" | "pgdn" => "pageDown".to_owned(),
        "arrowleft" => "left".to_owned(),
        "arrowright" => "right".to_owned(),
        "arrowup" => "up".to_owned(),
        "arrowdown" => "down".to_owned(),
        _ if NAMED_KEYS.contains(&lower.as_str()) => lower,
        _ if key.chars().count() == 1 => lower,
        _ => bail!("unknown key `{key}`"),
    };
    Ok((key, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::tests::retina;

    #[test]
    fn tools_parse_from_mcp_calls() {
        let t: Tool =
            serde_json::from_value(json!({"name": "click", "arguments": {"x": 1, "y": 2, "button": "right"}})).unwrap();
        assert!(matches!(t, Tool::Click { button: Button::Right, count: None, .. }));
        assert!(read_only("screenshot") && !read_only("click"));
        assert!(tools().iter().all(|t| serde_json::from_value::<Tool>(json!({"name": t["name"], "arguments": {"x": 1, "y": 1, "to_x": 1, "to_y": 1, "text": "", "keys": "a", "name": "x"}})).is_ok()));
    }

    #[test]
    fn screenshot_coordinates_map_to_points() {
        let d = retina();
        assert_eq!(d.shot_size(), (1280, 831));
        assert_eq!(d.to_points(1280.0, 831.0).unwrap(), (1512.0, 982.0));
        let (x, y) = d.to_points(640.0, 415.5).unwrap();
        assert!((x - 756.0).abs() < 0.01 && (y - 491.0).abs() < 0.01);
        assert!(d.to_points(1281.0, 10.0).is_err());
        // Small displays aren't upscaled.
        let small = Display { width: 800.0, height: 600.0, ..retina() };
        assert_eq!(small.shot_size(), (800, 600));
    }

    #[test]
    fn key_combos_parse() {
        assert_eq!(parse_keys("cmd+shift+T").unwrap(), ("t".into(), vec![Modifier::Cmd, Modifier::Shift]));
        assert_eq!(parse_keys("Enter").unwrap(), ("return".into(), vec![]));
        assert_eq!(parse_keys("ctrl+alt+Delete").unwrap(), ("delete".into(), vec![Modifier::Ctrl, Modifier::Option]));
        assert_eq!(parse_keys("cmd++").unwrap_err().to_string(), "no key in `cmd++`");
        assert!(parse_keys("hyper+x").is_err());
        assert!(parse_keys("cmd+banana").is_err());
    }

    #[test]
    fn input_events_serialize_for_helpers() {
        let e = InputEvent::Drag { x: 1.0, y: 2.0, to_x: 3.0, to_y: 4.0 };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"type": "drag", "x": 1.0, "y": 2.0, "toX": 3.0, "toY": 4.0})
        );
    }

    #[test]
    fn frames_scale_recursively() {
        let mut t = json!({"frame": [100, 50, 200, 20], "children": [{"frame": [10.0, 10.0, 5.0, 5.0]}]});
        scale_frames(&mut t, 0.5);
        assert_eq!(t["frame"], json!([50.0, 25.0, 100.0, 10.0]));
        assert_eq!(t["children"][0]["frame"], json!([5.0, 5.0, 3.0, 3.0]));
    }
}
