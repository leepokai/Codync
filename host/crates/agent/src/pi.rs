//! pi-acp takes a session's `mcpServers` and never hands them to pi (its README's "Limitations"), and answers a turn
//! whose model request failed with a plain `end_turn`, dropping pi's error (#470). pi-acp starts the command in
//! `PI_ACP_PI_COMMAND`, so [`env`] points it at a launcher that starts pi with an extension of the app's: it
//! registers the servers it finds in `<APP>_PI_MCP_SERVERS` (`pi.registerMcpServer`, pi 0.99 or later), and says
//! the error pi ended a turn in as a notification, which [`turn_error_text`] and [`turn_result`] turn back into a
//! failed turn. `app` names the app in the files, the variable and the notice (`codync`).
// ponytail: drop the MCP registration once pi-acp passes `mcpServers` to pi itself (svkozak/pi-acp#164); the launcher
// stays while the turn-error hook does.

use crate::acp::{INTERNAL_ERROR, RpcError, StopReason};
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// Older pi has no `registerMcpServer` and starts without the tools. With pi-acp's own bridge (`PI_ACP_MCP_SERVERS`)
/// pi already has them, and registering them twice would stop pi. A server that can't register (another extension
/// already has its name) is the only one left out: pi doesn't start when an extension throws. Once pi is done
/// retrying a failed model request, it says the error behind [`turn_error_prefix`].
// ponytail: drop the turn-error hook once pi-acp ends such a turn with an error itself.
fn extension(app: &str) -> String {
    let (servers, failed) = (servers_var(app), turn_error_prefix(app));
    format!(
        r#"export default function (pi) {{
  let last;
  pi.on("message_end", (event) => {{
    if (event.message?.role === "assistant") last = event.message;
  }});
  pi.on("agent_settled", (event, ctx) => {{
    if (!event.aborted && last?.stopReason === "error") {{
      ctx.ui.notify("{failed}" + (last.errorMessage || "pi's model request failed"), "error");
    }}
    last = undefined;
  }});
  if (typeof pi.registerMcpServer !== "function" || process.env.PI_ACP_MCP_SERVERS) return;
  const servers = JSON.parse(process.env.{servers} || "{{}}");
  for (const [name, config] of Object.entries(servers)) {{
    try {{
      pi.registerMcpServer(name, config);
    }} catch (e) {{
      console.error(`{app}: ${{name}}: ${{e.message}}`);
    }}
  }}
}}
"#
    )
}

/// The variable the extension reads the servers from.
fn servers_var(app: &str) -> String {
    format!("{}_PI_MCP_SERVERS", app.to_uppercase())
}

/// Leads the notification the extension sends for a turn pi ended in an error.
fn turn_error_prefix(app: &str) -> String {
    format!("[{app}: turn failed] ")
}

/// pi-acp's own command for pi (its file name and text), with the extension `<app>.js` beside it.
fn launcher(app: &str) -> (String, String) {
    if cfg!(windows) {
        (format!("pi-{app}.cmd"), format!("@pi.cmd -e \"%~dp0{app}.js\" %*\r\n"))
    } else {
        (format!("pi-{app}"), format!("#!/bin/sh\nexec pi -e \"$(dirname \"$0\")/{app}.js\" \"$@\"\n"))
    }
}

/// The environment that gives pi `servers` (ACP `mcpServers`), with the launcher kept in `dir`. pi waits
/// `call_timeout` seconds for a tool call (60 by default).
pub async fn env(dir: PathBuf, app: &str, servers: &Value, call_timeout: u64) -> Result<Vec<(String, String)>> {
    let owned = app.to_owned();
    let launcher = tokio::task::spawn_blocking(move || write_launcher(&dir, &owned)).await??;
    Ok(vec![
        ("PI_ACP_PI_COMMAND".into(), launcher.to_string_lossy().into_owned()),
        (servers_var(app), pi_servers(servers, call_timeout).to_string()),
    ])
}

/// The error reply text carries when it's the extension's notification (pi-acp passes it on with `notice` `error`),
/// not the reply's words: an agent quoting the notification is still talking. Only for pi.
pub fn turn_error_text<'a>(app: &str, backend: &str, notice: Option<&str>, text: &'a str) -> Option<&'a str> {
    if backend != "pi" || notice != Some("error") {
        return None;
    }
    text.strip_prefix(turn_error_prefix(app).as_str())
}

