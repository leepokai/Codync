//! pi-acp takes a session's `mcpServers` and never hands them to pi (its README's "Limitations"), so a bot on pi
//! had none of Codync's tools. pi-acp starts the command in `PI_ACP_PI_COMMAND`: Codync's launcher, which starts pi
//! with an extension that registers the servers it finds in `CODYNC_PI_MCP_SERVERS` (`pi.registerMcpServer`, pi
//! 0.99 or later). The extension also passes on the error pi ended a turn in, which pi-acp drops (see `EXTENSION`).
// ponytail: drop the MCP registration once pi-acp passes `mcpServers` to pi itself (svkozak/pi-acp#164); the launcher
// stays while the turn-error hook does.

use super::codex_names::{self, Renamed};
use crate::agent::acp::{INTERNAL_ERROR, RpcError};
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Older pi has no `registerMcpServer` and starts without the tools. With pi-acp's own bridge (`PI_ACP_MCP_SERVERS`)
/// pi already has them, and registering them twice would stop pi. A server that can't register (another extension
/// already has its name) is the only one left out: pi doesn't start when an extension throws.
///
/// pi-acp answers a turn whose model request failed with a plain `end_turn` and drops pi's error (#470), so once pi
/// is done retrying, the extension says the error as a notification (which pi-acp passes on as text) behind
/// [`TURN_ERROR`], and the host fails the turn with it ([`turn_error`]).
// ponytail: drop the turn-error hook once pi-acp ends such a turn with an error itself.
const EXTENSION: &str = r#"export default function (pi) {
  let last;
  pi.on("message_end", (event) => {
    if (event.message?.role === "assistant") last = event.message;
  });
  pi.on("agent_settled", (event, ctx) => {
    if (!event.aborted && last?.stopReason === "error") {
      ctx.ui.notify("[codync: turn failed] " + (last.errorMessage || "pi's model request failed"), "error");
    }
    last = undefined;
  });
  if (typeof pi.registerMcpServer !== "function" || process.env.PI_ACP_MCP_SERVERS) return;
  const servers = JSON.parse(process.env.CODYNC_PI_MCP_SERVERS || "{}");
  for (const [name, config] of Object.entries(servers)) {
    try {
      pi.registerMcpServer(name, config);
    } catch (e) {
      console.error(`codync: ${name}: ${e.message}`);
    }
  }
}
"#;

/// Leads the notification the extension sends for a turn pi ended in an error.
const TURN_ERROR: &str = "[codync: turn failed] ";

/// The error an agent message chunk (`update`, its `text`) carries when it's the extension's notification to pi
/// (pi-acp marks it `level` `error`), not reply text: an agent quoting the notification is still talking.
pub(super) fn turn_error_text<'a>(backend: &str, update: &Value, text: &'a str) -> Option<&'a str> {
    let notified = backend == "pi" && update["_meta"]["piAcp"]["notify"]["level"] == "error";
    text.strip_prefix(TURN_ERROR).filter(|_| notified)
}

/// The prompt's answer `done`, failed with `error` when pi ended the turn in one and pi-acp still answered `end_turn`.
pub(super) fn turn_result(done: Result<Value>, error: Option<String>) -> Result<Value> {
    match error {
        Some(error) if done.as_ref().is_ok_and(|v| v["stopReason"] == "end_turn") => Err(turn_error(&error)),
        _ => done,
    }
}

/// The failure of a turn pi ended in `message` (pi's words: `503: {…}`). The HTTP status it starts with says why, so
/// a busy or rate-limited provider moves the bot on to its next agent.
fn turn_error(message: &str) -> anyhow::Error {
    let kind = match message.split([':', ' ']).next().and_then(|s| s.parse::<u16>().ok()) {
        Some(429) => Some("rate_limit"),
        Some(500..=599) => Some("overloaded"),
        _ => None,
    };
    RpcError { code: INTERNAL_ERROR, message: message.to_owned(), kind: kind.map(Into::into) }.into()
}

/// pi-acp's own command for pi, plus the extension beside the launcher.
#[cfg(unix)]
const LAUNCHER: (&str, &str) = ("pi-codync", "#!/bin/sh\nexec pi -e \"$(dirname \"$0\")/codync.js\" \"$@\"\n");
#[cfg(windows)]
const LAUNCHER: (&str, &str) = ("pi-codync.cmd", "@pi.cmd -e \"%~dp0codync.js\" %*\r\n");

/// Seconds pi waits for a tool call (60 by default): past the host's own limit for any of them (`mcp::call`, the
/// longest a team change waiting for its approval).
const CALL_TIMEOUT: u64 = crate::chat::manage::APPROVAL_TIMEOUT.as_secs() + 60;

/// The environment that gives pi `servers` (ACP `mcpServers`).
pub(super) async fn env(servers: &Value) -> Result<Vec<(String, String)>> {
    let dir = crate::service::data_dir().join("pi");
    let launcher = tokio::task::spawn_blocking(move || write_launcher(&dir)).await??;
    Ok(vec![
        ("PI_ACP_PI_COMMAND".into(), launcher.to_string_lossy().into_owned()),
        ("CODYNC_PI_MCP_SERVERS".into(), pi_servers(servers).to_string()),
    ])
}

/// Renames each of `servers` whose name pi's own MCP config has (`mcp.json` in pi's agent folder or the project's
/// `.pi`, disabled entries too): pi leaves out a registered server its config names. `agent_dir` is the agent's
/// own `PI_CODING_AGENT_DIR` when it has one.
pub(super) async fn avoid_clashes(servers: &mut Value, cwd: &str, agent_dir: Option<PathBuf>) -> Renamed {
    let agent_dir = agent_dir
        .or_else(|| std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(".pi/agent")));
    let files = agent_dir.into_iter().chain([Path::new(cwd).join(".pi")]).map(|dir| dir.join("mcp.json")).collect();
    codex_names::rename_taken(servers, files, taken).await
}

