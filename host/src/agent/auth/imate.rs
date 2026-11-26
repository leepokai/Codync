//! iMate agents live on imate.woa.com, not on this computer. The host fetches the
//! signed-in user's list to fill the model picker and passes the personal token to
//! the stdio bridge as `IMATE_API_TOKEN`.

use super::Status;
use crate::agent::login_profiles;
use crate::analytics::{Event, capture};
use crate::http;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use codync_agent::backends;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;

const TOKEN: &str = "IMATE_API_TOKEN";
const BASE: &str = "https://imate.woa.com/server/web-api";
/// A colleague list. Short enough that the next ask starts its own lookup instead of waiting on this one.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Checking a token before it replaces the one already saved.
const TOKEN_CHECK: Duration = Duration::from_secs(20);
/// Unlock plus the check. The phone gives up at 60 seconds, so a save past this never starts.
const SAVE_BUDGET: Duration = Duration::from_secs(45);

/// What the phone needs to show iMate's token field and sign-in state.
pub async fn status(hub: Arc<Hub>) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    Ok(json!({
        "signedIn": saved_token(&hub.store)?.is_some(),
        "loginKeys": [TOKEN],
        "installHint": "Add an iMate personal API token in Login profiles.",
    }))
}

/// Saves the personal token in the host's vault and refreshes the agent catalog.
/// iMate has to accept the token first. A refusal leaves the token already saved in place.
pub async fn set_token(hub: Arc<Hub>, token: &str) -> Result<Value> {
    let token = token.trim();
    if token.is_empty() {
        bail!("token is empty");
    }
    let checked = tokio::time::timeout(SAVE_BUDGET, async {
        crate::market::vault::unlock(hub.clone()).await?;
        fetch_colleagues(token, TOKEN_CHECK).await
    })
    .await;
    match checked {
        Ok(Ok(_)) => listed(&hub, "success", None, "save"),
        Ok(Err(error)) => {
            listed(&hub, "failed", Some(failure_category(&error)), "save");
            return Err(if failure_category(&error) == "auth" {
                error.context("iMate rejected this token")
            } else {
                error
            });
        }
        Err(_) => {
            let error = anyhow::anyhow!("the iMate token check timed out");
            listed(&hub, "failed", Some(failure_category(&error)), "save");
            return Err(error);
        }
    }
    super::save_env(&hub.store, "agent-env:imate", &[(TOKEN.to_owned(), Some(token.to_owned()))])?;
    backends::set_signed_in("imate", Some(true));
    hub.model_lists.clear("imate");
    // A bridge already running still has the token it started with.
    hub.restart_agents("imate");
    status(hub).await
}

/// `setAgentEnv` for iMate. A token is checked with iMate before it replaces the saved one, and a
/// blank one is removed rather than kept as a sign-in. The token is not saved in the same call as
/// other variables: a refused token must not leave those other variables changed.
pub async fn save_agent_env(hub: Arc<Hub>, vars: &Map<String, Value>) -> Result<Value> {
    let mut rest = vars.clone();
    let Some(value) = rest.remove(TOKEN) else {
        return if rest.is_empty() {
            super::check(&hub.store, "imate").await
        } else {
            super::set_env(&hub.store, "imate", &rest).await
        };
    };
    if !rest.is_empty() {
        bail!("save the iMate token separately from other variables");
    }
    let text = value.as_str().map_or("", str::trim);
    if text.is_empty() {
        super::save_env(&hub.store, "agent-env:imate", &[(TOKEN.to_owned(), None)])?;
        hub.model_lists.clear("imate");
        hub.restart_agents("imate");
    } else {
        set_token(hub.clone(), text).await?;
    }
    super::check(&hub.store, "imate").await
}

/// Whether an iMate API key has been entered, for the agent's setup card.
pub fn check(store: &Store) -> Result<Status> {
    let signed_in = saved_token(store)?.is_some();
    Ok(Status { signed_in: Some(signed_in), detail: None, methods: Vec::new() })
}

