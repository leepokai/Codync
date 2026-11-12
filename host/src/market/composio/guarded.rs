//! Composio execution shares the immutable MCP policy and receipt boundary.
use super::{
    Api, Config, Connection, PREFIX, Result, Store, Value, bail, checked_slug, config, connections, json, post, text,
    tool,
};
use crate::{LockExt, hub::Hub};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn identity(c: &Config, app: &Connection) -> Result<String> {
    let digest = Sha256::digest(serde_json::to_vec(&json!({"key":c.key,"user":c.user_id,
        "account":app.id,"toolkit":app.toolkit}))?);
    Ok(crate::chat::files::digest_hex(&digest))
}

pub(crate) fn installed(store: &Store, connector: &str) -> Result<bool> {
    Ok(connections(store)?.iter().any(|app| app.active() && format!("{PREFIX}{}", app.toolkit) == connector))
}

pub(crate) fn fingerprint(store: &Store, connector: &str) -> Result<String> {
    let c = config(store)?;
    let app = c
        .connections
        .iter()
        .find(|app| app.active() && format!("{PREFIX}{}", app.toolkit) == connector)
        .context("Composio account is no longer connected.")?;
    identity(&c, app)
}

pub(crate) async fn prepare(hub: &Hub, bot: &str, args: &Value, rpc_id: &Value) -> Result<Value> {
    if rpc_id.is_null() || !args["arguments"].is_object() {
        bail!("A request id and object arguments are required.");
    }
    let c = config(&hub.store)?;
    let slug = checked_slug(args["slug"].as_str().unwrap_or_default())?;
    let tool = tool(&Api::of(&hub.store, &c)?, slug).await?;
    let toolkit = tool["toolkit"]["slug"].as_str().context("Tool has no toolkit identity.")?;
    let app = c
        .connections
        .iter()
        .find(|app| app.active() && app.toolkit.eq_ignore_ascii_case(toolkit))
        .context("This Composio app is not connected.")?;
    let connector = format!("{PREFIX}{}", app.toolkit);
    let captured = identity(&c, app)?;
    let _configuration = hub.store.connector_lock.locked();
    let row = hub.store.bot(bot)?.filter(|row| !row.deleted && !row.config.is_group()).context("Unknown bot.")?;
    if !row.config.connectors.contains(&connector) || fingerprint(&hub.store, &connector)? != captured {
        bail!("Composio connection changed or is disabled for this bot.");
    }
    // Only provider metadata can supply a read-only default. Missing annotations mean ask.
    let revoked = hub.store.observe_mcp_tools(&connector, &[json!({"name":slug,"annotations":tool["annotations"]})])?;
    for id in revoked {
        crate::mcp::policy::update_card(hub, &id, "revoked")?;
    }
    Ok(json!({"connectorId":connector,"fingerprint":captured,
        "message":{"jsonrpc":"2.0","id":rpc_id,"method":"tools/call","params":{"name":slug,"arguments":args["arguments"]}}}))
}

pub(crate) async fn dispatch(hub: &Hub, id: &str, expected: &str) -> Result<Value> {
    let (c, app, request) = {
        let _configuration = hub.store.connector_lock.locked();
        let request = hub.store.mcp_call(id)?.context("Unknown request.")?;
        if request.status != "dispatched" {
            bail!("Request is not authorized for dispatch.");
        }
        let c = config(&hub.store)?;
        let app = c
            .connections
            .iter()
            .find(|app| app.active() && format!("{PREFIX}{}", app.toolkit) == request.connector)
            .cloned()
            .context("Composio account disconnected.")?;
        if identity(&c, &app)? != expected {
            bail!("Composio configuration changed.");
        }
        check(hub, id)?;
        // A second HTTP request can read a receipt, but can never repeat the provider execution.
        hub.store.claim_mcp_execution(id)?;
        (c, app, request)
    };
    let slug = checked_slug(&request.tool)?;
    let path = format!("/tools/execute/{slug}");
    let body =
        json!({"user_id":c.user_id,"connected_account_id":app.id,"arguments":request.message["params"]["arguments"]});
    let api = Api::of(&hub.store, &c)?;
    let execution = post(&api, &path, &body);
    let monitor = async {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _configuration = hub.store.connector_lock.locked();
            check(hub, id)?;
            if fingerprint(&hub.store, &request.connector)? != expected {
                bail!("Composio configuration changed.");
            }
        }
        #[allow(unreachable_code)]
        anyhow::Ok(())
    };
    let result = tokio::select! { result=execution=>result, cancelled=monitor=>{
        cancelled?; bail!("Composio execution cancelled; external outcome may be unknown.");
    }}?;
    if !result["successful"].is_boolean() || (result["successful"] == true && result.get("data").is_none()) {
        bail!(
            "Composio returned an ambiguous response. Receipt {id} has an unknown outcome; do not repeat this execution."
        );
    }
    let reply = if result["successful"] == false {
        json!({"jsonrpc":"2.0","id":request.message["id"],"error":{"code":-32603,
            "message":text(&result["error"]).unwrap_or_else(||"Tool failed".into())}})
    } else {
        json!({"jsonrpc":"2.0","id":request.message["id"],"result":result["data"]})
    };
    hub.store.complete_mcp_call(id, &reply)?;
    Ok(reply)
}

fn check(hub: &Hub, id: &str) -> Result<()> {
    let request = hub.store.mcp_call(id)?.context("Unknown request.")?;
    let bot =
        hub.store.bot(&request.bot)?.filter(|row| !row.deleted && !row.config.is_group()).context("Unknown bot.")?;
    if request.status != "dispatched"
        || !bot.config.connectors.contains(&request.connector)
        || hub.store.mcp_policy(&request.connector, &request.tool)? == crate::store::mcp_policies::Mode::Deny
    {
        bail!("Composio authorization revoked; external outcome may be unknown.");
    }
    Ok(())
}
