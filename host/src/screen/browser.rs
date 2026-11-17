//! Dedicated browser tabs driven by the Codync Browser extension, with an isolated Chrome `DevTools`
//! Protocol engine retained for automated tests.
//!
//! The browser is part of Codync's existing `computer` MCP server, so it uses the same bot permission,
//! single-agent claim, and user-takeover guards. Each browser belongs to the user message that started
//! the turn. Screenshots are published through `send_file`, which puts the picture in that same chat.

use super::browser_extension::{
    EXTENSION_ID, ExtensionBridge, TAKEOVER_ERROR, failure_category as extension_failure_category,
};
use super::browser_headless;
use crate::hub::{BotStatus, Hub};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use base64::Engine as _;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::process::Child;
use tokio::sync::Mutex;
use url::Url;

const START: &str = "browser_start";
const NAVIGATE: &str = "browser_navigate";
const CLICK: &str = "browser_click";
const TYPE: &str = "browser_type";
const SCREENSHOT: &str = "browser_screenshot";
pub(super) const STOP: &str = "browser_stop";
const NAMES: [&str; 6] = [START, NAVIGATE, CLICK, TYPE, SCREENSHOT, STOP];

const MAX_URL: usize = 8 * 1024;
const MAX_SELECTOR: usize = 512;
const MAX_TYPE: usize = 16 * 1024;
const MAX_SCREENSHOT: usize = 20 * 1024 * 1024;
const EXTENSION_SESSION_CLOSED: &str = "browser_session_closed:";

/// Whether `name` belongs to this module's tools.
pub(super) fn offered(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Tool descriptions for the existing `computer` MCP server.
pub(super) fn tools() -> Vec<Value> {
    let binding = || {
        json!({
            "operationId": {"type": "string", "description": "The operationId returned by browser_start."},
            "browserId": {"type": "string", "description": "The browserId returned by browser_start."},
            "tabId": {"type": "string", "description": "The exact tabId returned by browser_start."},
            "generation": {"type": "string", "description": "The target generation returned by browser_start."},
        })
    };
    let binding_required = || json!(["operationId", "browserId", "tabId", "generation"]);
    vec![
        json!({
            "name": START,
            "description": "Start a dedicated foreground tab through the Codync Browser extension on this exact computer and bind it to this user turn. If the extension is absent, the call fails instead of switching browser or computer. Returns operationId, browserId, tabId, generation, the original messageId, browser identity and page state. Repeating start with the same URL for the same message returns the same binding.",
            "inputSchema": {
                "type": "object",
                "properties": {"url": {"type": "string", "description": "An absolute http or https URL."}},
                "required": ["url"]
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": true}
        }),
        json!({
            "name": NAVIGATE,
            "description": "Navigate the one tab bound to this operation. The targetId and generation must still match browser_start.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operationId": binding()["operationId"],
                    "browserId": binding()["browserId"],
                    "tabId": binding()["tabId"],
                    "generation": binding()["generation"],
                    "url": {"type": "string", "description": "An absolute http or https URL."}
                },
                "required": ["operationId", "browserId", "tabId", "generation", "url"]
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false}
        }),
        json!({
            "name": CLICK,
            "description": "Click one element by CSS selector in the bound tab through the real browser input path. It runs once; a timeout or connection loss is reported instead of being retried.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operationId": binding()["operationId"],
                    "browserId": binding()["browserId"],
                    "tabId": binding()["tabId"],
                    "generation": binding()["generation"],
                    "selector": {"type": "string", "description": "A CSS selector for exactly one element."}
                },
                "required": ["operationId", "browserId", "tabId", "generation", "selector"]
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false}
        }),
        json!({
            "name": TYPE,
            "description": "Focus one input, textarea or contenteditable element in the bound tab and send real browser text input. It runs once; a timeout or connection loss is reported instead of being retried.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operationId": binding()["operationId"],
                    "browserId": binding()["browserId"],
                    "tabId": binding()["tabId"],
                    "generation": binding()["generation"],
                    "selector": {"type": "string", "description": "A CSS selector for exactly one editable element."},
                    "text": {"type": "string", "description": "Text to enter."}
                },
                "required": ["operationId", "browserId", "tabId", "generation", "selector", "text"]
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false}
        }),
        json!({
            "name": SCREENSHOT,
            "description": "Capture the bound tab and publish the PNG to the user's same chat through the existing file card. Returns the exact URL, title, visible text, tab identity, dimensions, byte count and SHA-256.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operationId": binding()["operationId"],
                    "browserId": binding()["browserId"],
                    "tabId": binding()["tabId"],
                    "generation": binding()["generation"]
                },
                "required": binding_required()
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false}
        }),
        json!({
            "name": STOP,
            "description": "Close the tab bound to this operation. This works while the user has taken over the screen so the bot can clean up.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operationId": binding()["operationId"],
                    "browserId": binding()["browserId"],
                    "tabId": binding()["tabId"],
                    "generation": binding()["generation"]
                },
                "required": binding_required()
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": true}
        }),
    ]
}

