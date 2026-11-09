//! The Browser panel: a Codync-owned browser with tabs the owner opens from the app, driven over the same private
//! CDP pipe as protected entries. The page is shown as a screenshot; typing and clicking stay in the real window.

use super::Screen;
use super::protected_browser::{browser_executable, pipe};
use crate::hub::Hub;
use anyhow::{Context, Result, bail, ensure};
use pipe::rpc;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Tabs the panel allows before it asks to close one.
const MAX_TABS: usize = 12;

pub struct Panel {
    process: crate::agent::process::Process,
    folder: PathBuf,
    pipe: pipe::Pipe,
    context: String,
    active: String,
}

fn url_argument(raw: Option<&str>) -> Result<String> {
    let url = raw.unwrap_or_default().trim();
    if url.is_empty() {
        return Ok("about:blank".into());
    }
    ensure!(
        url.len() <= 2048 && (url.starts_with("https://") || url.starts_with("http://") || url == "about:blank"),
        "Enter an http or https address"
    );
    ensure!(!url.chars().any(char::is_whitespace), "The address has a space in it");
    Ok(url.into())
}

fn pages(targets: &Value, context: &str) -> Vec<(String, String, String)> {
    targets["targetInfos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|target| target["type"] == "page" && target["browserContextId"] == context)
        .filter_map(|target| {
            Some((
                target["targetId"].as_str()?.to_owned(),
                target["title"].as_str().unwrap_or_default().to_owned(),
                target["url"].as_str().unwrap_or_default().to_owned(),
            ))
        })
        .collect()
}

fn answer(panel: &Panel, tabs: Vec<(String, String, String)>) -> Value {
    json!({
        "open": true,
        "activeId": panel.active,
        "tabs": tabs.into_iter().map(|(id, title, url)| json!({"id":id,"title":title,"url":url})).collect::<Vec<_>>(),
    })
}

/// The open panel's tabs, or `{"open":false}`.
pub async fn state(screen: &Screen) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    let Some(panel) = current.as_mut() else { return Ok(json!({"open":false,"tabs":[]})) };
    if panel.process.exited()? {
        *current = None;
        return Ok(json!({"open":false,"tabs":[]}));
    }
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    Ok(answer(panel, pages(&targets, &panel.context)))
}

/// Opens the panel (with `url`) when it is closed, or a new tab on it.
pub async fn open(hub: &std::sync::Arc<Hub>, url: Option<&str>) -> Result<Value> {
    let url = url_argument(url)?;
    let mut current = hub.screen.panel.lock().await;
    if let Some(panel) = current.as_mut() {
        if !panel.process.exited()? {
            let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
            let tabs = pages(&targets, &panel.context);
            ensure!(tabs.len() < MAX_TABS, "Close a tab first");
            let created =
                rpc(&mut panel.pipe, "Target.createTarget", json!({"url":url,"browserContextId":panel.context}), None)
                    .await?;
            created["targetId"].as_str().context("The browser did not open a tab")?.clone_into(&mut panel.active);
            let _ = rpc(&mut panel.pipe, "Target.activateTarget", json!({"targetId":panel.active}), None).await;
            let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
            return Ok(answer(panel, pages(&targets, &panel.context)));
        }
        *current = None;
    }
    let panel = spawn(hub, &url).await?;
    let mut panel = panel;
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    let value = answer(&panel, pages(&targets, &panel.context));
    *current = Some(panel);
    Ok(value)
}