/// The prompt's answer `done`, failed with `error` when pi ended the turn in one and pi-acp still answered `end_turn`.
pub fn turn_result(done: Result<Value>, error: Option<String>) -> Result<Value> {
    match error {
        Some(error) if done.as_ref().is_ok_and(|v| StopReason::of(v) == StopReason::EndTurn) => Err(turn_error(&error)),
        _ => done,
    }
}

/// The failure of a turn pi ended in `message` (pi's words: `503: {…}`). The HTTP status it starts with says why: a
/// busy or rate-limited provider.
fn turn_error(message: &str) -> anyhow::Error {
    let kind = match message.split([':', ' ']).next().and_then(|s| s.parse::<u16>().ok()) {
        Some(429) => Some("rate_limit"),
        Some(500..=599) => Some("overloaded"),
        _ => None,
    };
    RpcError { code: INTERNAL_ERROR, message: message.to_owned(), kind: kind.map(Into::into) }.into()
}

/// Blocking: the launcher and the extension in `dir`, each written beside the old one and renamed over it, and
/// only when it changed, so a pi starting meanwhile never reads half a file.
fn write_launcher(dir: &Path, app: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let (name, script) = launcher(app);
    let launcher = dir.join(name);
    for (path, text) in [(dir.join(format!("{app}.js")), extension(app)), (launcher.clone(), script)] {
        if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
            continue;
        }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, &text).with_context(|| format!("writing {}", temporary.display()))?;
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
fn pi_servers(servers: &Value, call_timeout: u64) -> Value {
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
            json!({"command": s["command"], "args": s["args"], "env": env, "exposure": "direct", "timeout": call_timeout}),
        );
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_servers_become_direct_pi_servers() {
        let acp = json!([{"name": "manage", "command": "/bin/app", "args": ["mcp", "manage"],
            "env": [{"name": "APP_HOME", "value": "/h"}]}]);
        assert_eq!(
            pi_servers(&acp, 330),
            json!({"manage": {"command": "/bin/app", "args": ["mcp", "manage"],
                "env": {"APP_HOME": "/h"}, "exposure": "direct", "timeout": 330}})
        );
    }

    #[test]
    fn the_extension_reads_the_apps_variable_and_says_its_notice() {
        let js = extension("codync");
        assert!(js.contains("process.env.CODYNC_PI_MCP_SERVERS || \"{}\""));
        assert!(js.contains("ctx.ui.notify(\"[codync: turn failed] \" + "));
        assert!(js.contains("console.error(`codync: ${name}: ${e.message}`);"));
    }

    #[test]
    fn a_busy_provider_says_so() {
        let kind = |message: &str| turn_error(message).downcast::<RpcError>().expect("an RPC error").kind;
        assert_eq!(kind(r#"503: {"message":"queued: upstream busy"}"#).as_deref(), Some("overloaded"));
        assert_eq!(kind("429 Too Many Requests").as_deref(), Some("rate_limit"));
        assert_eq!(kind("400: context too long"), None);
        assert_eq!(kind("pi's model request failed"), None);
    }

    #[test]
    fn only_an_ended_turn_fails_with_pi_s_error() {
        let ended = || Ok(json!({ "stopReason": "end_turn" }));
        let error = || Some("503: busy".to_owned());
        let notified = Some("error");
        let quoted = format!("{}503: busy", turn_error_prefix("app"));
        assert_eq!(turn_error_text("app", "pi", notified, &quoted), Some("503: busy"));
        assert_eq!(turn_error_text("app", "pi", notified, "a reply"), None);
        // Quoted in a reply, by pi or another agent, it is the reply's text.
        assert_eq!(turn_error_text("app", "pi", None, &quoted), None);
        assert_eq!(turn_error_text("app", "claude", notified, &quoted), None);
        assert!(turn_result(ended(), error()).is_err());
        assert!(turn_result(ended(), None).is_ok());
        let cancelled = json!({ "stopReason": "cancelled" });
        assert_eq!(turn_result(Ok(cancelled.clone()), error()).expect("not failed"), cancelled);
    }

    #[test]
    fn the_launcher_is_written_once() {
        let dir = std::env::temp_dir().join(format!("codync-agent-pi-{}", uuid::Uuid::new_v4()));
        let launcher = write_launcher(&dir, "app").expect("written");
        let written = std::fs::metadata(&launcher).and_then(|m| m.modified()).expect("written");
        assert_eq!(write_launcher(&dir, "app").expect("written"), launcher);
        assert_eq!(std::fs::metadata(&launcher).and_then(|m| m.modified()).expect("written"), written);
        assert_eq!(std::fs::read_to_string(dir.join("app.js")).expect("written"), extension("app"));
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 2, "no temporary file is left");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
