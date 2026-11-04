//! `codync-host hook`: the relay every hook event of an agent running in a terminal runs.
//!
//! Reads the agent's hook JSON on stdin, maps its event and field names onto Claude Code's (`external::normalize`),
//! adds what the host needs to tell sessions apart, and posts it to `POST /ingest/hook`.
//!
//! Hard rule: **never block the agent.**
//! * No token or no host listening: exit 0 at once, with only the "no opinion" reply the agent expects.
//! * A connection budget of 300 ms; a host that accepts a connection and then stops reading cannot wedge the
//!   session either, because the whole run has a deadline.
//! * Only a permission request waits for an answer, and a missing answer means the agent asks in its terminal
//!   exactly as if Codync were not installed.
//!
//! Adapted from the MIT-licensed `coucou-hook` (github.com/Louis-CFM/coucou, `windows/hook/src/main.rs`).

use crate::external::{OWN_AGENT_ENV, normalize, reply};
use crate::service;
use anyhow::Result;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Budget for reaching the host. Beyond this the agent wins, always.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
/// Whole-run budget for an event nobody waits on: connect and write, no more.
const FIRE_AND_FORGET_BUDGET: Duration = Duration::from_secs(2);
/// How long a permission request may wait for a person; the host gives up 2 s earlier and says so on the card.
const DECISION_BUDGET: Duration = Duration::from_secs(115);

/// Fields that are pointless to forward and can be enormous (a whole file read, a command's full output).
const DROPPED_FIELDS: &[&str] = &["tool_response", "tool_output", "transcript_path"];
/// Longest string forwarded for any single field.
const MAX_FIELD_LEN: usize = 2_000;
/// The edit strings of a finished edit are kept whole for the chat's diff, within these limits.
const DIFF_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write"];
const DIFF_FIELDS: &[&str] = &["old_string", "new_string", "content"];
const MAX_DIFF_FIELD_LEN: usize = 256 * 1024;
const MAX_DIFF_TOTAL: usize = 512 * 1024;
/// A reply is a short JSON object; anything longer is not one.
const MAX_RESPONSE: usize = 8 * 1024;

pub async fn run(port: u16, agent: String, event: Option<String>, ask: bool) -> ExitCode {
    // Codync's own bots run this same agent: their events are the host's own business.
    if std::env::var_os(OWN_AGENT_ENV).is_some() {
        return ExitCode::SUCCESS;
    }
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let Some(event) = prepare(&raw, &agent, event.as_deref().unwrap_or_default(), ask) else {
        // An agent that needs JSON still gets its "no opinion".
        print(reply::stdout(&agent, normalize::event(event.as_deref().unwrap_or_default()), None));
        return ExitCode::SUCCESS;
    };
    let waits = (event.name == "PermissionRequest" && reply::takes_decisions(&agent)) || event.asks;
    let answer = talk(port, &event.line, waits).await;
    if event.asks {
        print(answer.as_ref().and_then(reply::pre_tool));
    } else {
        let decision = answer.as_ref().and_then(decision_in);
        print(reply::stdout(&agent, &event.name, decision.as_deref()));
    }
    ExitCode::SUCCESS
}

