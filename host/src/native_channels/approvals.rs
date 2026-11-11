use super::config;
use crate::{agent::bot::Cmd, hub::Hub, market::Connector};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fmt::Write as _;

pub(super) fn prepare(hub: &Hub, c: &Connector, generation: &str, item: &Value) -> Result<Option<Value>> {
    let mut user: Option<String> = None;
    let ids = item["data"]["inputEntryIds"].as_array().filter(|v| !v.is_empty());
    let Some(ids) = ids else { return Ok(None) };
    for id in ids {
        let Some(input) = id.as_str().map(|s| hub.store.entry_checked(s)).transpose()?.flatten() else {
            return Ok(None);
        };
        let f = &input.data["nativeFence"];
        if input.kind != "user"
            || input.data["source"] != "native-channel"
            || input.bot_id != item["bot"].as_str().unwrap_or_default()
            || input.thread_id.as_deref() != item["root"].as_str()
            || f["connection"] != c.id
            || f["generation"] != generation
            || f["chat"] != item["chat"]
        {
            return Ok(None);
        }
        let Some(sender) = f["user"].as_str() else { return Ok(None) };
        if user.as_deref().is_some_and(|prior| prior != sender) {
            return Ok(None);
        }
        user = Some(sender.to_owned());
    }
    let Some(user) = user else { return Ok(None) };
    if !config::authorized(hub, c, generation, &user)? {
        return Ok(None);
    }
    let options = item["data"]["options"].as_array().context("Approval options are missing")?;
    if options.is_empty() || options.len() > 20 {
        return Ok(None);
    }
    let code = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
    let mut text =
        format!("Approval requested by Codync:\n{}", item["data"]["title"].as_str().unwrap_or("Choose an option"));
    for (i, option) in options.iter().enumerate() {
        let Some(name) = option["name"].as_str() else { return Ok(None) };
        if option["optionId"].as_str().is_none() {
            return Ok(None);
        }
        let _ = write!(text, "\n{}. {}", i + 1, crate::agent::acp::truncate(name, 250));
    }
    let _ = write!(text, "\n\nReply /approve {code} <number>. Only the original sender may answer.");
    Ok(Some(
        json!({"id":item["id"],"chat":item["chat"],"replyTo":item["replyTo"],"user":user,"text":text,"files":[],"code":code,"bot":item["bot"],"options":options}),
    ))
}

pub(super) fn answer(hub: &Hub, c: &Connector, generation: &str, user: &str, chat: &str, text: &str) -> Result<Value> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    ensure!(words.len() == 3 && words[0] == "/approve" && words[1].len() == 12, "Use /approve <code> <number>");
    let number = words[2].parse::<usize>().context("Approval choice must be a number")?;
    let (entry, payload) =
        hub.store.native_card(&c.id, generation, words[1])?.context("Approval is unknown or expired")?;
    ensure!(
        payload["user"] == user && payload["chat"] == chat,
        "This decision belongs to another sender or conversation"
    );
    ensure!(config::bot(hub, c).ok().as_deref() == payload["bot"].as_str(), "Approval bot binding changed");
    let current = hub
        .store
        .entry_checked(&entry)?
        .filter(|e| e.kind == "permission" && e.data["status"] == "pending")
        .context("Approval is already settled")?;
    let options = payload["options"].as_array().context("Approval options are missing")?;
    let selected = number.checked_sub(1).and_then(|i| options.get(i)).context("Unknown approval choice")?;
    let selected = selected["optionId"].as_str().context("Approval choice is missing")?;
    ensure!(
        current.data["options"].as_array().is_some_and(|v| v.iter().any(|o| o["optionId"] == selected)),
        "Approval options changed"
    );
    hub.send_cmd(&current.bot_id, Cmd::Permission { entry_id: entry, option_id: Some(selected.to_owned()) })?;
    Ok(json!({"ok":true,"notice":"Your choice was submitted to Codync."}))
}
