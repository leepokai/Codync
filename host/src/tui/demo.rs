//! Demo mode of the terminal UI: a second `App` on a script instead of the host. Its client never opens a connection
//! ([`Client::demo`](super::net::Client::demo)): a command is answered here, and the events are the ones below. The real app
//! keeps running underneath, so leaving brings back exactly what was there. Nothing here reads or writes a file or reaches
//! the network (`tests::the_script_is_made_of_data_only`).

use serde_json::{Value, json};

/// The computer the demo pretends to be.
pub const COMPUTER: &str = "demo-computer";

/// One bot as the host's `bot` event carries it.
fn bot([id, name, description, shape, color, status, activity]: [&str; 7], now: i64, unread: i64) -> Value {
    json!({"type": "bot", "rev": 1, "bot": {
        "id": id, "name": name, "description": description, "avatarShape": shape, "avatarColor": color, "backend": "claude",
        "permission": "ask", "status": status, "activity": activity, "unread": unread, "lastAt": now, "createdAt": now - 86_400_000,
        "kind": "agent", "position": 0, "cwd": "/demo/pace", "approvalRules": [], "actionRules": {}, "connectors": [], "skills": [],
    }})
}

fn entry(seq: i64, bot: &str, kind: &str, turn: i64, at: i64, data: &Value) -> Value {
    json!({"type": "entry", "rev": 10 + seq, "entry": {"id": format!("demo-{seq}"), "seq": seq, "botId": bot, "kind": kind, "turn": turn, "data": data, "createdAt": at}})
}

/// The events of the demo, in the order the host would send them: three bots, a session with steps and a diff, an approval
/// waiting for the person.
pub fn events(now: i64) -> Vec<Value> {
    let patch =
        "@@ -18,0 +18,3 @@\n+function weeklyTotals(rows) {\n+  return rows.reduce((sum, r) => sum + r.km, 0)\n+}\n";
    vec![
        json!({"type": "hello", "hostId": "demo-host", "rev": 1, "usage": {"providers": []}, "screen": null, "network": null}),
        bot(["scout", "Scout", "Watches the tests", "squircle", "green", "idle", ""], now - 60_000, 0),
        bot(
            [
                "pacer",
                "Pacer",
                "Owns the pace math",
                "blob",
                "orange",
                "needsInput",
                "Needs approval: Edit src/pace.js",
            ],
            now - 30_000,
            1,
        ),
        bot(["scribe", "Scribe", "Writes the changelog", "circle", "blue", "idle", ""], now - 3_600_000, 0),
        entry(1, "scout", "user", 1, now - 600_000, &json!({"text": "Why is the weekly total wrong on Mondays?"})),
        entry(
            2,
            "scout",
            "tool",
            1,
            now - 590_000,
            &json!({"title": "Read src/pace.js", "toolKind": "read", "status": "completed"}),
        ),
        entry(
            3,
            "scout",
            "tool",
            1,
            now - 580_000,
            &json!({"title": "Edit src/pace.js", "toolKind": "edit", "status": "completed",
                "diffs": [{"path": "src/pace.js", "added": 3, "removed": 0, "isNew": false, "startLine": 18, "patch": patch}]}),
        ),
        entry(
            4,
            "scout",
            "agent",
            1,
            now - 570_000,
            &json!({"text": "The week started on Sunday for the total but Monday for the chart. I made both use the ISO week and added a test.", "final": true}),
        ),
        entry(
            5,
            "pacer",
            "permission",
            2,
            now - 30_000,
            &json!({"title": "Edit src/pace.js", "toolKind": "edit", "status": "pending", "author": "pacer", "detail": "", "diffs": [],
                "options": [
                    {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                    {"optionId": "deny", "name": "Deny", "kind": "reject_once"}
                ], "selected": null}),
        ),
    ]
}

/// What a command answers in the demo. Anything that would change something is refused, politely.
pub fn answer(method: &str, now: i64) -> Result<Value, String> {
    match method {
        "hello" => Ok(json!({
            "hostId": "demo-host", "name": "Demo computer", "version": env!("CARGO_PKG_VERSION"), "os": "demo", "home": "/demo",
            "computerId": COMPUTER, "backends": [], "rev": 1, "screen": null,
        })),
        "history" => Ok(json!({"entries": []})),
        "routines" => Ok(json!({"routines": [{
            "id": "demo-routine", "botId": "scout", "name": "Morning check", "instruction": "Run the tests and tell me what broke overnight.",
            "triggers": [], "enabled": true, "createdAt": now - 86_400_000, "updatedAt": now - 3_600_000,
            "triggerDescriptions": ["Every weekday at 9:00"], "nextRunAt": now + 12 * 3_600_000, "timeoutSeconds": 600, "lastError": null,
        }], "runs": []})),
        "usage" | "weekSummary" | "markRead" | "track" | "securityProfiles" | "hostHealth" | "telegramStatus"
        | "slackStatus" | "skillProposals" | "beliefs" | "memory" => Ok(json!({})),
        _ => Err("Demo mode: nothing is sent or changed. Leave it to use your bots.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_has_bots_a_session_with_a_diff_an_approval_and_a_routine() {
        let events = events(1_000_000);
        let kinds: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();
        assert_eq!(kinds.iter().filter(|k| **k == "bot").count(), 3);
        let entries: Vec<&Value> = events.iter().filter(|e| e["type"] == "entry").map(|e| &e["entry"]).collect();
        assert!(
            entries.iter().any(|e| e["kind"] == "tool" && e["data"]["diffs"][0]["added"] == 3),
            "a step with a diff"
        );
        assert!(
            entries.iter().any(|e| e["kind"] == "permission" && e["data"]["status"] == "pending"),
            "an approval waiting"
        );
        let routines = answer("routines", 1_000_000).expect("routines");
        assert_eq!(routines["routines"][0]["name"], "Morning check");
        let seqs: Vec<i64> = entries.iter().filter_map(|e| e["seq"].as_i64()).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "in order");
    }

    #[test]
    fn what_would_change_something_is_refused_and_the_rest_answers() {
        assert!(answer("send", 0).is_err());
        assert!(answer("createBot", 0).is_err());
        assert!(answer("respondPermission", 0).is_err());
        assert_eq!(answer("hello", 0).expect("hello")["computerId"], COMPUTER);
        assert!(answer("history", 0).is_ok());
    }

    #[test]
    fn the_script_is_made_of_data_only() {
        // No file, no network, no real host: the module is data and a match.
        let source = include_str!("demo.rs");
        let body = source.split("#[cfg(test)]").next().unwrap_or_default();
        for forbidden in
            ["std::fs", "tokio::fs", "reqwest", "TcpStream", "crate::http", "data_dir", "std::process", "std::net"]
        {
            assert!(!body.contains(forbidden), "the demo script must not use {forbidden}");
        }
    }
}