/// The token a bridge starts with for `login_profile`. A named profile uses only its own token.
/// With no profile, only the computer's token: clearing that token does not borrow a profile's.
/// The model picker uses the same choice, so the colleagues it lists are the ones that bot can reach.
pub(crate) fn token_for(store: &Store, login_profile: Option<&str>) -> Result<Option<String>> {
    // The same lookup the launch uses: a profile of another agent does not count, even when its
    // saved variables happen to include this name.
    if let Some(id) = login_profile.filter(|id| !id.is_empty()) {
        let (env, _) = login_profiles::env(store, "imate", Some(id))?;
        let token = env.into_iter().find(|(name, value)| name == TOKEN && !value.is_empty()).map(|(_, token)| token);
        return Ok(token);
    }
    saved_token(store)
}

/// Puts the chosen colleague and, when the login didn't, its token on `env`. A chat and a background
/// job (naming, a call title) both go through here, so they open the same tunnel.
pub(crate) fn apply_launch(
    store: &Store,
    model: Option<&str>,
    login_profile: Option<&str>,
    env: &mut Vec<(String, String)>,
) -> Result<()> {
    if let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) {
        env.retain(|(name, _)| name != "IMATE_CLIENT_UUID");
        env.push(("IMATE_CLIENT_UUID".into(), model.to_owned()));
    }
    if let Some(id) = login_profile.filter(|id| !id.is_empty()) {
        // Launch already copied the host token when this profile saved nothing. Drop it: this
        // login runs only on its own token.
        env.retain(|(name, _)| name != "IMATE_API_TOKEN");
        match token_for(store, Some(id))? {
            Some(token) => env.push(("IMATE_API_TOKEN".into(), token)),
            None => bail!("this iMate login has no token; sign in again"),
        }
        return Ok(());
    }
    // The default login uses only the computer's token. Drop anything already on the process so a
    // cleared token cannot keep running, and do not fill it from a named profile. With no saved
    // token the bridge must not start: a missing entry would let it inherit this process's token.
    env.retain(|(name, _)| name != "IMATE_API_TOKEN");
    match token_for(store, None)? {
        Some(token) => env.push(("IMATE_API_TOKEN".into(), token)),
        None => bail!("this iMate login has no token; sign in again"),
    }
    Ok(())
}

/// The computer's own iMate token. A named profile's token is not a stand-in for it.
fn saved_token(store: &Store) -> Result<Option<String>> {
    let Some(raw) = crate::market::vault::read(store, "agent-env:imate")? else { return Ok(None) };
    let vars: Map<String, Value> = serde_json::from_str(&raw)?;
    Ok(token_text(vars.get(TOKEN)))
}

fn token_text(value: Option<&Value>) -> Option<String> {
    let token = value.and_then(Value::as_str)?.trim();
    if token.is_empty() { None } else { Some(token.to_owned()) }
}

/// The signed-in user's iMate colleagues, in the shape the shared model picker reads.
/// `login_profile` is the bot's login, so the list is the one that token can see.
pub async fn shared_models(hub: &Arc<Hub>, login_profile: Option<&str>) -> Result<Value> {
    // The vault stays locked across a host restart until something opens it. Listing colleagues is that read.
    crate::market::vault::unlock(hub.clone()).await?;
    let login_profile = login_profile.filter(|id| !id.is_empty());
    hub.model_lists
        .get("imate", login_profile, async {
            let result = list_colleagues(&hub.store, login_profile).await;
            match &result {
                Ok(_) => listed(hub, "success", None, "list"),
                Err(error) => listed(hub, "failed", Some(failure_category(error)), "list"),
            }
            result
        })
        .await
}

