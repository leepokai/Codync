//! Source: Hermes plugins/platforms/{signal,email}, and official Home Assistant plugin (MIT).
//! Adaptation: native transports configured through Codync's existing encrypted app connections.

use crate::{
    market::{Connector, openbot},
    store::Store,
};
use anyhow::Result;
use serde_json::{Value, json};

fn field(id: &str, label: &str, required: bool, hint: &str) -> Value {
    json!({"id":id,"env":id,"label":label,"required":required,"hint":hint})
}

pub(crate) fn browse(store: &Store, search: &str) -> Result<Vec<Value>> {
    catalog(&crate::market::connectors(store)?, search)
}

fn catalog(installed: &[Connector], search: &str) -> Result<Vec<Value>> {
    let query = search.trim().to_lowercase();
    let mut apps = Vec::new();
    for (kind, name, description, mut fields) in [
        (
            "signal",
            "Signal",
            "Talk to a Codync bot through an existing signal-cli session. Native Signal identities, quotes and file transport are retained.",
            vec![
                field(
                    "SIGNAL_HTTP_URL",
                    "signal-cli HTTP URL",
                    false,
                    "Leave blank to reuse the Hermes profile; default http://127.0.0.1:8080.",
                ),
                field(
                    "SIGNAL_ACCOUNT",
                    "Signal account",
                    false,
                    "Existing linked Signal number, including country code. No new Signal identity is created.",
                ),
                field(
                    "SIGNAL_ALLOWED_USERS",
                    "Allowed Signal users",
                    false,
                    "Comma-separated native phone numbers or UUIDs; blank reuses the profile. Additional DMs can pair with a one-use code.",
                ),
                field(
                    "SIGNAL_GROUP_ALLOWED_USERS",
                    "Allowed Signal groups",
                    false,
                    "Native group IDs, comma-separated. Blank leaves groups disabled unless the profile explicitly permits them.",
                ),
            ],
        ),
        (
            "email",
            "Email",
            "Receive authenticated mail through the native Hermes IMAP adapter or a signed RFC822 webhook; send threaded replies and attachments through the existing SMTP account.",
            vec![
                field("EMAIL_ADDRESS", "Email address", false, "Leave blank to reuse the configured Hermes mailbox."),
                field(
                    "EMAIL_PASSWORD",
                    "Mailbox password or app password",
                    false,
                    "Leave blank to reuse the native profile credential. Saved only in the host vault.",
                ),
                field("EMAIL_IMAP_HOST", "IMAP host", false, "Leave blank to use the native profile."),
                field("EMAIL_SMTP_HOST", "SMTP host", false, "Leave blank to use the native profile."),
                field(
                    "EMAIL_AUTHSERV_ID",
                    "Receiving MTA authserv-id",
                    false,
                    "Exact trusted Authentication-Results server ID. Sender authentication is kept fail closed.",
                ),
                field(
                    "EMAIL_ALLOWED_USERS",
                    "Allowed senders",
                    false,
                    "Comma-separated authenticated sender addresses; blank reuses native policy. Pairing never bypasses sender authentication.",
                ),
                field(
                    "CODYNC_CHANNEL_WEBHOOK_SECRET",
                    "Signed email webhook secret",
                    false,
                    "Optional 32+ character secret for /hooks/native-email/<connection-id>. Payload is RFC822; native sender authentication still applies.",
                ),
            ],
        ),
        (
            "homeassistant",
            "Home Assistant",
            "Watch explicitly selected home entities as text events and deliver bot replies as persistent notifications through the native Hermes adapter.",
            vec![
                field("HASS_URL", "Home Assistant URL", false, "Leave blank to reuse the native Hermes configuration."),
                field(
                    "HASS_TOKEN",
                    "Existing long-lived access token",
                    false,
                    "Leave blank to reuse HASS_TOKEN in the native profile. Codync does not request broader permissions.",
                ),
                field(
                    "HASS_WATCH_ENTITIES",
                    "Watched entity IDs",
                    true,
                    "Comma-separated exact entity IDs, e.g. binary_sensor.front_door,sensor.temperature.",
                ),
            ],
        ),
    ] {
        if !query.is_empty() && !format!("{name} {description}").to_lowercase().contains(&query) {
            continue;
        }
        fields.extend([
            field(
                "CODYNC_CHANNEL_BOT",
                "Codync bot ID",
                false,
                "Optional. Otherwise enable this connection for exactly one bot.",
            ),
            field(
                "CODYNC_CHANNEL_HERMES_HOME",
                "Existing Hermes profile directory",
                false,
                "Optional native HERMES_HOME. Identity and credentials stay in that profile.",
            ),
            field(
                "CODYNC_CHANNEL_ROOT",
                "Pinned Hermes source directory",
                false,
                "Optional; autodetects the pinned Hermes installation.",
            ),
            field(
                "CODYNC_CHANNEL_PYTHON",
                "Hermes Python interpreter",
                false,
                "Optional; use the interpreter from Hermes's own virtual environment.",
            ),
        ]);
        if kind != "homeassistant" {
            fields.push(field("CODYNC_CHANNEL_PAIR_CODE","One-use DM pairing code",false,"Optional 12–64 letters or digits. Send /pair <code> from the sender's DM within ten minutes after connection. Used/expired codes never reopen on restart; change this field for a new code."));
        }
        let id = format!("hermes-channel-{kind}");
        let app: openbot::App = serde_json::from_value(
            json!({"id":id,"name":name,"tagline":description,"description":description,"category":"communication","creator":"NousResearch","version":"10d25f8a","featured":false,"setup":"key","server":{"name":name,"transport":"stdio","command":super::config::COMMAND,"args":[]},"fields":fields,"steps":[],"prompts":[],"website":"https://github.com/NousResearch/hermes-agent"}),
        )?;
        let mut card = openbot::app_public(&app, installed.iter().any(|c| c.catalog.as_deref() == Some(&id)));
        card["origin"]=format!("https://github.com/NousResearch/hermes-agent/tree/10d25f8ad65fdf71304e9272959421a0c22e5d23/plugins/platforms/{kind}").into();
        if kind == "homeassistant" {
            card["origin"]="https://github.com/NousResearch/hermes-official-plugins/tree/e7a218f26db88426ab3e7376f0e000b6e356f5e7/homeassistant".into();
        }
        card["license"] = "MIT".into();
        apps.push(card);
    }
    Ok(apps)
}

