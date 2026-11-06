//! Wire shape snapshot (docs/reference/compatibility.md).

use super::*;

/// Snapshot of what clients decode: field paths and JSON types of `hello` and the event
/// stream after a full turn. Values aren't kept, only the shape.
const WIRE_SHAPE: &str = "tests/fixtures/wire-shape.json";
/// Re-records the snapshot (refused while a field is removed or retyped without a new `minApp`).
const UPDATE_ENV: &str = "CODYNC_UPDATE_WIRE_SHAPE";
/// Depends on the machine (installed harnesses, displays, addresses, local usage), not the code.
const MACHINE_DEPENDENT: [&str; 4] = ["backends", "screen", "urls", "usage"];

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Every field path under `v` with its type; array elements merge under `[]`.
fn shape(v: &Value, path: &str, out: &mut std::collections::BTreeMap<String, String>) {
    out.insert(path.to_owned(), json_type(v).to_owned());
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                if !MACHINE_DEPENDENT.contains(&k.as_str()) {
                    shape(child, &format!("{path}.{k}"), out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                shape(item, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

/// The shape of a stream event, keyed by its type (and an entry's kind: their data differ).
fn event_shape(ev: &Value, out: &mut std::collections::BTreeMap<String, String>) {
    let ty = ev["type"].as_str().unwrap_or("?");
    let key = match ev["entry"]["kind"].as_str() {
        Some(kind) if ty == "entry" => format!("event:entry({kind})"),
        _ => format!("event:{ty}"),
    };
    shape(ev, &key, out);
}

/// What changed between the recorded and the current shape: (breaking, additions).
/// A field that was `null` on either side can't be judged by its type.
fn shape_diff(
    recorded: &std::collections::BTreeMap<String, String>,
    current: &std::collections::BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let mut breaking = vec![];
    for (path, ty) in recorded {
        match current.get(path) {
            None => breaking.push(format!("removed {path} ({ty})")),
            Some(now) if now != ty && now != "null" && ty != "null" => {
                breaking.push(format!("retyped {path}: {ty} → {now}"));
            }
            Some(_) => {}
        }
    }
    let added = current
        .iter()
        .filter(|(path, ty)| recorded.get(*path).is_none_or(|old| old == "null" && *ty != "null"))
        .map(|(path, ty)| format!("{path} ({ty})"))
        .collect();
    (breaking, added)
}

#[test]
fn shape_diff_tells_breaking_from_additive() {
    let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    let recorded = map(&[("a", "object"), ("a.x", "string"), ("a.y", "null"), ("a.z", "number")]);
    let current = map(&[("a", "object"), ("a.x", "number"), ("a.y", "string"), ("a.w", "bool")]);
    let (breaking, added) = shape_diff(&recorded, &current);
    assert_eq!(breaking, ["retyped a.x: string → number", "removed a.z (number)"]);
    assert_eq!(added, ["a.w (bool)", "a.y (string)"]);
}

/// Reads the event stream's catch-up until it goes quiet.
async fn catch_up(host: &Host) -> Vec<Value> {
    let res = reqwest::Client::new()
        .get(format!("{}/events?since=0&client=test", host.base))
        .bearer_auth(&host.token)
        .send()
        .await
        .unwrap();
    let mut body = res.bytes_stream();
    let (mut buf, mut events) = (String::new(), vec![]);
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_millis(1500), body.next()).await {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find('\n') {
            let line: String = buf.drain(..=i).collect();
            if let Some(data) = line.trim_end().strip_prefix("data:")
                && let Ok(v) = serde_json::from_str::<Value>(data.trim_start())
                // A live usage refresh lands whenever the agents' usage poll finishes (later on a
                // busy machine); its shape is `hello.usage`'s.
                && v["type"] != "usage"
            {
                events.push(v);
            }
        }
    }
    events
}

/// Older apps read what the host sends: a removed or retyped field breaks them. This fails
/// until the field is kept, or `MIN_APP` is raised the two-release way and the snapshot
/// re-recorded. New fields only need recording (`CODYNC_UPDATE_WIRE_SHAPE=1`).
#[tokio::test]
async fn wire_shape_matches_the_snapshot() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = start_host().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Tester", "backend": "custom", "command": common::python_agent(&agent),
                "cwd": host.home.to_string_lossy(), "permission": "ask",
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    host.call("send", json!({"botId": bot, "text": "go", "clientNonce": "n1"})).await;
    let card = host.wait_for(&bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    host.wait_for(&bot, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    // A bot another bot created: its `createdBy`.
    let lead = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    let lead = host
        .call(
            "createBot",
            json!({"name": "Lead", "backend": "custom", "command": common::python_agent(&lead), "connectors": [],
                "permission": "auto", "manageTeam": true}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    host.call("send", json!({"botId": lead, "text": "MANAGE create_bot {\"name\": \"Made\"}", "clientNonce": "n2"}))
        .await;
    let made = host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert!(made["data"]["text"].as_str().unwrap().starts_with("Done: "), "{made}");
    // A bot in a roster section: its `section`.
    host.call("updateBot", json!({"id": bot, "section": "Work"})).await;

    let hello = host.call("hello", json!({})).await;
    let min_app = hello["minApp"].as_str().unwrap().to_owned();
    let mut current = std::collections::BTreeMap::new();
    shape(&hello, "hello", &mut current);
    for ev in catch_up(&host).await {
        event_shape(&ev, &mut current);
    }
    assert!(current.keys().any(|k| k.starts_with("event:entry(tool)")), "the turn's entries are missing");

    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(WIRE_SHAPE);
    let recorded: Value = std::fs::read_to_string(&file).map_or(Value::Null, |s| serde_json::from_str(&s).unwrap());
    let recorded_shapes: std::collections::BTreeMap<String, String> =
        serde_json::from_value(recorded["shapes"].clone()).unwrap_or_default();
    let (breaking, added) = shape_diff(&recorded_shapes, &current);
    let floor_raised = recorded["minApp"].as_str() != Some(min_app.as_str());
    assert!(
        breaking.is_empty() || floor_raised,
        "Older apps would break: {breaking:#?}\nKeep these fields, or follow the two-release rule \
         in docs/reference/compatibility.md: raise MIN_APP (host/src/compat.rs) once that iPhone \
         version is live, then re-record with {UPDATE_ENV}=1 cargo test --test e2e wire_shape"
    );
    if std::env::var_os(UPDATE_ENV).is_some() {
        let json = json!({"minApp": min_app, "shapes": current});
        std::fs::write(&file, serde_json::to_string_pretty(&json).unwrap() + "\n").unwrap();
        return;
    }
    assert!(
        breaking.is_empty() && added.is_empty() && !floor_raised,
        "The wire shape changed.\nAdded: {added:#?}\nRemoved or retyped (minApp was raised): {breaking:#?}\n\
         Record it with {UPDATE_ENV}=1 cargo test --test e2e wire_shape"
    );
}