async fn list_colleagues(store: &Store, login_profile: Option<&str>) -> Result<Value> {
    let Some(token) = token_for(store, login_profile)? else {
        bail!(
            "{}",
            if login_profile.is_some() {
                "this iMate login has no token; sign in again"
            } else {
                "add an iMate API token in Extensions first"
            }
        );
    };
    let listed = fetch_colleagues(&token, LOOKUP_TIMEOUT).await?;
    // The same order as the bridge's default colleague: first by name, so "Default" is that person.
    let mut models: Vec<Value> = listed
        .iter()
        .filter_map(|imate| {
            let id = imate["clientUuid"].as_str()?;
            Some(json!({"id": id, "name": colleague_name(imate), "description": null, "options": []}))
        })
        .collect();
    models.sort_by(|a, b| a["name"].as_str().unwrap_or_default().cmp(b["name"].as_str().unwrap_or_default()));
    Ok(json!({
        "models": models,
        "currentModelId": models.first().and_then(|m| m["id"].as_str()).unwrap_or_default(),
        "defaultModelOptions": false,
        // Empty: no colleague is the unset "Default" row. A name that starts with Default stays selectable by id.
        "defaultModelId": "",
        "fastMode": false,
        "modes": [],
    }))
}

/// The name the picker sorts by. Empty names sort as the colleague's id, the same as the bridge.
pub(crate) fn colleague_name(imate: &Value) -> &str {
    imate["botName"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| imate["clientUuid"].as_str().unwrap_or_default())
}

fn listed(hub: &Hub, outcome: &str, category: Option<&str>, when: &str) {
    let mut properties = Map::new();
    properties.insert("outcome".into(), json!(outcome));
    properties.insert("when".into(), json!(when));
    if let Some(category) = category {
        properties.insert("category".into(), json!(category));
    }
    capture(hub, Event::ImateAgentsListed, properties);
}

/// The colleagues `token` can see. `timeout` bounds the whole request, so a silent iMate cannot hold the lookup open.
async fn fetch_colleagues(token: &str, timeout: Duration) -> Result<Vec<Value>> {
    let mut headers = HeaderMap::new();
    headers
        .insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).context("invalid iMate API token")?);
    let response = http()
        .get(format!("{BASE}/api/v1/open/imates"))
        .headers(headers)
        .timeout(timeout)
        .send()
        .await
        .context("listing iMate agents")?;
    let status = response.status();
    let body = response.text().await.context("reading the iMate agent list")?;
    colleagues_from(status.as_u16(), &body)
}

fn colleagues_from(status: u16, body: &str) -> Result<Vec<Value>> {
    if !(200..300).contains(&status) {
        bail!("iMate agent list failed ({status}): {}", first_line(body));
    }
    let value: Value = serde_json::from_str(first_line(body)).context("decoding the iMate agent list")?;
    if value["code"] != 0 {
        bail!("iMate agent list failed: {}", value["message"].as_str().unwrap_or("unknown error"));
    }
    value["data"].as_array().cloned().context("iMate agent list response is missing a data array")
}

fn first_line(body: &str) -> &str {
    body.split('\n').next().unwrap_or(body)
}

