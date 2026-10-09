//! The computer-use driver bots act through: cua-driver (MIT, <https://github.com/trycua/cua>),
//! pinned in `packaging/cua-driver/driver.json` and shipped beside the host (macOS: inside
//! Codync Screen). It works on apps in the background, by accessibility element where it can.
//!
//! One daemon serves every bot, started on first use and stopped with whoever started it. On
//! macOS Codync Screen starts it (`driver` request), so macOS counts its Accessibility and
//! Screen Recording use as Codync Screen's and the grants the user gave stay the only ones; on
//! Windows and Linux the host starts it. Requests use the daemon's line protocol: one JSON object
//! per line, `{method: "call", name, args, session_id, client_kind}` → `{ok, result | error}`.

use super::Screen;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// The tools bots get (`packaging/cua-driver/tools.mjs`), listed as-is when the driver can't start.
static TOOLS: LazyLock<Vec<Value>> =
    LazyLock::new(|| serde_json::from_str(include_str!("cua_tools.json")).expect("cua_tools.json is valid JSON"));
/// The longest one call may take: the MCP bridge gives up at 60 s.
const CALL_TIMEOUT: Duration = Duration::from_secs(55);
/// Content text shorter than this is a summary; the data is then only in `structuredContent`.
const SUMMARY_LEN: usize = 400;

pub(super) fn offered(name: &str) -> bool {
    TOOLS.iter().any(|t| t["name"] == name)
}

pub(super) fn read_only(name: &str) -> bool {
    TOOLS.iter().any(|t| t["name"] == name && t["annotations"]["readOnlyHint"] == true)
}

/// The daemon's own descriptions for this platform, for the tools Codync offers.
pub(super) async fn tools(screen: &Screen) -> Vec<Value> {
    let live = async {
        let list =
            request(&screen.driver.endpoint(screen).await?, json!({"method": "list", "client_kind": "cli"})).await?;
        let tools = list["tools"].as_array().context("the driver listed no tools")?;
        anyhow::Ok(
            tools
                .iter()
                .filter(|t| t["name"].as_str().is_some_and(offered))
                .map(|t| {
                    json!({
                        "name": t["name"],
                        "description": t["description"],
                        "inputSchema": t["input_schema"],
                        "annotations": {
                            "readOnlyHint": t["read_only"],
                            "destructiveHint": t["destructive"],
                            "idempotentHint": t["idempotent"],
                            "openWorldHint": t["open_world"],
                        },
                    })
                })
                .collect::<Vec<_>>(),
        )
    };
    match live.await {
        Ok(tools) if !tools.is_empty() => tools,
        Ok(_) => TOOLS.clone(),
        Err(e) => {
            tracing::debug!(error = format!("{e:#}"), "listing the stored driver tools");
            TOOLS.clone()
        }
    }
}

/// Runs one driver tool for `bot`: `{content, isError?}` for the agent.
pub(super) async fn call(screen: &Screen, bot: &str, name: &str, args: Value) -> Result<Value> {
    let body = json!({"method": "call", "name": name, "args": args, "session_id": format!("codync-{bot}"), "client_kind": "cli"});
    let mut result = request(&screen.driver.endpoint(screen).await?, body).await?;
    let mut out = json!({"content": agent_content(&mut result)});
    if result["isError"] == true {
        out["isError"] = true.into();
    }
    Ok(out)
}

/// The tool result as the agent reads it. Most tools put their data in `structuredContent` and
/// only a summary in `content`, which some agents never look past, so a short summary gets the
/// data as text. Window snapshots spell out their element tokens (`<snapshot_id>:<index>`).
fn agent_content(result: &mut Value) -> Value {
    let mut content = match result["content"].take() {
        Value::Array(c) => c,
        _ => vec![],
    };
    let structured = &result["structuredContent"];
    let text_len: usize = content.iter().filter_map(|c| c["text"].as_str()).map(str::len).sum();
    if let Some(id) = structured["snapshot_id"].as_str() {
        let line = format!("element_token for element [N] in this snapshot: {id}:N");
        content.push(json!({"type": "text", "text": line}));
    } else if text_len < SUMMARY_LEN && structured.as_object().is_some_and(|s| !s.is_empty()) {
        content.push(json!({"type": "text", "text": structured.to_string()}));
    }
    Value::Array(content)
}

