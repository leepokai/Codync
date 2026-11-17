//! Isolated Chrome `DevTools` Protocol engine used by browser end-to-end tests.
//!
//! Product browser calls use the installed Codync Browser extension. Keeping the test-only
//! headless engine here prevents product routing and test automation rules from sharing one file.

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use base64::Engine as _;
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::Message;

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 720;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(20);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) port: u16,
    pub(super) profile: PathBuf,
    pub(super) tab_id: String,
    pub(super) pid: u32,
    pub(super) executable: PathBuf,
    pub(super) version: String,
}

pub(super) async fn launch(state_root: &Path) -> Result<Launched> {
    let executable = browser_executable()?;
    let profile = state_root.join("profile");
    std::fs::create_dir_all(&profile).context("creating the isolated browser profile")?;
    let error_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state_root.join("chrome.log"))
        .context("opening the browser log")?;
    let mut command = Command::new(&executable);
    command
        .args([
            "--headless=new",
            "--remote-debugging-port=0",
            "--remote-allow-origins=*",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--disable-extensions",
            "--disable-component-update",
            "--disable-background-networking",
            "--disable-default-apps",
            "--disable-features=Translate,OptimizationHints,MediaRouter",
            "--force-device-scale-factor=1",
            "--window-size=1280,720",
            "about:blank",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(error_log))
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().with_context(|| format!("starting {}", executable.display()))?;
    let setup = async {
        let (port, browser_path) = wait_devtools(&mut child, state_root).await?;
        let browser_ws = format!("ws://127.0.0.1:{port}{browser_path}");
        let version = wait_version(port).await?;
        let created = cdp_command(
            &browser_ws,
            "Target.createTarget",
            json!({
                "url": "about:blank",
                "newWindow": true,
                "width": VIEWPORT_WIDTH,
                "height": VIEWPORT_HEIGHT,
                "background": true
            }),
        )
        .await?;
        let tab_id = created["targetId"].as_str().context("Chrome returned no tab id")?.to_owned();
        let pid = child.id().context("the browser process already exited")?;
        Ok::<_, anyhow::Error>((port, tab_id, pid, version))
    };
    match setup.await {
        Ok((port, tab_id, pid, version)) => Ok(Launched { child, port, profile, tab_id, pid, executable, version }),
        Err(error) => {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(state_root);
            Err(error)
        }
    }
}

pub(super) async fn navigate_tab(ws_url: &str, url: &str) -> Result<Value> {
    let mut cdp = Cdp::connect(ws_url).await?;
    cdp.command("Page.enable", json!({})).await?;
    set_viewport(&mut cdp).await?;
    let navigation = tokio::time::timeout(
        NAVIGATION_TIMEOUT,
        cdp.command("Page.navigate", json!({"url":url, "transitionType":"typed"})),
    )
    .await
    .context("navigation timed out; it was sent once and will not be retried")??;
    ensure!(
        navigation["errorText"].is_null(),
        "navigation failed: {}",
        navigation["errorText"].as_str().unwrap_or("unknown error")
    );
    let ready = cdp
        .command(
            "Runtime.evaluate",
            json!({
                "expression":"new Promise(resolve => { if (document.readyState === 'complete') resolve(true); else addEventListener('load', () => resolve(true), {once:true}); setTimeout(() => resolve(document.readyState === 'complete'), 10000); })",
                "returnByValue":true,
                "awaitPromise":true
            }),
        )
        .await?;
    ensure!(ready["result"]["value"] == true, "the bound tab did not finish loading");
    page_state(ws_url).await
}

pub(super) async fn click(ws_url: &str, selector: &str) -> Result<Value> {
    let selector = serde_json::to_string(selector)?;
    let expression = format!(
        "(() => {{ const el = document.querySelector({selector}); if (!el) return {{found:false}}; \
         el.scrollIntoView({{block:'center', inline:'center'}}); el.click(); return {{found:true}}; }})()"
    );
    let clicked = evaluate(ws_url, &expression).await?;
    ensure!(clicked["found"] == true, "selector did not match a clickable element: {selector}");
    tokio::time::sleep(Duration::from_millis(150)).await;
    page_state(ws_url).await
}