/// Every server name in the `mcpServers` of `files`.
fn taken(files: &[PathBuf]) -> HashSet<String> {
    files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter_map(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok())
        .filter_map(|config| config["mcpServers"].as_object().map(|s| s.keys().cloned().collect::<Vec<_>>()))
        .flatten()
        .collect()
}

/// Blocking: the launcher and the extension in `dir`, each written beside the old one and renamed over it, and
/// only when it changed, so a pi starting meanwhile never reads half a file.
fn write_launcher(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let launcher = dir.join(LAUNCHER.0);
    for (path, text) in [(dir.join("codync.js"), EXTENSION), (launcher.clone(), LAUNCHER.1)] {
        if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
            continue;
        }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&temporary, &path).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(launcher)
}

/// ACP's `[{name, command, args, env: [{name, value}]}]` as pi's `{name: {command, args, env: {name: value}}}`,
/// declared to the model like built-in tools (`direct`), as every other harness has them.
fn pi_servers(servers: &Value) -> Value {
    let mut out = Map::new();
    for s in servers.as_array().into_iter().flatten() {
        let Some(name) = s["name"].as_str() else { continue };
        let env: Map<String, Value> = s["env"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| Some((e["name"].as_str()?.to_owned(), e["value"].clone())))
            .collect();
        out.insert(
            name.to_owned(),
            json!({"command": s["command"], "args": s["args"], "env": env, "exposure": "direct", "timeout": CALL_TIMEOUT}),
        );
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_servers_become_direct_pi_servers() {
        let acp = json!([{"name": "manage", "command": "/bin/codync-host", "args": ["mcp", "manage"],
            "env": [{"name": "CODYNC_HOME", "value": "/h"}]}]);
        assert_eq!(
            pi_servers(&acp),
            json!({"manage": {"command": "/bin/codync-host", "args": ["mcp", "manage"],
                "env": {"CODYNC_HOME": "/h"}, "exposure": "direct", "timeout": 330}})
        );
    }

    #[test]
    fn a_busy_provider_moves_the_bot_on() {
        use crate::agent::fallback::reason::{Reason, of};
        assert!(EXTENSION.contains(TURN_ERROR));
        let busy = turn_error(r#"503: {"message":"queued: upstream busy, try again in 60s"}"#);
        assert_eq!(of(&busy), Some(Reason::Overloaded));
        assert!(busy.to_string().contains("upstream busy"));
        assert_eq!(of(&turn_error("429 Too Many Requests")), Some(Reason::RateLimited));
        assert_eq!(of(&turn_error("400: context too long")), None);
        assert_eq!(of(&turn_error("pi's model request failed")), None);
    }

    #[test]
    fn only_an_ended_turn_fails_with_pi_s_error() {
        let ended = || Ok(json!({ "stopReason": "end_turn" }));
        let error = || Some("503: busy".to_owned());
        let notified = json!({ "_meta": { "piAcp": { "notify": { "level": "error" } } } });
        let quoted = format!("{TURN_ERROR}503: busy");
        assert_eq!(turn_error_text("pi", &notified, &quoted), Some("503: busy"));
        assert_eq!(turn_error_text("pi", &notified, "a reply"), None);
        // Quoted in a reply, by pi or another agent, it is the reply's text.
        assert_eq!(turn_error_text("pi", &json!({}), &quoted), None);
        assert_eq!(turn_error_text("claude", &notified, &quoted), None);
        assert!(turn_result(ended(), error()).is_err());
        assert!(turn_result(ended(), None).is_ok());
        let cancelled = json!({ "stopReason": "cancelled" });
        assert_eq!(turn_result(Ok(cancelled.clone()), error()).expect("not failed"), cancelled);
    }

    #[test]
    fn the_launcher_is_written_once() {
        let dir = std::env::temp_dir().join(format!("codync-pi-tools-{}", uuid::Uuid::new_v4()));
        let launcher = write_launcher(&dir).unwrap();
        let written = std::fs::metadata(&launcher).unwrap().modified().unwrap();
        assert_eq!(write_launcher(&dir).unwrap(), launcher);
        assert_eq!(std::fs::metadata(&launcher).unwrap().modified().unwrap(), written);
        assert_eq!(std::fs::read_to_string(dir.join("codync.js")).unwrap(), EXTENSION);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "no temporary file is left");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn servers_pis_config_names_get_a_free_name() {
        let dir = std::env::temp_dir().join(format!("codync-pi-names-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("agent")).unwrap();
        std::fs::create_dir_all(dir.join("app/.pi")).unwrap();
        std::fs::write(dir.join("agent/mcp.json"), "\u{feff}{\"mcpServers\": {\"memory\": {\"enabled\": false}}}")
            .unwrap();
        std::fs::write(dir.join("app/.pi/mcp.json"), r#"{"mcpServers": {"google_drive": {"command": "x"}}}"#).unwrap();
        let mut servers = json!([{"name": "memory"}, {"name": "google-drive"}, {"name": "chat"}]);
        let cwd = dir.join("app").display().to_string();
        let renamed = avoid_clashes(&mut servers, &cwd, Some(dir.join("agent"))).await;
        assert_eq!(servers, json!([{"name": "codync-memory"}, {"name": "codync-google-drive"}, {"name": "chat"}]));
        assert_eq!(renamed.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
