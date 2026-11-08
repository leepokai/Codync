//! Windows-only owner-driven secret delivery. The external CUA driver never sees a secret.
use super::Screen;
use crate::{agent::process::Process, hub::Hub};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
pub(crate) mod pipe;
use pipe::rpc;
const FILL: &str = include_str!("protected-fill.js");
pub(super) struct Browser {
    id: String,
    process: Process,
    folder: PathBuf,
    pipe: pipe::Pipe,
    target: String,
    context: String,
    /// A named profile keeps its sign-ins between entries; a fresh one is deleted on close.
    persistent: bool,
    /// The loopback relay of this profile's proxy; dropped with the browser.
    relay: Option<crate::browser::proxy::Relay>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    id: String,
    #[serde(default)]
    login_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

pub async fn call(hub: &std::sync::Arc<Hub>, method: &str, body: Value) -> Result<Value> {
    let _operation = hub.screen.protected_operation.lock().await;
    let request: Request = serde_json::from_value(body)?;
    let state = hub.screen.protected_state();
    ensure!(state["id"] == request.id, "Protected entry changed; refresh before continuing");
    let bot = state["botId"].as_str().context("Protected entry has no bot")?;
    let site = state["site"].as_str().context("Protected entry has no origin")?;
    let profile = hub
        .store
        .bot(bot)?
        .filter(|row| !row.deleted && row.config.computer && row.config.computer_permissions.browser)
        .and_then(|row| row.config.browser_profile);
    ensure!(
        hub.store
            .bot(bot)?
            .is_some_and(|row| !row.deleted && row.config.computer && row.config.computer_permissions.browser),
        "Desktop permission is required"
    );
    let mut current = hub.screen.protected_browser.lock().await;
    ensure!(hub.screen.protected_state()["id"] == request.id, "Protected entry changed");
    if method == "openProtectedBrowser" {
        ensure!(request.code.is_none() && request.login_id.is_none(), "Opening a browser accepts no secret");
        if let Some(browser) = current.as_mut() {
            if browser.id.is_empty() {
                destroy(browser).await?;
                *current = None;
            } else {
                ensure!(
                    browser.id == request.id && !browser.process.exited()?,
                    "Close this protected entry before opening another browser"
                );
                return Ok(json!({"opened":true}));
            }
        }
        hub.screen.driver.cancel().await?;
        *current = Some(
            tokio::time::timeout(Duration::from_secs(20), open(hub, &request.id, site, bot, profile.as_deref()))
                .await
                .context("Protected browser opening timed out")??,
        );
        return Ok(
            json!({"opened":true,"instruction":"Use this isolated browser, focus its password or one-time-code field, then return to confirm filling. The original browser is never targeted."}),
        );
    }
    let browser =
        current.as_mut().filter(|browser| browser.id == request.id).context("Open the protected browser first")?;
    ensure!(!browser.process.exited()?, "Protected browser closed; finish this entry and start again");
    let kind = if method == "fillProtectedCode" { "code" } else { "password" };
    let operation = async {
        let socket = &mut browser.pipe;
        let attached =
            rpc(socket, "Target.attachToTarget", json!({"targetId":browser.target,"flatten":true}), None).await?;
        let session = attached["sessionId"].as_str().context("Could not bind protected browser")?;
        let focus = rpc(
            socket,
            "Runtime.evaluate",
            json!({"expression":"document.activeElement","objectGroup":"protected-entry","silent":true}),
            Some(session),
        )
        .await?;
        let object = focus["result"]["objectId"].as_str().context("Focus a password or one-time-code input first")?;
        let base = json!({"objectId":object,"functionDeclaration":FILL,"arguments":[{"value":site},{"value":kind}],"returnByValue":true,"silent":true});
        let probe = rpc(socket, "Runtime.callFunctionOn", base.clone(), Some(session)).await?;
        ensure!(
            probe["result"]["value"]["ready"] == true && probe["result"]["value"]["origin"] == site,
            "Focus the correct protected field on the exact approved HTTPS origin"
        );
        let secret = if kind == "code" {
            ensure!(request.login_id.is_none(), "A code request accepts no login");
            let code = request.code.as_deref().context("Code is required")?;
            ensure!(
                (4..=32).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
                "Invalid one-time code"
            );
            zeroize::Zeroizing::new(code.to_owned())
        } else {
            ensure!(request.code.is_none(), "A password request accepts no code");
            let login =
                crate::market::logins::get(&hub.store, request.login_id.as_deref().context("Choose a saved login")?)?;
            ensure!(
                login.site == crate::market::logins::normalize_site(site),
                "The saved login belongs to another website"
            );
            zeroize::Zeroizing::new(crate::market::passwords::resolve(&hub.store, &login.password).await?)
        };
        ensure!(hub.screen.protected_state()["id"] == request.id, "Protected entry changed");
        ensure!(
            hub.store
                .bot(bot)?
                .is_some_and(|row| !row.deleted && row.config.computer && row.config.computer_permissions.browser),
            "Desktop permission was revoked"
        );
        let mut fill = base;
        fill["arguments"]
            .as_array_mut()
            .context("Invalid private fill request")?
            .push(json!({"value":secret.as_str()}));
        let result = rpc(socket, "Runtime.callFunctionOn", fill, Some(session)).await?;
        ensure!(
            result["result"]["value"] == "filled",
            "The field changed before delivery; nothing is confirmed. Check it yourself before retrying"
        );
        anyhow::Ok(
            json!({"filled":true,"instruction":"The value was delivered to the approved field. Submit it yourself; Codync computer tools remain paused."}),
        )
    };
    tokio::time::timeout(Duration::from_secs(15), operation).await.context(
        "Secret delivery timed out; outcome unknown. Check the protected browser; do not retry automatically",
    )?
}

pub(super) async fn release(screen: &Screen, id: &str, keep_session: bool) -> Result<&'static str> {
    let mut current = screen.protected_browser.lock().await;
    if let Some(browser) = current.as_mut() {
        if browser.id.is_empty() {
            return Ok("none");
        }
        ensure!(browser.id == id, "Protected browser changed");
        if keep_session && !browser.process.exited()? {
            match tokio::time::timeout(Duration::from_secs(10), reset_private_pages(browser)).await {
                Ok(Ok(())) => return Ok("preserved"),
                _ => {
                    tracing::warn!(
                        "Private browser pages could not be reset; closing the entire owned browser instead"
                    );
                }
            }
        }
        destroy(browser).await?;
        *current = None;
        return Ok("closed");
    }
    Ok("none")
}
async fn reset_private_pages(browser: &mut Browser) -> Result<()> {
    let socket = &mut browser.pipe;
    let initial = rpc(socket, "Target.getTargets", json!({}), None).await?;
    let pages = initial["targetInfos"].as_array().context("Could not inspect private pages")?;
    ensure!(
        pages
            .iter()
            .filter(|target| target["type"] == "page")
            .all(|target| target["browserContextId"] == browser.context),
        "A page is outside the private session"
    );
    let blank = rpc(
        socket,
        "Target.createTarget",
        json!({"url":"about:blank","browserContextId":browser.context,"newWindow":true}),
        None,
    )
    .await?;
    let blank = blank["targetId"].as_str().context("Could not create a clean private page")?.to_owned();
    for _ in 0..3 {
        let targets = rpc(socket, "Target.getTargets", json!({}), None).await?;
        let pages = targets["targetInfos"].as_array().context("Could not inspect private pages")?;
        let mut closed = false;
        for page in pages.iter().filter(|target| target["type"] == "page") {
            ensure!(page["browserContextId"] == browser.context, "Unexpected browser context");
            let id = page["targetId"].as_str().context("Unidentified private page")?;
            if id == blank {
                continue;
            }
            let result = rpc(socket, "Target.closeTarget", json!({"targetId":id}), None).await?;
            ensure!(result["success"] == true, "Could not close a private page");
            closed = true;
        }
        if !closed {
            ensure!(
                pages.iter().any(|target| target["targetId"] == blank
                    && target["type"] == "page"
                    && target["url"] == "about:blank"
                    && target["browserContextId"] == browser.context
                    && target.get("openerId").is_none()),
                "Clean private page changed"
            );
            ensure!(
                pages.iter().all(|target| target["type"] == "browser" || target["targetId"] == blank),
                "Unverified browser targets survived private page reset"
            );
            browser.target = blank;
            return Ok(());
        }
    }
    bail!("Private pages continued changing during handback")
}
pub(super) async fn handed_back(screen: &Screen, id: &str) {
    if let Some(browser) = screen.protected_browser.lock().await.as_mut()
        && browser.id == id
    {
        browser.id.clear();
    }
}
async fn destroy(browser: &mut Browser) -> Result<()> {
    browser.process.terminate_owned().await?;
    if !browser.persistent
        && let Err(error) = remove_profile(&browser.folder)
    {
        tracing::warn!(%error,"Could not remove closed protected browser profile");
    }
    Ok(())
}
fn remove_profile(folder: &std::path::Path) -> Result<()> {
    if !folder.try_exists()? {
        return Ok(());
    }
    let parent = crate::service::data_dir().join("protected-browsers").canonicalize()?;
    let folder = folder.canonicalize()?;
    ensure!(folder.starts_with(&parent) && folder != parent, "Invalid protected browser directory");
    std::fs::remove_dir_all(folder)?;
    Ok(())
}

