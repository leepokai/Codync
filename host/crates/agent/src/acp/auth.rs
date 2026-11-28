//! Signing in the ACP way: the methods an agent advertises (`authMethods` in its `initialize` answer) and the
//! login it reports (`_auth/status_update`). Methods come in three kinds:
//! - terminal: a command (the agent's `_meta.terminal-auth`, or its own command with `args`) to run in a terminal;
//! - agent: the agent signs itself in on `authenticate` (usually a browser on this computer);
//! - env var: keys the user types, set in the agent's environment on every launch.

use super::{Incoming, text};
use crate::backends;
use crate::registry::env_prefix;
use crate::shell::quote as shell_quote;
use serde::Serialize;
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MethodKind {
    /// Shell command for a setup terminal. Never sent to clients.
    Terminal {
        #[serde(skip)]
        command: String,
    },
    Agent,
    EnvVar {
        vars: Vec<EnvVar>,
        link: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub label: String,
    pub secret: bool,
    pub optional: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(flatten)]
    pub kind: MethodKind,
}

/// The agent's first `_auth/status_update` (ACP `authStatus` extension): `kind: "none"` is signed
/// out, any other kind signed in. `None`: it can't tell (it stays silent then).
pub async fn reported_login(inbox: &mut mpsc::UnboundedReceiver<Incoming>) -> Option<bool> {
    // Claude's adapter gives its `claude auth status` 5 s.
    let report = async {
        while let Some(incoming) = inbox.recv().await {
            if let Incoming::Notification { method, params } = incoming
                && method == "_auth/status_update"
                && let Some(kind) = params["authStatus"]["kind"].as_str()
            {
                return Some(kind != "none");
            }
        }
        None
    };
    tokio::time::timeout(Duration::from_secs(8), report).await.ok().flatten()
}