#[derive(Default)]
pub(super) struct Browsers {
    state: Mutex<State>,
    start_lock: Mutex<()>,
    extension: ExtensionBridge,
    takeover_epoch: AtomicU64,
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, Session>,
    cancelled_epochs: HashMap<String, u64>,
}

struct Session {
    child: Option<Child>,
    port: u16,
    profile: PathBuf,
    binding: Binding,
    screenshots: u64,
    engine: Engine,
    extension_version: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    Extension,
    Headless,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Binding {
    host_id: String,
    bot_id: String,
    message_id: String,
    operation_id: String,
    browser_id: String,
    tab_id: String,
    generation: String,
    browser_pid: u32,
    browser_executable: String,
    browser_version: String,
    initial_url: String,
    current_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundArgs {
    operation_id: String,
    browser_id: String,
    tab_id: String,
    generation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NavigateArgs {
    #[serde(flatten)]
    bound: BoundArgs,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClickArgs {
    #[serde(flatten)]
    bound: BoundArgs,
    selector: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeArgs {
    #[serde(flatten)]
    bound: BoundArgs,
    selector: String,
    text: String,
}

#[derive(Deserialize)]
struct StartArgs {
    url: String,
}

impl BoundArgs {
    fn from_binding(binding: &Binding) -> Self {
        Self {
            operation_id: binding.operation_id.clone(),
            browser_id: binding.browser_id.clone(),
            tab_id: binding.tab_id.clone(),
            generation: binding.generation.clone(),
        }
    }
}

impl Browsers {
    pub(super) fn revoke_for_takeover(&self) {
        self.takeover_epoch.fetch_add(1, Ordering::Release);
        self.extension.revoke_for_takeover();
    }

    fn takeover_guard(&self, hub: &Hub, epoch: u64) -> Result<()> {
        ensure!(self.takeover_epoch.load(Ordering::Acquire) == epoch && !hub.screen.user_control(), "{TAKEOVER_ERROR}");
        Ok(())
    }

    pub(super) async fn serve_extension(&self, host_id: &str, socket: axum::extract::ws::WebSocket) -> Result<()> {
        self.extension.serve(socket, host_id).await
    }

    pub(super) fn call<'a>(
        &'a self,
        hub: &'a Arc<Hub>,
        bot: &'a str,
        name: &'a str,
        args: Value,
    ) -> BoxFuture<'a, Result<Value>> {
        Box::pin(async move {
            let started = Instant::now();
            let result = match name {
                START => Box::pin(self.start(hub, bot, args)).await,
                NAVIGATE => self.navigate(hub, bot, args).await,
                CLICK => self.click(hub, bot, args).await,
                TYPE => self.r#type(hub, bot, args).await,
                SCREENSHOT => self.screenshot(hub, bot, args).await,
                STOP => self.stop(bot, args).await,
                _ => bail!("unknown browser tool `{name}`"),
            };
            let action = match name {
                START => "start",
                NAVIGATE => "navigate",
                CLICK => "click",
                TYPE => "type",
                SCREENSHOT => "screenshot",
                STOP => "stop",
                _ => "other",
            };
            let mut properties = serde_json::Map::from_iter([
                ("action".into(), action.into()),
                ("outcome".into(), if result.is_ok() { "success" } else { "failed" }.into()),
                ("duration_ms".into(), u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX).into()),
            ]);
            if let Err(error) = &result {
                properties.insert("error".into(), extension_failure_category(error).into());
            }
            crate::analytics::capture(hub, crate::analytics::Event::BrowserAction, properties);
            result
        })
    }

    async fn start(&self, hub: &Arc<Hub>, bot: &str, args: Value) -> Result<Value> {
        // Starting involves a browser round trip. Serialize starts so concurrent calls for the
        // same user message cannot create duplicate tabs before either one is registered.
        let _start_guard = self.start_lock.lock().await;
        let takeover_epoch = self.takeover_epoch.load(Ordering::Acquire);
        self.takeover_guard(hub, takeover_epoch)?;
        let StartArgs { url } = serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_start call: {e}"))?;
        let url = checked_url(&url)?;
        let (message_id, cancelled_epoch) = {
            let mut state = self.state.lock().await;
            let message_id = current_message(hub, bot)?;
            let epoch = *state.cancelled_epochs.entry(bot.to_owned()).or_default();
            (message_id, epoch)
        };
        let existing = {
            let state = self.state.lock().await;
            state
                .sessions
                .values()
                .find(|session| session.binding.message_id == message_id)
                .map(|session| {
                    ensure!(
                        session.binding.initial_url == url,
                        "this user message already started a browser for a different URL"
                    );
                    Ok::<_, anyhow::Error>((session.binding.clone(), session.engine, session.extension_version.clone()))
                })
                .transpose()?
        };
        if let Some((binding, engine, extension_version)) = existing {
            self.takeover_guard(hub, takeover_epoch)?;
            let page = if engine == Engine::Extension {
                self.extension
                    .call("state", json!({"target": binding}))
                    .await
                    .context("checking the existing browser binding")?
            } else {
                public_state(&binding.current_url, "", "")
            };
            self.takeover_guard(hub, takeover_epoch)?;
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &BoundArgs::from_binding(&binding), true)?;
            session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
            let binding = session.binding.clone();
            let mut answer = json!({
                "target": binding,
                "state": page,
                "reused": true
            });
            if engine == Engine::Extension {
                answer["extension"] = json!({"id": EXTENSION_ID, "version": extension_version});
            }
            return Ok(answer);
        }
        {
            let state = self.state.lock().await;
            ensure!(
                state.sessions.values().filter(|session| session.binding.bot_id == bot).count() < 2,
                "too many browser tabs are open for this bot"
            );
        }
        let browser_id = uuid::Uuid::new_v4().to_string();
        let operation_id = uuid::Uuid::new_v4().to_string();
        let generation = uuid::Uuid::new_v4().to_string();
        if headless_enabled() {
            let state_root = crate::service::data_dir().join("browsers").join(bot).join(&browser_id);
            let launched = browser_headless::launch(&state_root).await?;
            let tab_ws = browser_headless::page_ws(launched.port, &launched.tab_id, &generation).await?;
            let page = browser_headless::navigate_tab(&tab_ws, &url).await?;
            let current_url = string_field(&page, "url", &url);
            let binding = Binding {
                host_id: hub.host_id.clone(),
                bot_id: bot.to_owned(),
                message_id: message_id.clone(),
                operation_id,
                browser_id: browser_id.clone(),
                tab_id: launched.tab_id,
                generation,
                browser_pid: launched.pid,
                browser_executable: launched.executable.display().to_string(),
                browser_version: launched.version,
                initial_url: url.clone(),
                current_url,
            };
            let session = Session {
                child: Some(launched.child),
                port: launched.port,
                profile: launched.profile,
                binding: binding.clone(),
                screenshots: 0,
                engine: Engine::Headless,
                extension_version: String::new(),
            };
            if let Err(error) = self.ensure_start_active(hub, bot, &message_id, cancelled_epoch, takeover_epoch).await {
                close(session).await;
                return Err(error);
            }
            let rejected = {
                let mut state = self.state.lock().await;
                if *state.cancelled_epochs.get(bot).unwrap_or(&0) != cancelled_epoch {
                    Err(anyhow!("the browser start was cancelled by the user"))
                } else if state.sessions.values().filter(|session| session.binding.bot_id == bot).count() >= 2 {
                    Err(anyhow!("too many browser tabs are open for this bot"))
                } else {
                    state.sessions.insert(browser_id, session);
                    Ok(())
                }
            };
            rejected?;
            return Ok(json!({"target": binding, "state": page, "reused": false}));
        }

        self.ensure_start_active(hub, bot, &message_id, cancelled_epoch, takeover_epoch).await?;
        let target = json!({
            "hostId": hub.host_id,
            "botId": bot,
            "messageId": message_id,
            "operationId": operation_id,
            "browserId": browser_id,
            "generation": generation,
            "url": url
        });
        let started = self
            .extension
            .call("start", json!({"target": target, "url": url}))
            .await
            .context("starting the Codync Browser extension")?;
        let tab_id = match &started["tabId"] {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => bail!("the Codync Browser extension returned no tab id"),
        };
        let page = started.get("state").cloned().unwrap_or_else(|| public_state(&url, "", ""));
        let current_url = string_field(&page, "url", &url);
        let binding = Binding {
            host_id: hub.host_id.clone(),
            bot_id: bot.to_owned(),
            message_id: message_id.clone(),
            operation_id,
            browser_id: browser_id.clone(),
            tab_id,
            generation,
            browser_pid: 0,
            browser_executable: started["browserExecutable"].as_str().unwrap_or("unknown").to_owned(),
            browser_version: started["browserVersion"].as_str().unwrap_or("unknown").to_owned(),
            initial_url: url.clone(),
            current_url,
        };
        let profile = crate::service::data_dir().join("browsers").join(bot).join(&browser_id).join("extension");
        let profile_for_io = profile.clone();
        let profile_result = tokio::task::spawn_blocking(move || std::fs::create_dir_all(&profile_for_io))
            .await
            .context("the extension browser directory task failed")?;
        if let Err(error) = profile_result {
            fail_extension_start(
                &self.extension,
                &binding,
                "the extension browser evidence directory could not be created",
            )
            .await;
            return Err(error).context("creating the extension browser evidence directory");
        }
        let extension_version = started["extensionVersion"].as_str().unwrap_or("unknown").to_owned();
        let session = Session {
            child: None,
            port: 0,
            profile,
            binding: binding.clone(),
            screenshots: 0,
            engine: Engine::Extension,
            extension_version: extension_version.clone(),
        };
        if let Err(error) = self.ensure_start_active(hub, bot, &message_id, cancelled_epoch, takeover_epoch).await {
            fail_extension_start(&self.extension, &binding, "the browser start was cancelled before it completed")
                .await;
            return Err(error);
        }
        let rejected = {
            let mut state = self.state.lock().await;
            if *state.cancelled_epochs.get(bot).unwrap_or(&0) != cancelled_epoch {
                Err(anyhow!("the browser start was cancelled by the user"))
            } else if state.sessions.values().filter(|session| session.binding.bot_id == bot).count() >= 2 {
                Err(anyhow!("too many browser tabs are open for this bot"))
            } else {
                state.sessions.insert(browser_id, session);
                Ok(())
            }
        };
        if let Err(error) = rejected {
            fail_extension_start(&self.extension, &binding, &error.to_string()).await;
            return Err(error);
        }
        Ok(json!({
            "target": binding,
            "state": page,
            "extension": {
                "id": EXTENSION_ID,
                "version": extension_version
            },
            "reused": false
        }))
    }

    async fn ensure_start_active(
        &self,
        hub: &Hub,
        bot: &str,
        message_id: &str,
        cancelled_epoch: u64,
        takeover_epoch: u64,
    ) -> Result<()> {
        self.takeover_guard(hub, takeover_epoch)?;
        ensure!(
            current_message(hub, bot).ok().as_deref() == Some(message_id),
            "the browser start no longer belongs to the user's current turn"
        );
        let state = self.state.lock().await;
        ensure!(
            *state.cancelled_epochs.get(bot).unwrap_or(&0) == cancelled_epoch,
            "the browser start was cancelled by the user"
        );
        Ok(())
    }

    async fn navigate(&self, hub: &Arc<Hub>, bot: &str, args: Value) -> Result<Value> {
        let takeover_epoch = self.takeover_epoch.load(Ordering::Acquire);
        self.takeover_guard(hub, takeover_epoch)?;
        let NavigateArgs { bound, url } =
            serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_navigate call: {e}"))?;
        let url = checked_url(&url)?;
        let extension = {
            let mut state = self.state.lock().await;
            extension_binding(&mut state, hub, bot, &bound, true)?
        };
        if let Some(binding) = extension {
            let page = self.extension.call("navigate", json!({"target": binding, "url": url})).await?;
            self.takeover_guard(hub, takeover_epoch)?;
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            session.binding.current_url = string_field(&page, "url", &url);
            return Ok(json!({"target": session.binding, "state": page}));
        }
        let (port, tab_id, generation) = {
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            (session.port, session.binding.tab_id.clone(), session.binding.generation.clone())
        };
        let tab_ws = browser_headless::page_ws(port, &tab_id, &generation).await?;
        let page = browser_headless::navigate_tab(&tab_ws, &url).await?;
        self.takeover_guard(hub, takeover_epoch)?;
        let mut state = self.state.lock().await;
        let session = bound_session(&mut state, hub, bot, &bound, true)?;
        session.binding.current_url = url;
        Ok(json!({"target": session.binding, "state": page}))
    }

    async fn click(&self, hub: &Arc<Hub>, bot: &str, args: Value) -> Result<Value> {
        let takeover_epoch = self.takeover_epoch.load(Ordering::Acquire);
        self.takeover_guard(hub, takeover_epoch)?;
        let ClickArgs { bound, selector } =
            serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_click call: {e}"))?;
        checked_selector(&selector)?;
        let extension = {
            let mut state = self.state.lock().await;
            extension_binding(&mut state, hub, bot, &bound, true)?
        };
        if let Some(binding) = extension {
            let page = self.extension.call("click", json!({"target": binding, "selector": selector})).await?;
            self.takeover_guard(hub, takeover_epoch)?;
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
            return Ok(json!({"target": session.binding, "state": page}));
        }
        let (port, tab_id, generation) = {
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            (session.port, session.binding.tab_id.clone(), session.binding.generation.clone())
        };
        let tab_ws = browser_headless::page_ws(port, &tab_id, &generation).await?;
        let page = browser_headless::click(&tab_ws, &selector).await?;
        self.takeover_guard(hub, takeover_epoch)?;
        let mut state = self.state.lock().await;
        let session = bound_session(&mut state, hub, bot, &bound, true)?;
        session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
        Ok(json!({"target": session.binding, "state": page}))
    }

    async fn r#type(&self, hub: &Arc<Hub>, bot: &str, args: Value) -> Result<Value> {
        let takeover_epoch = self.takeover_epoch.load(Ordering::Acquire);
        self.takeover_guard(hub, takeover_epoch)?;
        let TypeArgs { bound, selector, text } =
            serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_type call: {e}"))?;
        checked_selector(&selector)?;
        ensure!(text.len() <= MAX_TYPE, "text is longer than {MAX_TYPE} bytes");
        let extension = {
            let mut state = self.state.lock().await;
            extension_binding(&mut state, hub, bot, &bound, true)?
        };
        if let Some(binding) = extension {
            let page =
                self.extension.call("type", json!({"target": binding, "selector": selector, "text": text})).await?;
            self.takeover_guard(hub, takeover_epoch)?;
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
            return Ok(json!({"target": session.binding, "state": page}));
        }
        let (port, tab_id, generation) = {
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            (session.port, session.binding.tab_id.clone(), session.binding.generation.clone())
        };
        let tab_ws = browser_headless::page_ws(port, &tab_id, &generation).await?;
        let page = browser_headless::type_text(&tab_ws, &selector, &text).await?;
        self.takeover_guard(hub, takeover_epoch)?;
        let mut state = self.state.lock().await;
        let session = bound_session(&mut state, hub, bot, &bound, true)?;
        session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
        Ok(json!({"target": session.binding, "state": page}))
    }

