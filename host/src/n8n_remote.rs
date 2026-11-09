//! What a paired phone sees and does of this computer's n8n: whether it runs, its workflows (with a
//! switch to publish or unpublish each) and its latest runs. The host asks n8n's public API
//! (`/api/v1`, header `X-AUTOMAN-API-KEY`) with a key it creates once, for the owner it seeded, and
//! keeps in its store; the phone never sees the key nor the editor's session cookie (see [`view`]).
//!
//! Routes and fields are those of the `automan` public API (`packages/cli/src/public-api/v1`):
//! `GET /workflows`, `POST /workflows/{id}/publish|unpublish` (`activate|deactivate` on an instance
//! that predates them) and `GET /executions`.

use crate::api::devices::Caller;
use crate::hub::Hub;
use crate::n8n::{KV_OWNER_EMAIL, KV_OWNER_PASSWORD};
use anyhow::{Context, Result, anyhow, bail, ensure};
use reqwest::Method;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// The public API key the host made for itself.
const KV_API_KEY: &str = "n8n.apiKey";
const KEY_HEADER: &str = "X-AUTOMAN-API-KEY";
/// What the key may do: list and switch workflows, and read runs. Nothing that edits or deletes.
const SCOPES: [&str; 6] =
    ["workflow:list", "workflow:read", "workflow:activate", "workflow:deactivate", "execution:list", "execution:read"];
const WORKFLOWS_LIMIT: u32 = 100;
const EXECUTIONS_DEFAULT: u32 = 20;
const EXECUTIONS_MAX: u32 = 50;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// The n8n state without the editor's session cookie: the key stays, with no value.
pub fn without_session(mut state: Value) -> Value {
    state["auth"] = Value::Null;
    state
}

/// The n8n state a caller may see: the session cookie is for this computer's own app only.
pub fn view(caller: &Caller, state: Value) -> Value {
    if matches!(caller, Caller::Local) { state } else { without_session(state) }
}

/// The instance's workflows, name order as n8n lists them.
pub async fn workflows(hub: &Arc<Hub>) -> Result<Value> {
    let body = call(hub, Method::GET, "/workflows", &[("limit", WORKFLOWS_LIMIT.to_string())], None).await?;
    Ok(workflow_list(&body))
}

/// Publishes or unpublishes one workflow.
pub async fn set_workflow_active(hub: &Arc<Hub>, id: &str, active: bool) -> Result<Value> {
    ensure!(valid_id(id), "that is not a workflow id");
    let (now, before) = if active { ("publish", "activate") } else { ("unpublish", "deactivate") };
    match call(hub, Method::POST, &format!("/workflows/{id}/{now}"), &[], Some(json!({}))).await {
        // An instance older than publishing only knows the first names; a workflow that doesn't exist
        // is a 404 on both.
        Err(e) if e.downcast_ref::<NotFound>().is_some() => {
            call(hub, Method::POST, &format!("/workflows/{id}/{before}"), &[], Some(json!({}))).await?;
        }
        done => {
            done?;
        }
    }
    Ok(json!({ "id": id, "active": active }))
}

/// The latest runs, newest first, of one workflow or of all.
pub async fn executions(hub: &Arc<Hub>, workflow_id: Option<&str>, limit: Option<u64>) -> Result<Value> {
    let limit =
        limit.map_or(EXECUTIONS_DEFAULT, |l| u32::try_from(l).unwrap_or(EXECUTIONS_MAX)).clamp(1, EXECUTIONS_MAX);
    let mut query = vec![("limit", limit.to_string())];
    if let Some(id) = workflow_id {
        ensure!(valid_id(id), "that is not a workflow id");
        query.push(("workflowId", id.to_owned()));
    }
    let body = call(hub, Method::GET, "/executions", &query, None).await?;
    Ok(execution_list(&body))
}

/// An id n8n generates: letters, digits, `-` and `_`. Anything else never reaches a URL.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A request n8n answered with 404.
#[derive(Debug)]
struct NotFound;

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("n8n doesn't know that")
    }
}

impl std::error::Error for NotFound {}