fn print(line: Option<String>) {
    use std::io::Write as _;
    if let Some(line) = line {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

struct Event {
    /// The envelope as one line of JSON.
    line: String,
    /// The canonical event name.
    name: String,
    /// A question or a plan of Claude Code that a person may answer from a Codync client.
    asks: bool,
}

/// The tools whose answer a person may give from a client.
const ASKED_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];

/// The payload to forward, from the raw stdin bytes. Pure, so tests need no host.
fn prepare(raw: &[u8], agent: &str, event: &str, ask: bool) -> Option<Event> {
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let mut payload = serde_json::from_slice::<Value>(raw).ok()?;
    let map = payload.as_object_mut()?;
    let env = |key: &str| std::env::var(key).ok();

    let raw_event =
        map.get("hook_event_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(event).to_owned();
    normalize::fields(map, &env);
    let name = normalize::refine(normalize::event(&raw_event), map);
    map.insert("hook_event_name".into(), Value::String(name.clone()));
    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    if map.get("cwd").and_then(Value::as_str).is_none_or(str::is_empty)
        && let Ok(cwd) = std::env::current_dir()
    {
        map.insert("cwd".into(), Value::String(cwd.to_string_lossy().into_owned()));
    }
    truncate_payload(&mut payload, &name);

    let terminal = ["WT_SESSION", "ITERM_SESSION_ID", "TERM_SESSION_ID", "VSCODE_PID"]
        .iter()
        .find_map(|k| env(k).filter(|v| !v.is_empty()))
        .unwrap_or_default();
    let stdout_is_terminal = env("TERM_PROGRAM").unwrap_or_default();
    let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or_default();
    let asks = ask && agent.is_empty() && name == "PreToolUse" && ASKED_TOOLS.contains(&tool);
    let envelope = json!({
        "agent": agent,
        "ask": asks,
        "terminal": {"program": stdout_is_terminal, "session": terminal},
        "payload": payload,
    });
    Some(Event { line: envelope.to_string(), name, asks })
}

/// Caps every string, except the edit text of a finished `Edit` / `MultiEdit` / `Write`, which the chat's diff needs
/// whole; if even those had to be cut, `diffTruncated` tells the host not to show counts it cannot trust.
fn truncate_payload(payload: &mut Value, event: &str) {
    let keeps_diff = event == "PostToolUse"
        && payload.get("tool_name").and_then(Value::as_str).is_some_and(|t| DIFF_TOOLS.contains(&t));
    let input = if keeps_diff { payload.as_object_mut().and_then(|map| map.remove("tool_input")) } else { None };
    truncate_strings(payload);
    if let Some(mut input) = input {
        let mut budget = MAX_DIFF_TOTAL;
        let mut cut_any = false;
        cap_diff_strings(&mut input, &mut budget, &mut cut_any);
        if let Some(map) = payload.as_object_mut() {
            map.insert("tool_input".into(), input);
            if cut_any {
                map.insert("diffTruncated".into(), Value::Bool(true));
            }
        }
    }
}

fn cap_diff_strings(value: &mut Value, budget: &mut usize, cut_any: &mut bool) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match v {
                    Value::String(s) if DIFF_FIELDS.contains(&key.as_str()) => {
                        if cut(s, MAX_DIFF_FIELD_LEN.min(*budget)) {
                            *cut_any = true;
                        }
                        *budget = budget.saturating_sub(s.len());
                    }
                    _ => cap_diff_strings(v, budget, cut_any),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| cap_diff_strings(i, budget, cut_any)),
        Value::String(s) => {
            cut(s, MAX_FIELD_LEN);
        }
        _ => {}
    }
}

fn truncate_strings(value: &mut Value) {
    match value {
        Value::String(s) => {
            cut(s, MAX_FIELD_LEN);
        }
        Value::Array(items) => items.iter_mut().for_each(truncate_strings),
        Value::Object(map) => map.values_mut().for_each(truncate_strings),
        _ => {}
    }
}

