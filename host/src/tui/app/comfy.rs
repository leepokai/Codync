//! The computer's ComfyUI in the terminal: one phrase in the header while its program downloads, while it
//! starts, runs, makes an image or failed, from `toolStatus`; nothing while it is off. Same words as the
//! desktop's card and the phone's section.

use crate::tui::l10n::tr;
use serde_json::Value;

/// How often the header looks at ComfyUI again: a job moves along in seconds.
pub const REFRESH: std::time::Duration = std::time::Duration::from_secs(5);

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

/// `1.5 GB`, `320 MB`: the desktop's and the phone's sizes.
fn size(bytes: u64) -> String {
    if bytes >= GB { format!("{}.{} GB", bytes / GB, bytes % GB * 10 / GB) } else { format!("{} MB", bytes / MB) }
}

/// The header's phrase, or nothing for a stopped ComfyUI and an older host.
pub fn header(status: &Value) -> Option<String> {
    let install = &status["install"];
    let phrase = match (install["phase"].as_str(), status["state"].as_str()?, status["job"]["phase"].as_str()) {
        (Some("downloading"), ..) => tr("Downloading {name}… {done} of {total}")
            .replace("{name}", "ComfyUI")
            .replace("{done}", &size(install["done"].as_u64().unwrap_or_default()))
            .replace("{total}", &size(install["total"].as_u64().unwrap_or_default()))
            .into(),
        (Some("unpacking"), ..) => tr("Unpacking {name}…").replace("{name}", "ComfyUI").into(),
        (_, "running", Some("running")) => tr("Making an image…"),
        (_, "running", Some("queued")) => tr("Waiting in the queue"),
        (_, "running", _) => tr("Running"),
        (_, "starting", _) => tr("Starting…"),
        (Some("failed"), ..) | (_, "error", _) => tr("Stopped with an error"),
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

    #[test]
    fn the_first_download_shows_how_far_it_got() {
        let downloading = json!({"state": "stopped", "install": {"phase": "downloading", "done": GB, "total": 2 * GB}});
        assert_eq!(header(&downloading), Some("ComfyUI · Downloading ComfyUI… 1.0 GB of 2.0 GB".into()));
        let unpacking = json!({"state": "stopped", "install": {"phase": "unpacking"}});
        assert_eq!(header(&unpacking), Some("ComfyUI · Unpacking ComfyUI…".into()));
        let failed = json!({"state": "stopped", "install": {"phase": "failed", "error": "HTTP 503"}});
        assert_eq!(header(&failed), Some("ComfyUI · Stopped with an error".into()));
        assert_eq!(size(500 * MB), "500 MB");
    }
}
