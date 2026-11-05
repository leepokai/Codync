//! `hermes usage --json`: the account limits Hermes Agent reads for its configured provider, in a
//! document it keeps stable for scripts, with no session. A provider it has no reading for exits
//! non-zero with nothing on stdout.

use crate::hub::Hub;
use serde_json::Value;
use std::time::Duration;

use super::{Window, iso_ms, slug};

pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    super::command::json_windows(hub, "hermes", &["usage", "--json"], Duration::from_secs(30), parse).await
}

/// `None`: neither limits nor a reason why there are none, so the document changed.
fn parse(document: &Value) -> Option<Vec<Window>> {
    match document["windows"].as_array() {
        Some(windows) => Some(windows.iter().filter_map(window).collect()),
        None => document["unavailable_reason"].is_string().then(Vec::new),
    }
}

fn window(value: &Value) -> Option<Window> {
    let label = value["label"].as_str().filter(|label| !label.is_empty())?;
    let id = slug(label);
    let resets = value["resets_at"].as_str().and_then(iso_ms);
    match value["used_percent"].as_f64().filter(|used| used.is_finite()) {
        Some(used) => Some(Window::new(id, label, used.clamp(0.0, 100.0), resets)),
        // Credits with no cap are a note. A window with neither is one the provider didn't report.
        None => Some(Window::note(id, label, value["detail"].as_str().filter(|detail| !detail.is_empty())?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_documented_shape_reads_as_limits_and_notes() {
        let w = parse(&json!({"provider": "openai-codex", "windows": [
            {"label": "Session", "used_percent": 37.0, "resets_at": "2026-09-19T21:00:00+00:00", "detail": null},
            {"label": "Weekly", "used_percent": 12.5, "resets_at": null, "detail": null},
            {"label": "Credits", "used_percent": null, "resets_at": null, "detail": "$4.20 left"},
            {"label": "Unknown", "used_percent": null, "resets_at": null, "detail": null}
        ], "unavailable_reason": null}))
        .unwrap();
        assert_eq!(w.len(), 3, "a window nobody reported is dropped");
        assert_eq!((w[0].id.as_str(), w[0].label.as_str()), ("session", "Session"));
        assert!((w[0].percent - 37.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_789_851_600_000));
        assert_eq!(w[1].resets_at, None);
        assert_eq!(w[2].text.as_deref(), Some("$4.20 left"));
    }

    #[test]
    fn a_reason_is_an_empty_reading_and_anything_else_a_changed_format() {
        assert_eq!(parse(&json!({"windows": [], "unavailable_reason": "nope"})), Some(vec![]));
        assert_eq!(parse(&json!({"unavailable_reason": "nope"})), Some(vec![]));
        assert_eq!(parse(&json!({"limits": []})), None);
    }
}