    async fn screenshot(&self, hub: &Arc<Hub>, bot: &str, args: Value) -> Result<Value> {
        let takeover_epoch = self.takeover_epoch.load(Ordering::Acquire);
        self.takeover_guard(hub, takeover_epoch)?;
        let BoundArgs { operation_id, browser_id, tab_id, generation } =
            serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_screenshot call: {e}"))?;
        let bound = BoundArgs { operation_id, browser_id, tab_id, generation };
        let extension = {
            let mut state = self.state.lock().await;
            extension_binding(&mut state, hub, bot, &bound, true)?
        };
        let (encoded, page) = if let Some(binding) = extension {
            let answer = self.extension.call("screenshot", json!({"target": binding})).await?;
            let encoded =
                answer["data"].as_str().context("the Codync Browser extension returned no screenshot data")?.to_owned();
            let page = answer.get("state").cloned().unwrap_or_else(|| public_state(&binding.current_url, "", ""));
            (encoded, page)
        } else {
            let (port, tab_id, generation) = {
                let mut state = self.state.lock().await;
                let session = bound_session(&mut state, hub, bot, &bound, true)?;
                (session.port, session.binding.tab_id.clone(), session.binding.generation.clone())
            };
            let tab_ws = browser_headless::page_ws(port, &tab_id, &generation).await?;
            let (encoded, page) = browser_headless::screenshot(&tab_ws).await?;
            (encoded, page)
        };
        self.takeover_guard(hub, takeover_epoch)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).context("invalid screenshot data")?;
        ensure!(!bytes.is_empty() && bytes.len() <= MAX_SCREENSHOT, "screenshot is empty or too large");
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .context("Chrome returned an invalid PNG")?;
        ensure!(image.width() > 0 && image.height() > 0, "screenshot has an empty size");
        let screenshot_sha256 = digest(&bytes);
        let screenshot_size = bytes.len();
        let (binding, path, number) = {
            let mut state = self.state.lock().await;
            let session = bound_session(&mut state, hub, bot, &bound, true)?;
            session.screenshots += 1;
            let path = screenshot_path(&session.profile, session.screenshots);
            session.binding.current_url = string_field(&page, "url", &session.binding.current_url);
            (session.binding.clone(), path, session.screenshots)
        };
        let write_path = path.clone();
        tokio::task::spawn_blocking(move || write_whole(&write_path, &bytes))
            .await
            .context("the screenshot writer task failed")??;
        let current_url = string_field(&page, "url", &binding.current_url);
        let still_bound = if let Err(error) = self.takeover_guard(hub, takeover_epoch) {
            Err(error)
        } else {
            let mut state = self.state.lock().await;
            bound_session(&mut state, hub, bot, &bound, true).map(|session| {
                session.binding.current_url = current_url;
            })
        };
        if let Err(error) = still_bound {
            discard_screenshot(path.clone()).await;
            return Err(error);
        }
        let screenshot = json!({
            "path": path.display().to_string(),
            "sha256": screenshot_sha256,
            "size": screenshot_size,
            "width": image.width(),
            "height": image.height(),
            "number": number
        });
        let name = format!("browser-{}-{}.png", &binding.operation_id[..8], screenshot["number"]);
        let posted =
            crate::chat::outbox::call(hub, bot, "send_file", &json!({"path": screenshot["path"], "name": name}))
                .await
                .map_err(|e| anyhow!("the screenshot was captured but couldn't be posted to the chat: {e:#}"))?;
        Ok(json!({
            "target": binding,
            "state": page,
            "screenshot": screenshot,
            "postedToChat": posted["sent"] == true
        }))
    }

    async fn stop(&self, bot: &str, args: Value) -> Result<Value> {
        let bound: BoundArgs = serde_json::from_value(args).map_err(|e| anyhow!("invalid browser_stop call: {e}"))?;
        let session = {
            let mut state = self.state.lock().await;
            let session = state
                .sessions
                .get(&bound.browser_id)
                .ok_or_else(|| anyhow!("browser operation is no longer running"))?;
            ensure!(session.binding.bot_id == bot, "browser belongs to another bot");
            ensure!(session.binding.operation_id == bound.operation_id, "stale browser operation");
            ensure!(session.binding.tab_id == bound.tab_id, "browser tab changed");
            ensure!(session.binding.generation == bound.generation, "browser target changed");
            state.sessions.remove(&bound.browser_id)
        };
        let Some(session) = session else { return Ok(json!({"closed": true, "alreadyClosed": true})) };
        let binding = session.binding.clone();
        let mut already_closed = false;
        if session.engine == Engine::Extension {
            match self.extension.call("stop", json!({"target": binding})).await {
                Ok(_) => {}
                Err(error) if extension_session_closed(&error) => already_closed = true,
                Err(error) => {
                    self.state.lock().await.sessions.insert(bound.browser_id, session);
                    return Err(error);
                }
            }
        }
        close(session).await;
        Ok(json!({"closed": true, "alreadyClosed": already_closed, "target": binding}))
    }

    pub(super) async fn cancel_bot(&self, bot: &str) {
        let sessions = {
            let mut state = self.state.lock().await;
            let epoch = state.cancelled_epochs.entry(bot.to_owned()).or_default();
            *epoch = epoch.saturating_add(1);
            let ids: Vec<String> =
                state.sessions.iter().filter(|(_, s)| s.binding.bot_id == bot).map(|(id, _)| id.clone()).collect();
            ids.into_iter().filter_map(|id| state.sessions.remove(&id)).collect::<Vec<_>>()
        };
        for session in sessions {
            if session.engine == Engine::Extension {
                match self
                    .extension
                    .call_with_timeout("stop", json!({"target": session.binding}), Duration::from_secs(2))
                    .await
                {
                    Ok(_) => {}
                    Err(error) if extension_session_closed(&error) => {
                        tracing::debug!(browser_id = %session.binding.browser_id, "the browser tab was already closed");
                    }
                    Err(error) => {
                        tracing::warn!(%error, browser_id = %session.binding.browser_id, "could not close a browser tab after stopping its bot");
                        self.state.lock().await.sessions.insert(session.binding.browser_id.clone(), session);
                        continue;
                    }
                }
            }
            close(session).await;
        }
    }

    pub(super) async fn shutdown(&self) {
        let sessions = {
            let mut state = self.state.lock().await;
            state.sessions.drain().map(|(_, session)| session).collect::<Vec<_>>()
        };
        for session in sessions {
            if session.engine == Engine::Extension
                && let Err(error) = self
                    .extension
                    .call_with_timeout("stop", json!({"target": session.binding}), Duration::from_secs(2))
                    .await
            {
                tracing::warn!(%error, browser_id = %session.binding.browser_id, "could not close a browser tab during host shutdown");
            }
            close(session).await;
        }
    }
}