/// Shortens `s` to at most `max` bytes plus an ellipsis; true if it was cut. Cut on a char boundary.
fn cut(s: &mut String, max: usize) -> bool {
    if s.len() <= max {
        return false;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push('…');
    true
}

/// Posts the event and returns the host's reply, if it had one in time and the event is one that waits for it.
async fn talk(port: u16, body: &str, waits: bool) -> Option<Value> {
    let token = std::fs::read_to_string(service::data_dir().join("token")).ok()?;
    let token = token.trim().to_owned();
    let request = format!(
        "POST /ingest/hook HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let budget = if waits { DECISION_BUDGET } else { FIRE_AND_FORGET_BUDGET };
    let sent = tokio::time::timeout(budget, exchange(port, &request)).await.ok().flatten()?;
    reply_of(&sent).filter(|_| waits)
}

async fn exchange(port: u16, request: &str) -> Option<String> {
    let mut stream =
        tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect(("127.0.0.1", port))).await.ok()?.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_RESPONSE {
            return None;
        }
        if let Some((head, body)) = split_response(&buf)
            && body.len() >= content_length(head)
        {
            break;
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// The head and the body so far.
fn split_response(buf: &[u8]) -> Option<(&str, &[u8])> {
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?;
    Some((head, &buf[split + 4..]))
}

fn content_length(head: &str) -> usize {
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0)
}

/// The host's JSON reply; a failed request or a non-200 answer is no reply.
fn reply_of(response: &str) -> Option<Value> {
    let (head, body) = split_response(response.as_bytes())?;
    if !head.starts_with("HTTP/1.1 200") {
        return None;
    }
    serde_json::from_str(std::str::from_utf8(body).ok()?).ok()
}

/// `{"decision": "allow"}` → `allow`. A failed request, a non-200 answer or no decision is no decision.
#[cfg(test)]
fn decision_of(response: &str) -> Option<String> {
    decision_in(&reply_of(response)?)
}

fn decision_in(reply: &Value) -> Option<String> {
    match reply.get("decision")?.as_str()? {
        "allow" => Some("allow".to_owned()),
        "deny" => Some("deny".to_owned()),
        _ => None,
    }
}

// MARK: installing the hook in Claude Code

/// `~/.claude/settings.json`: where Claude Code keeps the hooks of every session on this computer.
fn claude_settings() -> Option<std::path::PathBuf> {
    Some(dirs::home_dir()?.join(".claude").join("settings.json"))
}

fn with_settings(f: impl FnOnce(&Path) -> Result<bool>) -> Result<()> {
    let Some(settings) = claude_settings() else { anyhow::bail!("can't find your home folder") };
    let changed = f(&settings)?;
    let message = if changed { "Claude Code now reports to Codync." } else { "Nothing changed." };
    println!("{message}\nSettings: {}", settings.display());
    Ok(())
}

/// Adds Codync's hook for Claude Code's every event.
pub fn install() -> Result<()> {
    let exe = std::env::current_exe()?;
    with_settings(|settings| crate::external::install::install(settings, &exe))
}

/// Removes Codync's hook again, leaving the person's own hooks as they were.
pub fn uninstall() -> Result<()> {
    with_settings(crate::external::install::uninstall)
}

pub fn status() -> Result<()> {
    let Some(settings) = claude_settings() else { anyhow::bail!("can't find your home folder") };
    let on = crate::external::install::installed(&settings)?;
    println!("Claude Code hooks: {}\nSettings: {}", if on { "on" } else { "off" }, settings.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forwarded(raw: &str, agent: &str, event: &str) -> (Value, Event) {
        let ev = prepare(raw.as_bytes(), agent, event, false).expect("forwarded");
        let v = serde_json::from_str(ev.line.trim_end()).unwrap();
        (v, ev)
    }

    #[test]
    fn claude_codes_payload_reaches_the_host_under_our_names() {
        let (v, ev) = forwarded(
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/p","session_id":"s"}"#,
            "",
            "",
        );
        assert_eq!(ev.name, "PreToolUse");
        assert_eq!(v["agent"], "");
        assert_eq!(v["payload"]["session_id"], "s");
        assert_eq!(v["payload"]["tool_input"]["command"], "ls");
        assert!(v["terminal"].is_object());
    }

    #[test]
    fn another_agents_event_is_renamed_and_tagged() {
        let (v, ev) = forwarded(
            r#"{"hook_event_name":"BeforeTool","toolCall":{"name":"shell","args":{"CommandLine":"ls"}}}"#,
            "gemini",
            "PreToolUse",
        );
        assert_eq!((ev.name.as_str(), v["agent"].as_str()), ("PreToolUse", Some("gemini")));
        assert_eq!(v["payload"]["tool_input"]["command"], "ls");
        // Copilot sends no event name: the command line's is used.
        let (_, ev) = forwarded(r#"{"toolName":"bash"}"#, "copilot", "permissionRequest");
        assert_eq!(ev.name, "PermissionRequest");
    }

    #[test]
    fn what_cannot_be_read_is_not_forwarded() {
        for raw in ["", "not json", "[1,2]"] {
            assert!(prepare(raw.as_bytes(), "", "", false).is_none(), "{raw}");
        }
    }

    #[test]
    fn a_finished_edit_keeps_its_text_whole_for_the_chat_diff() {
        let big = "line\n".repeat(4_000);
        // The text inside JSON: its line breaks escaped, as the agent sends them.
        let json = big.replace('\n', "\\n");
        let raw = format!(
            r#"{{"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{{"file_path":"a","old_string":"{json}","new_string":"{json}"}},"cwd":"{}"}}"#,
            "c".repeat(5_000)
        );
        let (v, _) = forwarded(&raw, "", "");
        assert_eq!(v["payload"]["tool_input"]["old_string"].as_str().unwrap().len(), big.len());
        assert!(v["payload"]["cwd"].as_str().unwrap().len() <= MAX_FIELD_LEN + 4);
        assert!(v["payload"].get("diffTruncated").is_none());
        // Before the edit happens: the ordinary cap.
        let raw =
            format!(r#"{{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{{"old_string":"{json}"}}}}"#);
        let (v, _) = forwarded(&raw, "", "");
        assert!(v["payload"]["tool_input"]["old_string"].as_str().unwrap().len() <= MAX_FIELD_LEN + 4);
    }

    #[test]
    fn an_edit_beyond_the_budget_is_cut_and_flagged() {
        let huge = "x".repeat(MAX_DIFF_FIELD_LEN + 10);
        let raw = format!(
            r#"{{"hook_event_name":"PostToolUse","tool_name":"Write","tool_input":{{"file_path":"a","content":"{huge}"}}}}"#
        );
        let (v, _) = forwarded(&raw, "", "");
        assert!(v["payload"]["tool_input"]["content"].as_str().unwrap().len() <= MAX_DIFF_FIELD_LEN + 4);
        assert_eq!(v["payload"]["diffTruncated"], true);
    }

    #[test]
    fn only_a_200_answer_with_a_decision_is_one() {
        assert_eq!(
            decision_of("HTTP/1.1 200 OK\r\ncontent-length: 24\r\n\r\n{\"decision\":\"allow\"}    ").as_deref(),
            Some("allow")
        );
        assert_eq!(decision_of("HTTP/1.1 401 Unauthorized\r\n\r\n{\"decision\":\"allow\"}"), None);
        assert_eq!(decision_of("HTTP/1.1 200 OK\r\n\r\n{\"decision\":null}"), None);
        assert_eq!(decision_of("garbage"), None);
        assert_eq!(content_length("HTTP/1.1 200 OK\r\nContent-Length: 12\r\n"), 12);
    }

    #[test]
    fn only_the_waiting_hook_of_claude_code_asks_about_questions_and_plans() {
        let raw = |tool: &str| format!(r#"{{"hook_event_name":"PreToolUse","tool_name":"{tool}","tool_input":{{}}}}"#);
        let asks = |tool: &str, agent: &str, ask: bool| prepare(raw(tool).as_bytes(), agent, "", ask).unwrap().asks;
        assert!(asks("AskUserQuestion", "", true));
        assert!(asks("ExitPlanMode", "", true));
        assert!(!asks("AskUserQuestion", "", false), "the plain hook only reports");
        assert!(!asks("Bash", "", true));
        assert!(!asks("AskUserQuestion", "codex", true));
    }
}