async fn spawn(hub: &std::sync::Arc<Hub>, url: &str) -> Result<Panel> {
    let exe = browser_executable().context("Install Chrome or Edge to use the browser panel")?;
    let parent = crate::service::data_dir().join("protected-browsers").join("panel");
    std::fs::create_dir_all(&parent)?;
    let (process, mut socket) = pipe::spawn(&exe, &parent, None)?;
    let targets = rpc(&mut socket, "Target.getTargets", json!({}), None).await?;
    let context = rpc(&mut socket, "Target.createBrowserContext", json!({"disposeOnDetach":false}), None).await?;
    let context = context["browserContextId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .context("The browser did not create an owned context")?
        .to_owned();
    for page in targets["targetInfos"].as_array().into_iter().flatten().filter(|page| page["type"] == "page") {
        rpc(&mut socket, "Target.closeTarget", json!({"targetId":page["targetId"]}), None).await.ok();
    }
    let target = rpc(&mut socket, "Target.createTarget", json!({"url":url,"browserContextId":context}), None).await?;
    let active = target["targetId"].as_str().context("The browser did not open a tab")?.to_owned();
    let _ = hub;
    Ok(Panel { process, folder: parent, pipe: socket, context, active })
}

/// Puts the panel's browser window at `left, top, width, height`, so it sits beside the app instead of behind it.
pub async fn place(screen: &Screen, body: &Value) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    let panel = current.as_mut().context("Open the browser panel first")?;
    let window = rpc(&mut panel.pipe, "Browser.getWindowForTarget", json!({"targetId":panel.active}), None).await?;
    let window_id = window["windowId"].as_i64().context("The browser gave no window")?;
    let number = |key: &str, max: i64| body[key].as_i64().filter(|value| (0..=max).contains(value));
    let (Some(left), Some(top), Some(width), Some(height)) =
        (number("x", 20_000), number("y", 20_000), number("width", 20_000), number("height", 20_000))
    else {
        bail!("The browser window needs x, y, width and height");
    };
    rpc(
        &mut panel.pipe,
        "Browser.setWindowBounds",
        json!({"windowId":window_id, "bounds":{"left":left, "top":top, "width":width, "height":height, "windowState":"normal"}}),
        None,
    )
    .await?;
    Ok(json!({"placed":true}))
}

/// Shows `tab`: it becomes the active tab.
pub async fn activate(screen: &Screen, tab: &str) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    let panel = current.as_mut().context("Open the browser panel first")?;
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    let tabs = pages(&targets, &panel.context);
    ensure!(tabs.iter().any(|(id, _, _)| id == tab), "Unknown tab");
    rpc(&mut panel.pipe, "Target.activateTarget", json!({"targetId":tab}), None).await?;
    tab.clone_into(&mut panel.active);
    Ok(answer(panel, tabs))
}

/// Loads `url` in `tab`.
pub async fn navigate(screen: &Screen, tab: &str, url: Option<&str>) -> Result<Value> {
    let url = url_argument(url)?;
    let mut current = screen.panel.lock().await;
    let panel = current.as_mut().context("Open the browser panel first")?;
    rpc(&mut panel.pipe, "Target.activateTarget", json!({"targetId":tab}), None).await.ok();
    let attached = rpc(&mut panel.pipe, "Target.attachToTarget", json!({"targetId":tab,"flatten":true}), None).await?;
    let session = attached["sessionId"].as_str().context("Could not reach the tab")?.to_owned();
    let result = rpc(&mut panel.pipe, "Page.navigate", json!({"url":url}), Some(&session)).await;
    rpc(&mut panel.pipe, "Target.detachFromTarget", json!({"sessionId":session}), None).await.ok();
    result?;
    tab.clone_into(&mut panel.active);
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    Ok(answer(panel, pages(&targets, &panel.context)))
}

/// Closes `tab`; the last one is replaced by a blank tab.
pub async fn close(screen: &Screen, tab: &str) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    let panel = current.as_mut().context("Open the browser panel first")?;
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    let tabs = pages(&targets, &panel.context);
    ensure!(tabs.iter().any(|(id, _, _)| id == tab), "Unknown tab");
    rpc(&mut panel.pipe, "Target.closeTarget", json!({"targetId":tab}), None).await?;
    if tabs.len() == 1 {
        let created = rpc(
            &mut panel.pipe,
            "Target.createTarget",
            json!({"url":"about:blank","browserContextId":panel.context}),
            None,
        )
        .await?;
        created["targetId"].as_str().context("The browser did not open a tab")?.clone_into(&mut panel.active);
    } else if panel.active == tab
        && let Some((id, _, _)) = tabs.iter().find(|(id, _, _)| id != tab)
    {
        panel.active.clone_from(id);
    }
    let targets = rpc(&mut panel.pipe, "Target.getTargets", json!({}), None).await?;
    Ok(answer(panel, pages(&targets, &panel.context)))
}