fn extension_binding(
    state: &mut State,
    hub: &Hub,
    bot: &str,
    bound: &BoundArgs,
    require_current_message: bool,
) -> Result<Option<Binding>> {
    let session = bound_session(state, hub, bot, bound, require_current_message)?;
    Ok((session.engine == Engine::Extension).then(|| session.binding.clone()))
}

async fn fail_extension_start(bridge: &ExtensionBridge, binding: &Binding, reason: &str) {
    if let Err(error) = bridge.call_with_timeout("stop", json!({"target": binding}), Duration::from_secs(2)).await {
        tracing::warn!(%error, %reason, browser_id = %binding.browser_id, "could not close a browser tab after start failed");
    }
}

fn extension_session_closed(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains(EXTENSION_SESSION_CLOSED)
}

fn bound_session<'a>(
    state: &'a mut State,
    hub: &Hub,
    bot: &str,
    bound: &BoundArgs,
    require_current_message: bool,
) -> Result<&'a mut Session> {
    let session = state.sessions.get_mut(&bound.browser_id).ok_or_else(|| {
        anyhow!("browser operation is no longer running; it was stopped, timed out, or its turn ended")
    })?;
    ensure!(session.binding.bot_id == bot, "browser belongs to another bot");
    ensure!(session.binding.operation_id == bound.operation_id, "stale browser operation");
    ensure!(session.binding.tab_id == bound.tab_id, "browser tab changed");
    ensure!(session.binding.generation == bound.generation, "browser target changed");
    if require_current_message {
        let running = hub.runtime(bot);
        ensure!(running.status == BotStatus::Working, "browser operations work only during the user's turn");
        ensure!(
            running.message.as_deref() == Some(session.binding.message_id.as_str()),
            "this browser belongs to a different user message"
        );
    }
    Ok(session)
}