/// One request on the daemon's line protocol.
async fn request(endpoint: &str, body: Value) -> Result<Value> {
    let exchange = async {
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(endpoint).await?;
        #[cfg(windows)]
        let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint)?;
        exchange(stream, &body).await
    };
    let reply = tokio::time::timeout(CALL_TIMEOUT, exchange)
        .await
        .map_err(|_| anyhow!("the computer-use driver didn't answer in time"))?
        .context("can't reach the computer-use driver")?;
    if reply["ok"] == true {
        Ok(reply["result"].clone())
    } else {
        bail!("{}", reply["error"].as_str().unwrap_or("the computer-use driver refused the request"))
    }
}

async fn exchange(stream: impl AsyncRead + AsyncWrite + Unpin, body: &Value) -> Result<Value> {
    let mut stream = BufReader::new(stream);
    let mut line = serde_json::to_vec(body)?;
    line.push(b'\n');
    stream.get_mut().write_all(&line).await?;
    let mut reply = String::new();
    stream.read_line(&mut reply).await?;
    serde_json::from_str(&reply).context("the computer-use driver sent no JSON")
}

/// Starts the daemon when needed and says where it listens.
#[derive(Default)]
pub(super) struct Driver {
    #[cfg(not(target_os = "macos"))]
    daemon: tokio::sync::Mutex<Option<(tokio::process::Child, String)>>,
}

impl Driver {
    /// macOS: Codync Screen runs the daemon, so its grants are the ones that count.
    #[cfg(target_os = "macos")]
    async fn endpoint(&self, screen: &Screen) -> Result<String> {
        let res = screen.link()?.request("driver", json!({})).await?;
        res["socket"].as_str().map(str::to_owned).context("Codync Screen didn't start the computer-use driver")
    }

    #[cfg(not(target_os = "macos"))]
    async fn endpoint(&self, _screen: &Screen) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        if let Some((child, endpoint)) = daemon.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            return Ok(endpoint.clone());
        }
        let started = start().await?;
        let endpoint = started.1.clone();
        *daemon = Some(started);
        Ok(endpoint)
    }
}

/// The driver installed beside the host (`packaging/cua-driver/fetch.mjs`).
#[cfg(not(target_os = "macos"))]
pub(super) fn executable() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) { "cua-driver.exe" } else { "cua-driver" };
    std::env::current_exe().ok()?.parent().map(|dir| dir.join(name)).filter(|p| p.is_file())
}

/// Linux sessions the driver works on in the background: X11 and Sway. Elsewhere (GNOME, KDE
/// and other Wayland desktops) bots use Codync Screen's portal input instead.
#[cfg(target_os = "linux")]
pub(super) fn linux_session() -> bool {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
    std::env::var_os("SWAYSOCK").is_some() || (!wayland && x_display().is_some())
}

/// `$DISPLAY`, or the first X server's when a service started without one.
#[cfg(target_os = "linux")]
fn x_display() -> Option<String> {
    std::env::var("DISPLAY").ok().filter(|d| !d.is_empty()).or_else(|| {
        let mut servers: Vec<String> = std::fs::read_dir("/tmp/.X11-unix")
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.strip_prefix('X').map(|n| format!(":{n}")))
            .collect();
        servers.sort();
        servers.into_iter().next()
    })
}