pub(super) async fn type_text(ws_url: &str, selector: &str, text: &str) -> Result<Value> {
    let selector = serde_json::to_string(selector)?;
    let text = serde_json::to_string(text)?;
    let expression = format!(
        "(() => {{ const el = document.querySelector({selector}); if (!el) return {{found:false}}; \
         el.focus(); const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : \
         el instanceof HTMLInputElement ? HTMLInputElement.prototype : null; \
         if (proto) {{ const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set; \
         if (!setter) return {{found:false}}; setter.call(el, {text}); }} \
         else if (el.isContentEditable) el.textContent = {text}; else return {{found:false}}; \
         el.dispatchEvent(new Event('input', {{bubbles:true}})); \
         el.dispatchEvent(new Event('change', {{bubbles:true}})); return {{found:true}}; }})()"
    );
    let typed = evaluate(ws_url, &expression).await?;
    ensure!(typed["found"] == true, "selector did not match an editable element: {selector}");
    page_state(ws_url).await
}

pub(super) async fn screenshot(ws_url: &str) -> Result<(String, Value)> {
    let mut cdp = Cdp::connect(ws_url).await?;
    set_viewport(&mut cdp).await?;
    let result = cdp
        .command("Page.captureScreenshot", json!({"format":"png", "fromSurface":true, "captureBeyondViewport":false}))
        .await?;
    let encoded = result["data"].as_str().context("Chrome returned no screenshot data")?.to_owned();
    let image = image::load_from_memory_with_format(
        &base64::engine::general_purpose::STANDARD.decode(&encoded).context("invalid screenshot data")?,
        image::ImageFormat::Png,
    )
    .context("Chrome returned an invalid PNG")?;
    ensure!(
        image.width() == VIEWPORT_WIDTH && image.height() == VIEWPORT_HEIGHT,
        "screenshot has an unexpected size {}x{}",
        image.width(),
        image.height()
    );
    Ok((encoded, page_state(ws_url).await?))
}

pub(super) async fn page_ws(port: u16, tab_id: &str, generation: &str) -> Result<String> {
    ensure!(uuid::Uuid::parse_str(generation).is_ok(), "browser target changed");
    let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build()?;
    let list: Value = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .await
        .context("reading the bound tab")?
        .error_for_status()?
        .json()
        .await?;
    let target = list
        .as_array()
        .and_then(|targets| targets.iter().find(|target| target["id"] == tab_id))
        .ok_or_else(|| anyhow!("the bound browser tab is gone; refusing to use another tab"))?;
    let ws =
        target["webSocketDebuggerUrl"].as_str().ok_or_else(|| anyhow!("the bound tab has no DevTools endpoint"))?;
    ensure!(
        ws.starts_with("ws://127.0.0.1:") || ws.starts_with("ws://localhost:"),
        "the bound tab has an unexpected DevTools endpoint"
    );
    Ok(ws.to_owned())
}

fn browser_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODYNC_BROWSER_BIN") {
        let path = PathBuf::from(path);
        ensure!(path.is_file(), "CODYNC_BROWSER_BIN does not name a file: {}", path.display());
        return Ok(path);
    }
    #[cfg(windows)]
    let candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    #[cfg(target_os = "macos")]
    let candidates = ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates =
        ["/usr/bin/google-chrome", "/usr/bin/google-chrome-stable", "/usr/bin/chromium", "/usr/bin/chromium-browser"];
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    bail!("Chrome or Chromium was not found; set CODYNC_BROWSER_BIN to an isolated-test browser executable")
}

