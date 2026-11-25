//! Fast mode in a session's config options (ACP `configOptions`).

use serde_json::{Value, json};

/// Fast mode's option IDs: Claude's adapter (`fast`) and Codex's (`fast-mode`).
const FAST_MODE_IDS: [&str; 2] = ["fast", "fast-mode"];

/// The Fast mode option. Agents offer it only while the session's model supports it.
pub fn fast_mode_config(session: &Value) -> Option<&Value> {
    session["configOptions"]
        .as_array()?
        .iter()
        .find(|v| v["id"].as_str().is_some_and(|id| FAST_MODE_IDS.contains(&id)) && is_switch(v))
}

/// A boolean, or a select listing `on` and `off`, as Claude and Codex offer it. Cursor's `fast`
/// (`true` / `false`) is one of its model's options instead, set from the bot's `modelOptions`.
fn is_switch(option: &Value) -> bool {
    let lists = |value: &str| option["options"].as_array().is_some_and(|o| o.iter().any(|v| v["value"] == value));
    option["type"] == "boolean" || (option["type"] == "select" && lists("on") && lists("off"))
}

/// Whether the Fast mode option is on.
pub fn fast_mode_on(config: &Value) -> bool {
    config["currentValue"] == true || config["currentValue"] == "on"
}

/// Why the agent turned Fast mode off by itself, when it says: Claude's adapter appends it to
/// the option's description ("Faster responses on supported models — requires extra usage…").
pub fn fast_mode_refusal(config: &Value) -> Option<String> {
    let description = config["description"].as_str()?;
    let (_, why) = description.split_once(" — ")?;
    Some(why.trim()).filter(|why| !why.is_empty()).map(str::to_owned)
}

/// What a bot's agent says about Fast mode (wire `fastModeSupport` and `fastModeIssue`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FastModeSupport {
    /// No session has listed its options yet.
    #[default]
    Unknown,
    Offered,
    /// The session's model doesn't offer it.
    Unsupported,
    /// The agent turned it off after the bot asked for it (Claude: the account can't use it),
    /// with its reason when it gave one. Asked again only once the user changes the setting.
    Refused(Option<String>),
}

impl FastModeSupport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Offered => "offered",
            Self::Unsupported => "unsupported",
            Self::Refused(_) => "refused",
        }
    }

    pub fn issue(&self) -> Option<&str> {
        match self {
            Self::Refused(why) => why.as_deref(),
            _ => None,
        }
    }

    /// Kept across host restarts (`fast-mode.<bot>`), so clients can offer the switch before
    /// the first turn. A refusal isn't kept: the next host asks again.
    pub fn stored(&self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Offered | Self::Refused(_) => Some("offered"),
            Self::Unsupported => Some("unsupported"),
        }
    }

    pub fn from_stored(v: Option<&str>) -> Self {
        match v {
            Some("offered") => Self::Offered,
            Some("unsupported") => Self::Unsupported,
            _ => Self::Unknown,
        }
    }
}

/// The value that turns `config` on or off: a boolean, or the `on` / `off` select entries.
/// `None` when it already is.
pub fn fast_mode_change(config: &Value, on: bool) -> Option<Value> {
    let value = if config["type"] == "boolean" { json!(on) } else { json!(if on { "on" } else { "off" }) };
    (config["currentValue"] != value).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_mode_is_claudes_and_codexs_on_off_option() {
        let on_off = json!([{"value": "on"}, {"value": "off"}]);
        for id in ["fast", "fast-mode"] {
            let session = json!({"configOptions": [
                {"id": "model", "category": "model", "type": "select", "options": []},
                {"id": id, "category": "model_config", "type": "select", "currentValue": "off", "options": on_off}
            ]});
            assert_eq!(fast_mode_config(&session).unwrap()["id"], id);
        }
        let boolean = json!({"configOptions": [{"id": "fast", "type": "boolean", "currentValue": false}]});
        assert!(fast_mode_config(&boolean).is_some());
        let other = json!({"configOptions": [{"id": "effort", "category": "thought_level", "type": "select"}]});
        assert!(fast_mode_config(&other).is_none());
        // Cursor's `fast` is a model option the bot sets through `modelOptions`.
        let cursor = json!({"configOptions": [{"id": "fast", "type": "select", "currentValue": "true",
            "options": [{"value": "false"}, {"value": "true"}]}]});
        assert!(fast_mode_config(&cursor).is_none());
    }

    #[test]
    fn fast_mode_refusal_reads_the_reason_from_the_description() {
        let refused = json!({"id": "fast", "currentValue": "off",
            "description": "Faster responses on supported models — requires extra usage to be enabled for this account"});
        assert_eq!(fast_mode_refusal(&refused).as_deref(), Some("requires extra usage to be enabled for this account"));
        assert!(!fast_mode_on(&refused));
        assert!(fast_mode_refusal(&json!({"description": "1.5x speed, increased usage"})).is_none());
        assert!(fast_mode_on(&json!({"currentValue": true})));
    }

    #[test]
    fn fast_mode_support_keeps_only_what_the_agent_offers_across_restarts() {
        let refused = FastModeSupport::Refused(Some("no extra usage".into()));
        assert_eq!(refused.as_str(), "refused");
        assert_eq!(refused.issue(), Some("no extra usage"));
        assert_eq!(FastModeSupport::from_stored(refused.stored()), FastModeSupport::Offered);
        assert_eq!(FastModeSupport::from_stored(FastModeSupport::Unknown.stored()), FastModeSupport::Unknown);
    }

    #[test]
    fn fast_mode_change_sends_only_a_different_value() {
        let select = json!({"id": "fast", "type": "select", "currentValue": "off"});
        assert_eq!(fast_mode_change(&select, true), Some(json!("on")));
        assert_eq!(fast_mode_change(&select, false), None);
        let boolean = json!({"id": "fast-mode", "type": "boolean", "currentValue": true});
        assert_eq!(fast_mode_change(&boolean, false), Some(json!(false)));
        assert_eq!(fast_mode_change(&boolean, true), None);
    }
}