fn joined_args(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(shell_quote).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// The sign-in methods the agent advertises in its `initialize` answer `init`. `program` is the command that
/// started it, which a terminal method runs with its own arguments.
pub fn methods(backend: &str, init: &Value, program: &str) -> Vec<Method> {
    let empty = Map::new();
    let mut out = init["authMethods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = text(&m["id"])?;
            let name = text(&m["name"]).unwrap_or_else(|| id.clone());
            let meta = m["_meta"].as_object().unwrap_or(&empty);
            let kind = if m["type"] == "env_var" {
                let vars = m["vars"]
                    .as_array()?
                    .iter()
                    .filter_map(|v| {
                        let name = text(&v["name"])?;
                        crate::shell::is_env_name(&name).then(|| EnvVar {
                            label: text(&v["label"]).unwrap_or_else(|| name.clone()),
                            secret: v["secret"].as_bool().unwrap_or(true),
                            optional: v["optional"].as_bool().unwrap_or(false),
                            name,
                        })
                    })
                    .collect::<Vec<_>>();
                if vars.is_empty() {
                    return None;
                }
                MethodKind::EnvVar { vars, link: text(&m["link"]) }
            } else if let Some(t) = meta.get("terminal-auth").filter(|t| t["command"].is_string()) {
                let program = t["command"].as_str().unwrap_or_default();
                // An adapter pointing at another harness's CLI is a copy-paste slip (Kilo → `opencode`).
                if backends::belongs_to_other(backend, program) {
                    return None;
                }
                let command = format!("{}{} {}", env_prefix(&t["env"]), shell_quote(program), joined_args(&t["args"]));
                MethodKind::Terminal { command }
            } else if m["type"] == "terminal" || meta.get("type").is_some_and(|t| t == "terminal") {
                let args = if m["args"].is_array() { &m["args"] } else { meta.get("args").unwrap_or(&Value::Null) };
                let env = if m["env"].is_object() { &m["env"] } else { meta.get("env").unwrap_or(&Value::Null) };
                MethodKind::Terminal { command: format!("{}{} {}", env_prefix(env), program, joined_args(args)) }
            } else if meta.contains_key("api-key") || meta.contains_key("gateway") {
                // Keys the agent itself asks for through a UI we don't have.
                return None;
            } else {
                MethodKind::Agent
            };
            Some(Method { id, name, description: text(&m["description"]), kind })
        })
        .collect::<Vec<_>>();
    // Some agents list a key twice: prose-only ("set Z_AI_API_KEY") and as a real env-var method.
    let keyed: Vec<String> =
        out.iter().filter(|m| matches!(m.kind, MethodKind::EnvVar { .. })).map(|m| m.name.clone()).collect();
    out.retain(|m| !matches!(m.kind, MethodKind::Agent) || !keyed.contains(&m.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROGRAM: &str = "npx -y pkg@1";

    #[test]
    fn reads_every_kind_of_method() {
        let init = json!({"authMethods": [
            {"id": "a", "name": "Terminal meta", "_meta": {"terminal-auth": {"command": "/bin/tool", "args": ["login", "--x y"]}}},
            {"id": "b", "name": "Terminal args", "type": "terminal", "args": ["--terminal-login"]},
            {"id": "c", "name": "Key", "type": "env_var", "link": "https://k", "vars": [{"name": "Z_KEY", "label": "Key"}, {"name": "bad name"}]},
            {"id": "d", "name": "Browser"},
            {"id": "e", "name": "Api key", "_meta": {"api-key": {"provider": "openai"}}},
            {"id": "f", "name": "Qwen style", "_meta": {"type": "terminal", "args": ["--auth-type=openai"]}},
            {"id": "g", "name": "Key"},
        ]});
        let m = methods("x", &init, PROGRAM);
        let kinds: Vec<_> = m.iter().map(|m| (m.id.as_str(), &m.kind)).collect();
        assert!(
            matches!(kinds[0], ("a", MethodKind::Terminal { command }) if command == &format!("/bin/tool login {}", shell_quote("--x y")))
        );
        assert!(
            matches!(kinds[1], ("b", MethodKind::Terminal { command }) if command == "npx -y pkg@1 --terminal-login")
        );
        assert!(
            matches!(kinds[2], ("c", MethodKind::EnvVar { vars, .. }) if vars.len() == 1 && vars[0].name == "Z_KEY")
        );
        assert!(matches!(kinds[3], ("d", MethodKind::Agent)));
        assert!(
            matches!(kinds[4], ("f", MethodKind::Terminal { command }) if command == "npx -y pkg@1 --auth-type=openai")
        );
        assert_eq!(m.len(), 5);
        // Commands stay on the host.
        assert!(serde_json::to_value(&m[0]).unwrap().get("command").is_none());
    }

    #[tokio::test]
    async fn takes_the_first_login_the_agent_reports() {
        let report = |kind: &str| Incoming::Notification {
            method: "_auth/status_update".into(),
            params: json!({"authStatus": {"kind": kind, "label": "x"}}),
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(Incoming::Update(crate::events::Update::parse(&json!({})))).unwrap();
        tx.send(report("none")).unwrap();
        tx.send(report("account")).unwrap();
        assert_eq!(reported_login(&mut rx).await, Some(false));
        assert_eq!(reported_login(&mut rx).await, Some(true));
        drop(tx);
        assert_eq!(reported_login(&mut rx).await, None, "an agent that goes quiet can't tell");
    }

    #[test]
    fn skips_terminal_commands_of_another_harness() {
        let init = json!({"authMethods": [{"id": "k", "name": "Kilo", "_meta": {"terminal-auth": {"command": "opencode", "args": ["auth", "login"]}}}]});
        assert!(methods("kilo", &init, PROGRAM).is_empty());
        assert_eq!(methods("opencode", &init, PROGRAM).len(), 1);
    }

    #[test]
    fn antigravity_terminal_login_runs_through_acp_subcommand() {
        let init = json!({"authMethods": [
            {"id": "oauth-personal", "name": "Log in with Google"},
            {"id": "terminal-login", "name": "Log in from the terminal", "type": "terminal", "args": ["--login"]}
        ]});
        let program = "/usr/local/bin/agy acp --app-data-dir antigravity-acp --gemini-dir /tmp/slot";
        let found = methods("antigravity", &init, program);
        assert!(matches!(&found[0].kind, MethodKind::Agent));
        assert!(
            matches!(&found[1].kind, MethodKind::Terminal { command } if command == &format!("{program} --login")),
            "{found:?}"
        );
    }
}