/// A PNG of `tab` (or the active tab), as a data URL.
pub async fn shot(screen: &Screen, tab: Option<&str>) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    let panel = current.as_mut().context("Open the browser panel first")?;
    let target = tab.map_or_else(|| panel.active.clone(), str::to_owned);
    let attached =
        rpc(&mut panel.pipe, "Target.attachToTarget", json!({"targetId":target,"flatten":true}), None).await?;
    let session = attached["sessionId"].as_str().context("Could not read the tab")?.to_owned();
    let image = rpc(
        &mut panel.pipe,
        "Page.captureScreenshot",
        json!({"format":"png","captureBeyondViewport":false}),
        Some(&session),
    )
    .await;
    rpc(&mut panel.pipe, "Target.detachFromTarget", json!({"sessionId":session}), None).await.ok();
    let image = image?;
    let data = image["data"].as_str().context("The tab gave no image")?;
    Ok(json!({"image":format!("data:image/png;base64,{data}")}))
}

/// Closes the panel's browser.
pub async fn shutdown(screen: &Screen) -> Result<Value> {
    let mut current = screen.panel.lock().await;
    if let Some(mut panel) = current.take() {
        panel.process.terminate_owned().await?;
        let folder = panel.folder.clone();
        // Chrome lets go of its profile a moment after it is stopped; a failed removal is retried.
        if let Err(error) = remove_folder(&folder) {
            tracing::warn!(%error, "Could not remove the closed browser panel profile yet");
            tokio::spawn(async move {
                for _ in 0..6 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if remove_folder(&folder).is_ok() {
                        break;
                    }
                }
            });
        }
    }
    Ok(json!({"open":false,"tabs":[]}))
}

fn remove_folder(folder: &std::path::Path) -> Result<()> {
    if !folder.try_exists()? {
        return Ok(());
    }
    let parent = crate::service::data_dir().join("protected-browsers").canonicalize()?;
    let folder = folder.canonicalize()?;
    ensure!(folder.starts_with(&parent), "The panel folder is outside the protected browsers");
    std::fs::remove_dir_all(folder)?;
    Ok(())
}

/// Dispatches a panel method; `allow_closed` are those that work before the panel exists.
#[cfg(windows)]
pub async fn call(hub: &std::sync::Arc<Hub>, method: &str, body: &Value) -> Result<Value> {
    match method {
        "browserPanel" => state(&hub.screen).await,
        "browserPanelOpen" => open(hub, body["url"].as_str()).await,
        "browserPanelActivate" => activate(&hub.screen, body["tabId"].as_str().context("`tabId` is required")?).await,
        "browserPanelNavigate" => {
            navigate(&hub.screen, body["tabId"].as_str().context("`tabId` is required")?, body["url"].as_str()).await
        }
        "browserPanelClose" => close(&hub.screen, body["tabId"].as_str().context("`tabId` is required")?).await,
        "browserPanelShot" => shot(&hub.screen, body["tabId"].as_str()).await,
        "browserPanelPlace" => place(&hub.screen, body).await,
        "browserPanelShutdown" => shutdown(&hub.screen).await,
        other => bail!("unknown browser panel method `{other}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::url_argument;

    #[test]
    fn only_web_addresses_are_opened() {
        assert_eq!(url_argument(None).unwrap(), "about:blank");
        assert_eq!(url_argument(Some(" https://example.com ")).unwrap(), "https://example.com");
        assert!(url_argument(Some("file:///c:/x")).is_err());
        assert!(url_argument(Some("https://a b.com")).is_err());
        assert!(url_argument(Some(&format!("https://x.com/{}", "a".repeat(2100)))).is_err());
    }
}