fn current_message(hub: &Hub, bot: &str) -> Result<String> {
    let runtime = hub.runtime(bot);
    ensure!(runtime.status == BotStatus::Working, "browser tools work only during the user's turn");
    runtime
        .message
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("browser tools need a user message in this bot's own chat"))
}

fn checked_url(raw: &str) -> Result<String> {
    ensure!(!raw.is_empty() && raw.len() <= MAX_URL, "URL must be 1-{MAX_URL} bytes");
    let url = Url::parse(raw).context("invalid URL")?;
    ensure!(matches!(url.scheme(), "http" | "https"), "only http and https pages can be opened");
    ensure!(url.username().is_empty() && url.password().is_none(), "URLs with credentials are not allowed");
    ensure!(url.host_str().is_some(), "URL needs a host");
    Ok(url.to_string())
}

fn checked_selector(selector: &str) -> Result<()> {
    ensure!(!selector.is_empty() && selector.len() <= MAX_SELECTOR, "selector must be 1-{MAX_SELECTOR} bytes");
    Ok(())
}

/// Automated browser tests can select the old isolated CDP engine explicitly. Product builds
/// always use the installed extension, so losing the extension never silently opens another browser.
fn headless_enabled() -> bool {
    std::env::var_os("CODYNC_BROWSER_HEADLESS").is_some_and(|value| value != "0")
}

