//! The computer's ComfyUI in the terminal: one phrase in the header while it starts, runs, makes an image or
//! failed, from `toolStatus`; nothing while it is off. Same words as the desktop's card and the phone's section.

use crate::tui::l10n::tr;
use serde_json::Value;

/// How often the header looks at ComfyUI again: a job moves along in seconds.
pub const REFRESH: std::time::Duration = std::time::Duration::from_secs(5);

/// The header's phrase, or nothing for a stopped ComfyUI and an older host.
pub fn header(status: &Value) -> Option<String> {
    let phrase = match (status["state"].as_str()?, status["job"]["phase"].as_str()) {
        ("running", Some("running")) => tr("Making an image…"),
        ("running", Some("queued")) => tr("Waiting in the queue"),
        ("running", _) => tr("Running"),
        ("starting", _) => tr("Starting…"),
        ("error", _) => tr("Stopped with an error"),
        _ => return None,
    };
    Some(format!("ComfyUI · {phrase}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_header_says_what_comfyui_does_and_nothing_when_it_is_off() {
        assert_eq!(header(&Value::Null), None, "an older host says nothing");
        assert_eq!(header(&json!({"state": "stopped", "job": null})), None);
        assert_eq!(header(&json!({"state": "starting"})), Some("ComfyUI · Starting…".into()));
        assert_eq!(header(&json!({"state": "running", "job": null})), Some("ComfyUI · Running".into()));
        assert_eq!(
            header(&json!({"state": "running", "job": {"phase": "running"}})),
            Some("ComfyUI · Making an image…".into())
        );
        assert_eq!(
            header(&json!({"state": "running", "job": {"phase": "queued"}})),
            Some("ComfyUI · Waiting in the queue".into())
        );
        assert_eq!(
            header(&json!({"state": "running", "job": {"phase": "done"}})),
            Some("ComfyUI · Running".into()),
            "a finished job leaves the header"
        );
        assert_eq!(header(&json!({"state": "error"})), Some("ComfyUI · Stopped with an error".into()));
    }
}