async fn wait_devtools(child: &mut Child, root: &Path) -> Result<(u16, String)> {
    let port_file = root.join("profile").join("DevToolsActivePort");
    let deadline = Instant::now() + LAUNCH_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            bail!("the browser exited before DevTools opened ({status})");
        }
        if let Ok(text) = tokio::fs::read_to_string(&port_file).await {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next())
                && let Ok(port) = port.parse::<u16>()
                && path.starts_with("/devtools/browser/")
            {
                return Ok((port, path.to_owned()));
            }
        }
        ensure!(Instant::now() < deadline, "the browser DevTools endpoint did not start in time");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_version(port: u16) -> Result<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build()?;
    let deadline = Instant::now() + LAUNCH_TIMEOUT;
    loop {
        if let Ok(response) = client.get(format!("http://127.0.0.1:{port}/json/version")).send().await
            && response.status().is_success()
            && let Ok(value) = response.json::<Value>().await
        {
            return Ok(value["Browser"].as_str().unwrap_or("unknown").to_owned());
        }
        ensure!(Instant::now() < deadline, "the browser DevTools endpoint did not answer in time");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn page_state(ws_url: &str) -> Result<Value> {
    let value = evaluate(
        ws_url,
        "(() => { const fields = [...document.querySelectorAll('input,textarea')].map(el => `${el.id || el.name || 'field'}=${el.value || ''}`); return {url: location.href, title: document.title || '', text: ((document.body?.innerText || '') + (fields.length ? '\\n' + fields.join('\\n') : '')).slice(0, 4096)}; })()",
    )
    .await?;
    Ok(public_state(
        value["url"].as_str().unwrap_or_default(),
        value["title"].as_str().unwrap_or_default(),
        value["text"].as_str().unwrap_or_default(),
    ))
}

fn public_state(url: &str, title: &str, text: &str) -> Value {
    json!({"url":url, "title":title, "text":text})
}

async fn set_viewport(cdp: &mut Cdp) -> Result<()> {
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width":VIEWPORT_WIDTH,
            "height":VIEWPORT_HEIGHT,
            "deviceScaleFactor":1,
            "mobile":false
        }),
    )
    .await?;
    Ok(())
}

async fn evaluate(ws_url: &str, expression: &str) -> Result<Value> {
    let mut cdp = Cdp::connect(ws_url).await?;
    let reply = cdp
        .command("Runtime.evaluate", json!({"expression":expression, "returnByValue":true, "awaitPromise":true}))
        .await?;
    ensure!(reply["exceptionDetails"].is_null(), "page script failed: {}", reply["exceptionDetails"]);
    Ok(reply["result"]["value"].clone())
}

struct Cdp {
    socket: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: u64,
}

impl Cdp {
    async fn connect(ws_url: &str) -> Result<Self> {
        let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(ws_url))
            .await
            .context("connecting to the isolated browser timed out")?
            .context("connecting to the isolated browser")?;
        Ok(Self { socket, next_id: 1 })
    }

    async fn command(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.socket
            .send(Message::Text(json!({"id":id, "method":method, "params":params}).to_string().into()))
            .await
            .with_context(|| format!("sending {method}"))?;
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            ensure!(!left.is_zero(), "{method} timed out");
            let message = tokio::time::timeout(left, self.socket.next())
                .await
                .with_context(|| format!("{method} timed out"))?
                .ok_or_else(|| anyhow!("{method}: browser connection closed"))?
                .with_context(|| format!("{method}: browser connection failed"))?;
            match message {
                Message::Text(text) => {
                    let reply: Value =
                        serde_json::from_str(&text).with_context(|| format!("{method}: invalid reply"))?;
                    if reply["id"].as_u64() != Some(id) {
                        continue;
                    }
                    if !reply["error"].is_null() {
                        let detail = reply["error"]["message"].as_str().unwrap_or("unknown DevTools error");
                        bail!("{method}: {detail}");
                    }
                    return Ok(reply["result"].clone());
                }
                Message::Binary(bytes) => {
                    let Ok(reply) = serde_json::from_slice::<Value>(&bytes) else { continue };
                    if reply["id"].as_u64() != Some(id) {
                        continue;
                    }
                    if !reply["error"].is_null() {
                        let detail = reply["error"]["message"].as_str().unwrap_or("unknown DevTools error");
                        bail!("{method}: {detail}");
                    }
                    return Ok(reply["result"].clone());
                }
                Message::Ping(data) => self.socket.send(Message::Pong(data)).await?,
                Message::Close(_) => bail!("{method}: browser connection closed"),
                Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    }
}

async fn cdp_command(ws_url: &str, method: &str, params: Value) -> Result<Value> {
    Cdp::connect(ws_url).await?.command(method, params).await
}