/// One call to the public API; a key n8n refuses (a reset database forgets its keys) is made again once.
async fn call(
    hub: &Arc<Hub>,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
) -> Result<Value> {
    ensure!(hub.n8n.running(), "n8n is not running: start it first");
    let base = format!("http://127.0.0.1:{}", hub.n8n.port());
    let url =
        reqwest::Url::parse_with_params(&format!("{base}/api/v1{path}"), query).context("building the n8n address")?;
    for attempt in 0..2 {
        let key = api_key(hub, &base, attempt > 0).await?;
        let mut request =
            crate::http().request(method.clone(), url.clone()).header(KEY_HEADER, key).timeout(CALL_TIMEOUT);
        if let Some(body) = &body {
            request = request.json(body);
        }
        let resp = request.send().await.context("asking n8n")?;
        let status = resp.status();
        if status.as_u16() == 401 && attempt == 0 {
            continue;
        }
        let reply: Value = resp.json().await.unwrap_or(Value::Null);
        if status.as_u16() == 404 {
            return Err(anyhow!(NotFound));
        }
        if !status.is_success() {
            bail!("n8n said no ({status}): {}", reply["message"].as_str().unwrap_or("no reason given"));
        }
        return Ok(reply);
    }
    bail!("n8n refused the host's API key twice")
}

/// The host's public API key: the stored one, or a new one made by logging in as the seeded owner.
async fn api_key(hub: &Arc<Hub>, base: &str, renew: bool) -> Result<String> {
    // Two calls at once must not make two keys.
    static MAKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = MAKING.lock().await;
    let store = &hub.store;
    if !renew && let Some(key) = store.kv_get(KV_API_KEY) {
        return Ok(key);
    }
    let (Some(email), Some(password)) = (store.kv_get(KV_OWNER_EMAIL), store.kv_get(KV_OWNER_PASSWORD)) else {
        bail!("n8n has no owner yet: it creates one the first time it runs");
    };
    let client = reqwest::Client::builder().cookie_store(true).build().context("building the n8n client")?;
    let login = client
        .post(format!("{base}/rest/login"))
        .json(&json!({ "emailOrLdapLoginId": email, "password": password }))
        .timeout(CALL_TIMEOUT)
        .send()
        .await
        .context("logging into n8n")?;
    let status = login.status();
    login.error_for_status().with_context(|| format!("logging into n8n ({status})"))?;
    let made = client
        .post(format!("{base}/rest/api-keys"))
        .json(&json!({ "label": "Codync host", "scopes": SCOPES, "expiresAt": null }))
        .timeout(CALL_TIMEOUT)
        .send()
        .await
        .context("making n8n's API key")?;
    let status = made.status();
    let reply: Value = made.json().await.unwrap_or(Value::Null);
    ensure!(
        status.is_success(),
        "n8n wouldn't make an API key ({status}): {}",
        reply["message"].as_str().unwrap_or("its public API may be turned off")
    );
    let key = raw_key(&reply).ok_or_else(|| anyhow!("n8n's reply had no API key"))?.to_owned();
    store.kv_set(KV_API_KEY, &key)?;
    Ok(key)
}

/// The key n8n shows once, at creation: `rawApiKey`, possibly inside n8n's `data` wrapper.
fn raw_key(reply: &Value) -> Option<&str> {
    reply["rawApiKey"]
        .as_str()
        .or_else(|| reply["data"]["rawApiKey"].as_str())
        .filter(|k| !k.is_empty() && !k.starts_with('*'))
}

/// A string n8n sends as text or as a number (execution ids are either).
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `{ "workflows": [...], "more": bool }` from a `GET /workflows` reply. A workflow counts as on when
/// it has a published version (`activeVersionId`) or, on an older instance, `active`.
fn workflow_list(body: &Value) -> Value {
    let rows: Vec<Value> = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| w["isArchived"] != true)
        .filter_map(|w| {
            Some(json!({
                "id": text(&w["id"])?,
                "name": w["name"].as_str().unwrap_or_default(),
                "active": w["activeVersionId"].is_string() || w["active"] == true,
                "updatedAt": w["updatedAt"],
            }))
        })
        .collect();
    json!({ "workflows": rows, "more": body["nextCursor"].is_string() })
}

/// `{ "executions": [...] }` from a `GET /executions` reply.
fn execution_list(body: &Value) -> Value {
    let rows: Vec<Value> = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(json!({
                "id": text(&e["id"])?,
                "workflowId": text(&e["workflowId"]),
                "status": e["status"].as_str().unwrap_or("unknown"),
                "mode": e["mode"].as_str().unwrap_or_default(),
                "startedAt": e["startedAt"],
                "stoppedAt": e["stoppedAt"],
            }))
        })
        .collect();
    json!({ "executions": rows })
}

#[cfg(test)]
mod tests;