pub(crate) fn listings(store: &Store, search: &str) -> Result<Vec<Value>> {
    Ok(browse(store, search)?
        .into_iter()
        .map(|app| {
            let mut card = server_card(app);
            card["options"] = json!(options(&card));
            card
        })
        .collect())
}

fn server_card(app: Value) -> Value {
    json!({"name":format!("codync-native/{}",app["id"].as_str().unwrap_or_default().trim_start_matches("hermes-channel-")),"title":app["name"],"description":app["description"],"version":"10d25f8a","websiteUrl":app["origin"],"installed":app["installed"],"codyncNativeApp":app})
}

pub(crate) fn server(name: &str) -> Result<Option<Value>> {
    let Some(kind) = name.strip_prefix("codync-native/") else { return Ok(None) };
    // Catalog construction needs no secrets; installed state is irrelevant to a registry lookup.
    Ok(catalog(&[], "")?.into_iter().find(|app| app["id"] == format!("hermes-channel-{kind}")).map(server_card))
}

pub(crate) fn options(server: &Value) -> Vec<Value> {
    let inputs:Vec<_>=server["codyncNativeApp"]["fields"].as_array().into_iter().flatten().map(|f|json!({"name":f["id"],"description":format!("{} — {}",f["label"].as_str().unwrap_or_default(),f["hint"].as_str().unwrap_or_default()),"required":f["required"],"secret":true})).collect();
    vec![json!({"id":"native:0","kind":"local","label":"Native channel · existing session","inputs":inputs})]
}

pub(crate) async fn install(store: &Store, name: &str, option: &str, inputs: &Value) -> Result<Value> {
    anyhow::ensure!(option == "native:0", "Unknown native channel install option");
    let server = server(name)?.ok_or_else(|| anyhow::anyhow!("Unknown native channel"))?;
    let app = &server["codyncNativeApp"];
    let mut env = serde_json::Map::new();
    for field in app["fields"].as_array().into_iter().flatten() {
        let key = field["id"].as_str().ok_or_else(|| anyhow::anyhow!("Native field ID missing"))?;
        let value = inputs[key].as_str().map(str::trim).unwrap_or_default();
        anyhow::ensure!(!value.is_empty() || field["required"] != true, "{} is required", key);
        env.insert(key.into(), value.into());
    }
    crate::market::add_custom_connector(
        store,
        &json!({"name":app["name"],"catalog":app["id"],"command":super::config::COMMAND,"args":[],"env":env}),
    )
    .await
}

pub(crate) fn verify(store: &Store, c: &Connector) -> Result<Value> {
    let state = super::config::state(store, &c.id)?;
    let matching = state.as_ref().filter(|v| v["base"] == super::config::fingerprint(c));
    let status = matching.and_then(|v| v["status"].as_str()).unwrap_or("starting");
    anyhow::ensure!(
        status == "ready",
        "Native channel is {status}. Enable it for exactly one bot, check its existing native account, then retry."
    );
    Ok(json!({"status":"ready","nativeChannel":true}))
}

pub(crate) fn decorate(store: &Store, c: &Connector, value: &mut Value) -> Result<()> {
    if !super::is_native(c) {
        return Ok(());
    }
    let state = super::config::state(store, &c.id)?.filter(|v| v["base"] == super::config::fingerprint(c));
    let status = state.as_ref().and_then(|s| s["status"].as_str()).unwrap_or("starting");
    value["nativeChannel"] = json!({"platform":super::config::kind(c),"status":status});
    if status == "ready" {
        value.as_object_mut().map(|v| v.remove("lastFailure"));
    } else {
        value["lastFailure"] = json!({"kind":"tool","message":format!("Native channel: {status}. Check its existing account and enable this connection for exactly one bot."),"at":state.as_ref().map(|s|s["updatedAt"].clone()).unwrap_or(Value::Null)});
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_apps_use_shared_fields_and_never_export_a_fake_mcp_process() {
        let cards = catalog(&[], "").unwrap();
        assert_eq!(cards.len(), 3);
        for app in cards {
            assert_eq!(app["setup"], "key");
            assert_eq!(app["server"]["command"], super::super::config::COMMAND);
            let mut c: Connector = serde_json::from_value(
                json!({"id":"native","name":"Native","catalog":app["id"],"command":super::super::config::COMMAND}),
            )
            .unwrap();
            if app["id"] == "hermes-channel-homeassistant" {
                c.env.insert("HASS_WATCH_ENTITIES".into(), "sensor.temperature".into());
            }
            super::super::prepare_connector(&mut c).unwrap();
            assert!(c.acp(std::path::Path::new("host"), 1).is_none());
            let card = server_card(app);
            let options = options(&card);
            assert_eq!(options[0]["id"], "native:0");
            assert!(!options[0]["inputs"].as_array().unwrap().is_empty());
        }
    }
}