async fn open(hub: &std::sync::Arc<Hub>, id: &str, site: &str, bot: &str, profile: Option<&str>) -> Result<Browser> {
    let exe = browser_executable().context("Install Chrome or Edge to use protected browser entry")?;
    crate::market::vault::unlock(hub.clone()).await?;
    let relay = match crate::browser::proxy::load(&hub.store, bot)? {
        Some(proxy) => Some(crate::browser::proxy::start(proxy).await?),
        None => None,
    };
    let proxy = relay.as_ref().map(|relay| format!("http://127.0.0.1:{}", relay.port));
    let parent = crate::service::data_dir().join("protected-browsers");
    std::fs::create_dir_all(&parent)?;
    let named = profile.map(profile_slug).filter(|name| !name.is_empty());
    let (folder, persistent) = if let Some(name) = named {
        let folder = parent.join("profiles").join(profile_slug(bot)).join(name);
        std::fs::create_dir_all(&folder)?;
        (folder, true)
    } else {
        let folder = parent.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&folder)?;
        (folder, false)
    };
    let (process, mut socket) = pipe::spawn(&exe, &folder, proxy.as_deref())?;
    let targets = rpc(&mut socket, "Target.getTargets", json!({}), None).await?;
    let context = rpc(&mut socket, "Target.createBrowserContext", json!({"disposeOnDetach":false}), None).await?;
    let context = context["browserContextId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .context("The browser did not create an owned private context")?
        .to_owned();
    let target =
        rpc(&mut socket, "Target.createTarget", json!({"url":site,"browserContextId":context,"newWindow":true}), None)
            .await?;
    let target = target["targetId"].as_str().context("Missing protected tab")?.to_owned();
    for page in targets["targetInfos"].as_array().into_iter().flatten().filter(|page| page["type"] == "page") {
        rpc(&mut socket, "Target.closeTarget", json!({"targetId":page["targetId"]}), None).await?;
    }
    Ok(Browser { id: id.into(), process, folder, pipe: socket, target, context, persistent, relay })
}

/// A safe folder name for a bot or profile label.
fn profile_slug(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
        .chars()
        .take(40)
        .collect()
}
pub(crate) fn browser_executable() -> Option<PathBuf> {
    for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        let Some(root) = std::env::var_os(root) else { continue };
        for path in ["Microsoft/Edge/Application/msedge.exe", "Google/Chrome/Application/chrome.exe"] {
            let path = PathBuf::from(&root).join(path);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::profile_slug;

    #[test]
    fn profile_and_bot_names_make_safe_short_folders() {
        assert_eq!(profile_slug(" Client A "), "client-a");
        assert_eq!(profile_slug("acme/../etc"), "acme----etc");
        assert_eq!(profile_slug(&"x".repeat(80)).len(), 40);
    }
}
