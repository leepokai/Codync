//! Permission status in the bot editor. OS grants happen on the computer's desktop.
use serde_json::Value;

pub(super) fn status(screen: &Value, online: bool) -> &'static str {
    if !online {
        return "Computer offline";
    }
    if screen.is_null() {
        return "Checking computer access";
    }
    if screen["platform"] == "windows" {
        return "Control ready; remote viewing unavailable";
    }
    if screen["enabled"] != true {
        return "Computer access is off";
    }
    if screen["connected"] != true {
        return "Start Codync Screen";
    }
    if screen["platform"] == "linux" && (screen["capture"] != true || screen["input"] != true) {
        return "Allow screen sharing and control";
    }
    if screen["capture"] != true {
        return "Allow screen recording";
    }
    if screen["input"] != true {
        return "Allow computer control";
    }
    "Computer access ready"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_status_follows_grants_and_revocation() {
        let mut state = json!({"enabled":true,"connected":true,"capture":false,"input":false});
        assert_eq!(status(&state, false), "Computer offline");
        assert_eq!(status(&state, true), "Allow screen recording");
        state["capture"] = true.into();
        assert_eq!(status(&state, true), "Allow computer control");
        state["input"] = true.into();
        assert_eq!(status(&state, true), "Computer access ready");
        state["platform"] = "windows".into();
        assert_eq!(status(&state, true), "Control ready; remote viewing unavailable");
    }
}
