//! A local log of the security-relevant things that were decided on this computer: who paired, which device was
//! revoked, which cloud, network or approval setting changed. The newest 500 stay for 90 days; nothing carries content.

use crate::hub::Hub;
use anyhow::Result;
use serde_json::{Value, json};

const KV: &str = "audit.log";
const MAX: usize = 500;
const KEEP_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// What a recorded method is called, when it is one of them.
pub fn action(method: &str) -> Option<&'static str> {
    Some(match method {
        "pair" => "A device paired",
        "decideAccessRequest" => "An access request was decided",
        "revokeDevice" => "A device was revoked",
        "decidePairing" => "A pairing request was decided",
        "setCloud" | "setCloudUrl" => "The cloud connection changed",
        "networkJoin" => "This computer joined a network",
        "networkLeave" => "This computer left a network",
        "networkInvite" => "A computer was invited to the network",
        "networkRemove" => "A computer was removed from the network",
        "networkPromote" => "A recovery key was made",
        "setNetworkRules" => "Who may write to the bots changed",
        "setApproval" => "The approval setting changed",
        "setAssistants" => "Cloud assistants were let in or out",
        _ => return None,
    })
}

/// A safe line about the call: the ids it names, never text.
pub fn detail(method: &str, body: &Value) -> String {
    let at = |key: &str| body[key].as_str().unwrap_or_default().to_owned();
    match method {
        "setApproval" => body["approval"].as_str().or_else(|| body["mode"].as_str()).unwrap_or_default().to_owned(),
        "setAssistants" => {
            if body["on"] == true || body["enabled"] == true {
                "on".into()
            } else {
                "off".into()
            }
        }
        "decideAccessRequest" | "decidePairing" => {
            let allowed = body["allowed"].as_bool().unwrap_or(false);
            format!("{} {}", if allowed { "allowed" } else { "refused" }, at("key"))
        }
        "setCloud" | "setCloudUrl" => at("url"),
        "revokeDevice" => at("key"),
        _ => {
            [at("peer"), at("computer"), at("key"), at("name")].into_iter().find(|v| !v.is_empty()).unwrap_or_default()
        }
    }
    .chars()
    .take(120)
    .collect()
}

/// Adds one line; a body with nothing safe to say still records the action.
pub fn record(hub: &Hub, method: &str, body: &Value) {
    let Some(action) = action(method) else { return };
    if let Err(error) = append(hub, action, &detail(method, body)) {
        tracing::warn!(error = format!("{error:#}"), "audit line was not saved");
    }
}

fn read(hub: &Hub) -> Vec<Value> {
    hub.store.kv_read(KV).ok().flatten().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

fn append(hub: &Hub, action: &str, detail: &str) -> Result<()> {
    let mut lines = read(hub);
    let now = crate::store::now_ms();
    lines.retain(|line| line["at"].as_i64().is_some_and(|at| now - at <= KEEP_MS));
    lines.push(json!({"at": now, "action": action, "detail": detail}));
    if lines.len() > MAX {
        let drop = lines.len() - MAX;
        lines.drain(0..drop);
    }
    hub.store.kv_set(KV, &serde_json::to_string(&lines)?)
}

/// The log, newest first.
pub fn list(hub: &Hub) -> Value {
    let mut lines = read(hub);
    lines.reverse();
    json!({"lines": lines})
}

pub fn clear(hub: &Hub) -> Result<()> {
    hub.store.kv_delete(KV)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_security_methods_are_recorded() {
        assert_eq!(action("pair"), Some("A device paired"));
        assert_eq!(action("revokeDevice"), Some("A device was revoked"));
        assert_eq!(action("send"), None);
        assert_eq!(action("getBot"), None);
    }

    #[test]
    fn a_detail_names_ids_but_no_text() {
        assert_eq!(detail("decideAccessRequest", &json!({"key": "abc", "allowed": true})), "allowed abc");
        assert_eq!(detail("setApproval", &json!({"approval": "auto", "mode": "ask"})), "auto");
        assert_eq!(detail("networkRemove", &json!({"peer": "n2a_x"})), "n2a_x");
        assert_eq!(detail("setAssistants", &json!({"enabled": false})), "off");
    }
}