fn failure_category(error: &anyhow::Error) -> &'static str {
    let text = format!("{error:#}").to_lowercase();
    if text.contains("timed out") || text.contains("timeout") {
        "timeout"
    } else if text.contains("401")
        || text.contains("403")
        || text.contains("invalid token")
        || text.contains("rejected")
    {
        "auth"
    } else if text.contains("decode") || text.contains("parse") {
        "protocol"
    } else {
        "network"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_ignores_the_framework_suffix() {
        assert_eq!(first_line("{\"error\":\"x\"}\n{\"code\":0}"), "{\"error\":\"x\"}");
    }

    #[test]
    fn a_refused_token_is_not_a_colleague_list() {
        let denied = colleagues_from(401, "{\"error\":\"unauthorized\"}\n{\"code\":0}");
        assert!(denied.unwrap_err().to_string().contains("401"));
        let rejected = colleagues_from(200, "{\"code\":10001,\"message\":\"invalid token\"}");
        assert!(rejected.unwrap_err().to_string().contains("invalid token"));
        let accepted = colleagues_from(200, "{\"code\":0,\"data\":[{\"clientUuid\":\"a\"}]}").expect("colleagues");
        assert_eq!(accepted[0]["clientUuid"], "a");
        assert_eq!(colleagues_from(200, "{\"code\":0,\"data\":[]}").expect("an empty list"), Vec::<Value>::new());
        assert!(colleagues_from(200, "{\"code\":0}").is_err(), "a missing data array is not an empty list");
    }

    #[test]
    fn failure_categories_never_include_the_error_text() {
        assert_eq!(failure_category(&anyhow::anyhow!("iMate token rejected: secret")), "auth");
        assert_eq!(failure_category(&anyhow::anyhow!("the iMate token check timed out")), "timeout");
        assert_eq!(failure_category(&anyhow::anyhow!("request timed out")), "timeout");
        assert_eq!(failure_category(&anyhow::anyhow!("connection refused")), "network");
    }

    #[tokio::test]
    async fn a_named_profile_without_a_token_does_not_use_another_account() {
        use crate::LockExt;
        let store = Store::open(std::path::Path::new(":memory:")).expect("store");
        *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![7; 32]));
        crate::agent::auth::save_env(&store, "agent-env:imate", &[(TOKEN.to_owned(), Some("host-token".into()))])
            .expect("host token");
        let added = login_profiles::add(&store, "imate", login_profiles::Kind::ApiKey).await.expect("profile");
        let id = added["id"].as_str().expect("id").to_owned();
        assert_eq!(token_for(&store, Some(&id)).expect("lookup"), None, "no host token");
        assert_eq!(token_for(&store, None).expect("computer").as_deref(), Some("host-token"));
        let mut env = vec![("IMATE_API_TOKEN".into(), "host-token".into())];
        let err = apply_launch(&store, None, Some(&id), &mut env).expect_err("must not start");
        assert!(err.to_string().contains("sign in again"), "{err}");
        assert!(env.iter().all(|(name, _)| name != "IMATE_API_TOKEN"));

        crate::agent::auth::save_env(&store, "agent-env:imate", &[(TOKEN.to_owned(), None)]).expect("cleared");
        let other = login_profiles::add(&store, "imate", login_profiles::Kind::ApiKey).await.expect("other profile");
        let other_id = other["id"].as_str().expect("id");
        let vars = json!({"IMATE_API_TOKEN": "other-token"});
        login_profiles::keys::set(&store, other_id, vars.as_object().expect("vars"), &[]).await.expect("other token");
        assert_eq!(token_for(&store, Some(&id)).expect("still empty"), None, "no other profile");
        assert_eq!(token_for(&store, None).expect("default"), None, "a cleared computer token does not take a profile");
        let err = apply_launch(&store, None, None, &mut env).expect_err("default login does not borrow a profile");
        assert!(err.to_string().contains("sign in again"), "{err}");

        let own = json!({"IMATE_API_TOKEN": "profile-token"});
        login_profiles::keys::set(&store, &id, own.as_object().expect("vars"), &[]).await.expect("own token");
        assert_eq!(token_for(&store, Some(&id)).expect("own").as_deref(), Some("profile-token"));
        let mut env = vec![("IMATE_API_TOKEN".into(), "host-token".into())];
        apply_launch(&store, None, Some(&id), &mut env).expect("starts on its own token");
        assert!(env.iter().any(|(name, value)| name == "IMATE_API_TOKEN" && value == "profile-token"));
    }

    #[test]
    fn a_cleared_token_does_not_start_on_one_the_process_inherited() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("store");
        let mut env = vec![(TOKEN.to_owned(), "inherited".into())];
        let err = apply_launch(&store, None, None, &mut env).expect_err("no saved token");
        assert!(err.to_string().contains("sign in again"), "{err}");
        assert!(env.iter().all(|(name, _)| name != TOKEN), "the inherited token is not passed on");
    }
}
