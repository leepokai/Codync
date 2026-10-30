//! Methods about this computer itself: host updates, hello/sync, agents and setup
//! terminals, usage, folders, voice and the remote screen.

use super::devices::Caller;
use super::str_arg;
use crate::LockExt;
use crate::agent::backends;
use crate::hub::Hub;
use crate::usage;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    Ok(ControlFlow::Break(match method {
        "hostUpdateStatus" => crate::update::status()?,
        "checkHostUpdate" => crate::update::check().await?,
        "setHostAutomaticUpdates" => {
            crate::update::set_automatic(b["enabled"].as_bool().context("enabled must be a boolean")?)?
        }
        "installHostUpdate" => {
            let port = hub.port;
            let force = b["force"].as_bool().unwrap_or(false);
            if !force && hub.busy() {
                bail!("A bot is working on this computer. Update it once the bot is done.");
            }
            tokio::task::spawn_blocking(move || crate::update::request_install(port, force)).await??;
            json!({"scheduled": true})
        }
        "voiceStatus" => crate::voice::status(&hub.store)?,
        "setVoiceKey" => {
            let provider = crate::voice::Provider::parse(str_arg(&b, "provider")?)?;
            crate::voice::set_key(&hub.store, provider, b["key"].as_str().unwrap_or_default()).await?
        }
        "voiceSession" => {
            let provider = crate::voice::Provider::parse(str_arg(&b, "provider")?)?;
            crate::voice::session(&hub.store, provider, str_arg(&b, "model")?, str_arg(&b, "voice")?).await?
        }
        "voiceModels" => {
            crate::voice::models(&hub.store, crate::voice::Provider::parse(str_arg(&b, "provider")?)?).await?
        }
        "voiceTranscribe" => {
            let provider = crate::voice::Provider::parse(str_arg(&b, "provider")?)?;
            crate::voice::transcribe(&hub.store, provider, str_arg(&b, "model")?, str_arg(&b, "audio")?).await?
        }
        "voiceSpeak" => {
            let provider = crate::voice::Provider::parse(str_arg(&b, "provider")?)?;
            crate::voice::speak(
                &hub.store,
                provider,
                str_arg(&b, "model")?,
                str_arg(&b, "voice")?,
                str_arg(&b, "text")?,
            )
            .await?
        }
        "hello" => {
            let port = hub.port;
            let icons = crate::icons::snapshot(&hub.store)?;
            json!({
                "hostId": hub.host_id,
                "computerId": hub.identity.computer_id(),
                "signKey": hub.identity.sign_pub_b64(),
                "boxKey": hub.identity.box_pub_b64(),
                "cloud": crate::remote::cloud::url(&hub.store),
                "name": crate::service::host_name(),
                "version": env!("CARGO_PKG_VERSION"),
                "minApp": crate::compat::MIN_APP,
                // Whether `installHostUpdate` can update this computer, from here or a phone.
                "canUpdate": tokio::task::spawn_blocking(crate::update::installable).await?,
                // The latest release, so a phone can say whether this computer is current (null: unknown).
                "latestVersion": tokio::task::spawn_blocking(crate::update::latest_version).await?,
                "os": std::env::consts::OS,
                "device": tokio::task::spawn_blocking(crate::service::device).await?,
                "home": dirs::home_dir().map(|p| p.to_string_lossy().into_owned()),
                "backends": backends::list(),
                "icons": icons.icons,
                "iconsRevision": icons.icons_revision,
                "rev": hub.store.current_rev(),
                "screen": hub.screen.state(),
                "analytics": hub.analytics.state(),
                // Shells out to `tailscale`: keep it off the async workers.
                "urls": tokio::task::spawn_blocking(move || crate::service::addresses(port)).await?,
            })
        }
        "refreshBackends" => {
            tokio::task::spawn_blocking(backends::hydrate_path).await?;
            crate::market::agent_sign_in::refresh(hub).await;
            json!({"backends": backends::list()})
        }
        "usage" => {
            if b["refresh"].as_bool().unwrap_or(false) {
                usage::refresh(hub).await;
            }
            serde_json::to_value(&*hub.usage.locked())?
        }
        "listDirs" => {
            let path = b["path"].as_str().map(std::path::PathBuf::from);
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || super::folders::listing(&hub.store, path)).await??
        }
        "selectWorkspace" => {
            let path = str_arg(&b, "path")?.to_owned();
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || super::folders::select(&hub.store, &path)).await??
        }
        "agentSetup" => {
            let step: crate::agent::term::Step =
                serde_json::from_value(b["step"].clone()).context("`step` is install or login")?;
            let (cols, rows) = term_size(&b);
            // Node.js belongs to the computer, not to one agent.
            let backend = if step == crate::agent::term::Step::Node { "" } else { str_arg(&b, "backend")? };
            json!({"term": hub.terms.start(backend, step, b["method"].as_str(), cols, rows).await?})
        }
        "agentModels" => crate::agent::auth::models(&hub.store, str_arg(&b, "backend")?).await?,
        "agentAuth" | "agentAuthenticate" | "setAgentEnv" => {
            let backend = str_arg(&b, "backend")?;
            let status = match method {
                "agentAuth" => crate::agent::auth::check(&hub.store, backend).await?,
                "agentAuthenticate" => {
                    crate::agent::auth::authenticate(&hub.store, backend, str_arg(&b, "method")?).await?
                }
                _ => {
                    let vars = b["vars"].as_object().ok_or_else(|| anyhow!("`vars` is required"))?;
                    crate::agent::auth::set_env(&hub.store, backend, vars).await?
                }
            };
            // Chats that asked the user to sign in get their messages answered now.
            if status["signedIn"] == true {
                crate::market::agent_sign_in::agent_signed_in(hub, backend)?;
            }
            status
        }
        "termInput" => {
            hub.terms.write(str_arg(&b, "term")?, str_arg(&b, "data")?)?;
            json!({})
        }
        "termResize" => {
            let (cols, rows) = term_size(&b);
            hub.terms.resize(str_arg(&b, "term")?, cols, rows)?;
            json!({})
        }
        "termClose" => {
            hub.terms.close(str_arg(&b, "term")?);
            json!({})
        }
        "screenStatus" => hub.screen.state(),
        "requestScreenPermission" => hub.screen.request_permission(&b).await?,
        "screenPrepare" => {
            if !hub.screen.enabled() {
                bail!("Remote screen is turned off on this computer.");
            }
            let ice = if b["relay"].as_bool().unwrap_or(false) {
                crate::remote::cloud::screen_ice(hub, caller.device_key()).await?
            } else {
                crate::screen::IceConfig::default()
            };
            let result = hub.screen.prepare(caller.device_key(), ice, b["trickle"].as_bool().unwrap_or(false))?;
            let session = result["session"].as_str().context("missing screen session")?.to_owned();
            tokio::spawn(crate::screen::watch_viewer(hub.clone(), session, caller.device_key().to_owned()));
            result
        }
        "screenOffer" => {
            let display = b["display"].as_u64().and_then(|d| u32::try_from(d).ok());
            hub.screen.offer(caller.device_key(), str_arg(&b, "sdp")?, str_arg(&b, "session")?, display).await?
        }
        "screenCandidate" => {
            let candidate = serde_json::from_value(b["candidate"].clone())?;
            hub.screen.candidate(str_arg(&b, "session")?, caller.device_key(), candidate).await?;
            json!({})
        }
        "screenClose" => {
            let session = str_arg(&b, "session")?;
            if hub.screen.owns_session(session, caller.device_key()) {
                hub.screen.close(session).await?;
            }
            json!({})
        }
        "screenTakeover" => {
            hub.screen.takeover(b["on"].as_bool().unwrap_or(false));
            hub.screen.state()
        }
        "setAnalytics" => {
            hub.analytics.set_enabled(&hub.store, b["enabled"].as_bool().unwrap_or(false))?;
            json!({"enabled": hub.analytics.state()})
        }
        "track" => {
            crate::analytics::track(hub, &b)?;
            json!({})
        }
        "reportError" => {
            crate::analytics::errors::report(hub, &b)?;
            json!({})
        }
        "setScreenEnabled" => {
            hub.screen.set_enabled(&hub.store, b["enabled"].as_bool().unwrap_or(false)).await?;
            hub.screen.state()
        }
        "computerTools" => crate::screen::computer_tools(hub).await,
        "computerCall" => {
            let args = b.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({}));
            crate::screen::computer(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, args).await?
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}

fn term_size(b: &Value) -> (u16, u16) {
    let dim = |k: &str, default: u16| b[k].as_u64().and_then(|v| u16::try_from(v).ok()).unwrap_or(default);
    (dim("cols", 80), dim("rows", 24))
}
