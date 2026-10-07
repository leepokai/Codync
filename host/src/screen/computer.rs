//! Bots operating the computer: the `computer` MCP tools, who is in control, and signing in.
//! Tools run on the computer-use driver (`cua.rs`); on Linux desktops it doesn't support, on
//! Codync Screen's portal input (`helper_tools.rs`).

#[cfg(target_os = "linux")]
use super::helper_tools;
use super::{AGENT_HOLD, Screen, cua};
use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;

const TYPE_LOGIN: &str = "type_login";

/// What runs the tools on this computer.
#[derive(Clone, Copy)]
enum Backend {
    Driver,
    #[cfg(target_os = "linux")]
    Helper,
}

#[cfg(not(target_os = "linux"))]
fn backend() -> Backend {
    Backend::Driver
}

#[cfg(target_os = "linux")]
fn backend() -> Backend {
    if cua::linux_session() && cua::executable().is_some() { Backend::Driver } else { Backend::Helper }
}

impl Screen {
    /// Whether bots may use the computer here: Windows needs nothing else; macOS and Linux run
    /// their part through Codync Screen, which runs while Remote screen is on.
    pub(super) fn computer_use(&self) -> bool {
        cfg!(windows) || self.enabled()
    }

    /// Marks `bot` as the one using the computer. Returns whether that's news.
    fn claim(&self, bot: &str, read_only: bool) -> Result<bool> {
        let mut c = self.control.locked();
        if c.user && !read_only {
            bail!(
                "The user has taken over the screen from their phone. Wait until they hand it back, or ask them what to do."
            );
        }
        if let Some(other) = c.active_agent()
            && other != bot
        {
            bail!("Another bot is using the computer right now. Try again in a minute.");
        }
        let fresh = c.active_agent().is_none();
        c.agent = Some((bot.to_owned(), Instant::now()));
        Ok(fresh)
    }
}

/// The `computer` MCP tools on this computer.
pub async fn computer_tools(hub: &Arc<Hub>) -> Value {
    let screen = &hub.screen;
    let mut tools = match backend() {
        Backend::Driver => cua::tools(screen).await,
        #[cfg(target_os = "linux")]
        Backend::Helper => helper_tools::tools(),
    };
    if cfg!(not(windows)) {
        tools.push(type_login_tool());
    }
    json!({"tools": tools})
}

/// Runs one `computer` tool for `bot`: `{content, isError?}` for the agent.
pub async fn computer(hub: &Arc<Hub>, bot: &str, name: &str, args: Value) -> Result<Value> {
    let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    if !row.config.computer {
        bail!("Computer use is turned off for this bot.");
    }
    let screen = &hub.screen;
    if !screen.computer_use() {
        bail!("Remote screen is turned off on this computer. Turn it on in Codync's menu there.");
    }
    let backend = backend();
    let read_only = match backend {
        _ if name == TYPE_LOGIN => false,
        Backend::Driver if !cua::offered(name) => bail!("unknown computer tool `{name}`"),
        Backend::Driver => cua::read_only(name),
        #[cfg(target_os = "linux")]
        Backend::Helper => helper_tools::read_only(name),
    };
    if screen.claim(bot, read_only)? {
        screen.emit();
        let screen = screen.clone();
        tokio::spawn(async move {
            // Say when the bot stops, so phones drop the live chip.
            loop {
                tokio::time::sleep(AGENT_HOLD).await;
                if screen.control.locked().active_agent().is_none() {
                    screen.emit();
                    return;
                }
            }
        });
    }
    if name == TYPE_LOGIN {
        return type_login(hub, bot, backend, args).await;
    }
    match backend {
        Backend::Driver => cua::call(screen, bot, name, args).await,
        #[cfg(target_os = "linux")]
        Backend::Helper => helper_tools::run(screen, name, args).await,
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum LoginField {
    Username,
    Password,
}

#[derive(serde::Deserialize)]
struct LoginArgs {
    login: String,
    field: LoginField,
    pid: Option<i64>,
}

fn type_login_tool() -> Value {
    json!({
        "name": TYPE_LOGIN,
        "description": "Type a saved login (see list_logins / request_login) into an app's focused sign-in field: click the field first, then pass that app's `pid`. Works only when the field belongs to the login's website or app; `password` only into a password field. You never see the value.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "login": {"type": "string", "description": "The login's id."},
                "field": {"type": "string", "enum": ["username", "password"]},
                "pid": {"type": "integer", "description": "The app with the sign-in field (from list_windows); the frontmost app when left out."},
            },
            "required": ["login", "field"],
        },
    })
}

/// Types a saved login only into its own site or app, and a password only into a password
/// field, so a misled bot can't paste it into a page or chat that would show it. Codync Screen
/// checks the focused field (accessibility): which app, which web page, whether it's secure.
/// Windows has no Codync Screen to check it yet.
async fn type_login(hub: &Arc<Hub>, bot: &str, backend: Backend, args: Value) -> Result<Value> {
    if cfg!(windows) {
        bail!("type_login isn't available on Windows yet. Ask the user to sign in themselves.");
    }
    let a: LoginArgs = serde_json::from_value(args).map_err(|e| anyhow!("invalid type_login call: {e}"))?;
    let login = crate::market::logins::get(&hub.store, &a.login)?;
    let screen = &hub.screen;
    let focus = screen.link()?.request("focusedField", json!({"pid": a.pid})).await?;
    let app = focus["app"].as_str().unwrap_or_default();
    let url = focus["url"].as_str();
    if !crate::market::logins::matches(&login.site, url, app) {
        bail!(
            "The focused field is in {}, not {}. Open {} and click its sign-in field first.",
            url.unwrap_or(app),
            login.site,
            login.site
        );
    }
    let text = match a.field {
        LoginField::Username => login.username,
        LoginField::Password => {
            if focus["secure"] != true {
                bail!("Click into the password field first; type_login types a password only into a password field.");
            }
            crate::market::passwords::resolve(&hub.store, &login.password).await?
        }
    };
    match backend {
        Backend::Driver => {
            // macOS: into the checked app, in the background; Linux: the active window it checked.
            let args = if cfg!(target_os = "macos") {
                json!({"pid": focus["pid"], "text": text})
            } else {
                json!({"scope": "desktop", "text": text})
            };
            // The driver's answer may quote what it typed: the agent gets only the outcome.
            let res = cua::call(screen, bot, "type_text", args).await?;
            if res["isError"] == true {
                bail!("Couldn't type into {app}'s field. Click it again and retry.");
            }
            Ok(
                json!({"content": [{"type": "text", "text": format!("Typed the saved value into {app}. Take a fresh look to check it landed.")}]}),
            )
        }
        #[cfg(target_os = "linux")]
        Backend::Helper => helper_tools::type_text(screen, text).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[test]
    fn user_takeover_blocks_bot_actions_but_not_looking() {
        let (tx, _) = broadcast::channel(8);
        let s = Screen::new(Some(true), tx);
        assert!(s.claim("a", false).unwrap());
        assert!(!s.claim("a", false).unwrap());
        assert!(s.claim("b", true).is_err(), "one bot at a time");
        s.takeover(true);
        assert!(s.claim("a", false).is_err());
        assert!(s.claim("a", true).is_ok());
    }

    #[test]
    fn type_login_names_its_target() {
        let t = type_login_tool();
        assert_eq!(t["inputSchema"]["required"], json!(["login", "field"]));
        assert!(t["inputSchema"]["properties"]["pid"].is_object());
    }
}