/// Spawns `cua-driver serve` and waits until it listens. It exits when the host does: its stdin
/// closes then (`CUA_DRIVER_PARENT_LIVENESS_STDIN`), crash or not.
#[cfg(not(target_os = "macos"))]
async fn start() -> Result<(tokio::process::Child, String)> {
    let exe = executable().context("The computer-use driver isn't installed next to codync-host. Reinstall Codync.")?;
    #[cfg(windows)]
    let endpoint = endpoint_name();
    #[cfg(target_os = "linux")]
    let endpoint = endpoint_name()?;
    let log = std::fs::File::create(crate::service::data_dir().join("cua-driver.log"))?;
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.args(["serve", "--socket", &endpoint])
        .env("CUA_DRIVER_EMBEDDED", "1")
        .env("CUA_DRIVER_PARENT_LIVENESS_STDIN", "1")
        .env("CUA_DRIVER_RS_TELEMETRY_ENABLED", "0")
        .env("CUA_DRIVER_RS_UPDATE_CHECK", "0")
        .stdin(std::process::Stdio::piped())
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true);
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("SWAYSOCK").is_some() {
            cmd.env("CUA_DRIVER_RS_ENABLE_WAYLAND", "1");
        } else if let Some(display) = x_display() {
            cmd.env("DISPLAY", display);
        }
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().with_context(|| format!("can't start {}", exe.display()))?;
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            bail!("the computer-use driver exited ({status}); see cua-driver.log in Codync's data folder");
        }
        if request(&endpoint, json!({"method": "list", "client_kind": "cli"})).await.is_ok() {
            tracing::info!(endpoint, "computer-use driver started");
            return Ok((child, endpoint));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    bail!("the computer-use driver didn't start listening")
}

/// A socket in a directory only this user can open (whoever reaches it can drive the desktop),
/// short enough for `sun_path`: under the login session's runtime dir, else Codync's data dir,
/// both private to this user already.
#[cfg(target_os = "linux")]
fn endpoint_name() -> Result<String> {
    use std::os::unix::fs::PermissionsExt;
    let parent = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(crate::service::data_dir, std::path::PathBuf::from);
    let folder = match crate::environment::Environment::current() {
        crate::environment::Environment::Main => "codync-cua",
        crate::environment::Environment::Dev => "codync-dev-cua",
    };
    let dir = parent.join(folder);
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let socket = dir.join("cua.sock");
    let _ = std::fs::remove_file(&socket);
    Ok(socket.to_string_lossy().into_owned())
}

/// A random pipe name: Windows lets another process add an instance to a name it can guess.
#[cfg(windows)]
fn endpoint_name() -> String {
    format!(r"\\.\pipe\codync-cua-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_tools_say_which_only_look() {
        assert!(offered("click") && !read_only("click"));
        assert!(read_only("get_window_state") && read_only("list_windows"));
        assert!(!offered("bring_to_front") && !offered("kill_app") && !offered("clipboard_read"));
    }

    #[test]
    fn summaries_get_their_data_and_snapshots_their_tokens() {
        let mut r = json!({"content": [{"type": "text", "text": "Found 2 window(s)."}], "structuredContent": {"windows": [1, 2]}});
        assert_eq!(agent_content(&mut r)[1]["text"], r#"{"windows":[1,2]}"#);
        let mut r = json!({"content": [{"type": "image", "data": "x"}, {"type": "text", "text": "- [0] AXWindow"}], "structuredContent": {"snapshot_id": "s00000002", "elements": []}});
        let c = agent_content(&mut r);
        assert_eq!(c.as_array().unwrap().len(), 3);
        assert!(c[2]["text"].as_str().unwrap().ends_with("s00000002:N"));
        let long = "x".repeat(SUMMARY_LEN);
        let mut r = json!({"content": [{"type": "text", "text": long}], "structuredContent": {"apps": []}});
        assert_eq!(agent_content(&mut r).as_array().unwrap().len(), 1, "the text already has the data");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn line_protocol_roundtrip() {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        tokio::spawn(async move {
            let mut b = BufReader::new(b);
            let mut line = String::new();
            b.read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let reply = json!({"ok": true, "result": {"echo": req["name"]}});
            b.get_mut().write_all(format!("{reply}\n").as_bytes()).await.unwrap();
        });
        let reply = exchange(a, &json!({"method": "call", "name": "list_windows"})).await.unwrap();
        assert_eq!(reply["result"]["echo"], "list_windows");
    }
}