fn public_state(url: &str, title: &str, text: &str) -> Value {
    json!({"url":url, "title":title, "text":text})
}

fn string_field(value: &Value, key: &str, fallback: &str) -> String {
    value[key].as_str().filter(|s| !s.is_empty()).unwrap_or(fallback).to_owned()
}

fn screenshot_path(profile: &Path, number: u64) -> PathBuf {
    profile.parent().unwrap_or(profile).join(format!("screenshot-{number}.png"))
}

fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("screenshot folder")?)?;
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

fn remove_screenshot_artifacts(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("removing an abandoned screenshot"),
    }
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    match std::fs::remove_dir(parent) {
        Ok(()) => {}
        Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty) => {}
        Err(error) => return Err(error).context("removing an abandoned screenshot directory"),
    }
    Ok(())
}

async fn discard_screenshot(path: PathBuf) {
    if let Err(error) = tokio::task::spawn_blocking(move || remove_screenshot_artifacts(&path))
        .await
        .context("the screenshot cleanup task failed")
        .and_then(|result| result)
    {
        tracing::warn!(%error, "could not remove a screenshot after its browser stopped");
    }
}

fn digest(bytes: &[u8]) -> String {
    crate::chat::files::digest_hex(&Sha256::digest(bytes))
}

async fn close(mut session: Session) {
    if let Some(child) = session.child.as_mut() {
        if let Err(error) = child.start_kill() {
            tracing::warn!(%error, browser_id = %session.binding.browser_id, "could not signal the test browser to stop");
        }
        match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, browser_id = %session.binding.browser_id, "could not reap the test browser");
            }
            Err(_) => {
                tracing::warn!(browser_id = %session.binding.browser_id, "the test browser did not exit within five seconds");
            }
        }
    }
    let root = session.profile.parent().unwrap_or(&session.profile).to_owned();
    let browser_id = session.binding.browser_id.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(root))
        .await
        .context("the browser cleanup task failed")
        .and_then(|result| result.context("removing the browser evidence directory"))
    {
        tracing::warn!(%error, %browser_id, "could not remove the browser evidence directory");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_urls_and_selectors_are_bounded() {
        assert_eq!(checked_url("https://example.com/a").unwrap(), "https://example.com/a");
        assert!(checked_url("file:///etc/passwd").is_err());
        assert!(checked_url("https://user:secret@example.com").is_err());
        assert!(checked_url(&format!("https://example.com/{}", "x".repeat(MAX_URL))).is_err());
        assert!(checked_selector("").is_err());
        assert!(checked_selector(&"x".repeat(MAX_SELECTOR + 1)).is_err());
    }

    #[tokio::test]
    async fn abandoned_screenshots_are_removed_with_their_empty_evidence_directory() {
        let root = std::env::temp_dir().join(format!("codync-browser-{}", uuid::Uuid::new_v4()));
        let path = root.join("screenshot-1.png");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&path, b"abandoned").unwrap();

        discard_screenshot(path.clone()).await;

        assert!(!path.exists(), "the abandoned screenshot remained on disk");
        assert!(!root.exists(), "the empty screenshot directory remained on disk");
    }

    #[test]
    fn only_the_explicit_closed_session_error_removes_the_host_binding() {
        assert!(extension_session_closed(&anyhow!(
            "browser_session_closed: Browser operation is no longer running in Codync Browser"
        )));
        assert!(!extension_session_closed(&anyhow!("The Codync Browser extension disconnected before it answered")));
    }

    #[test]
    fn tools_keep_the_same_binding_shape() {
        let tools = tools();
        assert_eq!(tools.len(), NAMES.len());
        for tool in &tools[1..] {
            for key in ["operationId", "browserId", "tabId", "generation"] {
                assert!(tool["inputSchema"]["properties"][key].is_object(), "{tool}");
            }
        }
    }
}
