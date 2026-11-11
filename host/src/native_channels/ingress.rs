use super::{approvals, config};
use crate::{
    LockExt,
    agent::bot::Cmd,
    hub::Hub,
    market::Connector,
    store::{EntryKind, Lane},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncReadExt;

pub(super) fn authorize(hub: &Hub, c: &Connector, generation: &str, body: &Value) -> Result<Value> {
    let _guard = hub.store.connector_lock.locked();
    let user = body["user"].as_str().filter(|s| !s.is_empty() && s.len() <= 512).context("Native sender is missing")?;
    let chat =
        body["chat"].as_str().filter(|s| !s.is_empty() && s.len() <= 512).context("Native conversation is missing")?;
    let valid = config::matches(&hub.store, c)?
        && config::generation(&hub.store, c)?.as_deref() == Some(generation)
        && config::bot(hub, c).is_ok();
    if !valid {
        return Ok(json!({"ok":true,"authorized":false,"pairing":false}));
    }
    let allowed = config::authorized(hub, c, generation, user)?;
    let pairing = !allowed
        && config::kind(c) != Some("homeassistant")
        && body["chatType"] == "dm"
        && !chat.starts_with("group:")
        && body["text"].as_str().is_some_and(|s| s.trim().starts_with("/pair "))
        && hub.store.native_pairing_active(&c.id, generation)?;
    Ok(json!({"ok":true,"authorized":allowed,"pairing":pairing}))
}

pub(super) fn accept(hub: &Arc<Hub>, c: &Connector, generation: &str, payload: &Value) -> Result<Value> {
    let _guard = hub.store.connector_lock.locked();
    ensure!(
        config::matches(&hub.store, c)? && config::generation(&hub.store, c)?.as_deref() == Some(generation),
        "Native connection changed"
    );
    let user =
        payload["user"].as_str().filter(|s| !s.is_empty() && s.len() <= 512).context("Native sender is missing")?;
    let chat = payload["chat"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .context("Native conversation is missing")?;
    let text = payload["text"].as_str().unwrap_or_default();
    ensure!(text.len() <= 32768, "Native message exceeds 32 KiB");
    if let Some(code) = text.trim().strip_prefix("/pair ") {
        ensure!(
            payload["chatType"] == "dm" && !chat.starts_with("group:") && config::kind(c) != Some("homeassistant"),
            "Pair from a direct conversation"
        );
        let valid = hub.store.native_consume_pairing(&c.id, generation, user, &config::digest(code.trim()))?;
        return Ok(
            json!({"ok":true,"notice":if valid {"Paired with Codync. You can now message this bot."} else {"Pairing code is invalid or expired. Ask the computer owner for a new code."}}),
        );
    }
    ensure!(config::authorized(hub, c, generation, user)?, "Sender is not allowed for this native connection");
    if text.trim().starts_with("/approve ") {
        return approvals::answer(hub, c, generation, user, chat, text.trim());
    }
    let event = payload["messageId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .context("Native message id is missing")?;
    let bot = config::bot(hub, c)?;
    ensure!(payload.to_string().len() <= 128 * 1024, "Native event metadata is too large");
    hub.store.native_enqueue(&c.id, generation, event, &bot, payload)?;
    Ok(json!({"ok":true}))
}

fn attachment(cache: &Path, value: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(value["path"].as_str().context("Attachment path is missing")?).canonicalize()?;
    let root = cache.canonicalize()?;
    ensure!(path.starts_with(root) && path.is_file(), "Native attachment is outside its media store");
    Ok(path)
}

async fn contents(
    hub: &Hub,
    cache: &Path,
    event: &Value,
    cfg: &crate::store::BotConfig,
) -> Result<(String, Vec<Value>)> {
    let mut text = event["text"].as_str().unwrap_or_default().to_owned();
    if let Some(quote) = event["quote"].as_str().filter(|s| !s.is_empty()) {
        text = format!("[Quoted message]\n{}\n\n{text}", crate::agent::acp::truncate(quote, 4000));
    }
    let mut files = Vec::new();
    let mut bytes_total = 0_u64;
    for media in event["media"].as_array().into_iter().flatten().take(10) {
        let path = attachment(cache, media)?;
        let size = tokio::fs::metadata(&path).await?.len();
        ensure!((1..=20 * 1024 * 1024).contains(&size), "Native attachment exceeds 20 MiB");
        bytes_total = bytes_total.saturating_add(size);
        ensure!(bytes_total <= 20 * 1024 * 1024, "Native attachments exceed 20 MiB total");
        let name = media["name"]
            .as_str()
            .unwrap_or("attachment")
            .chars()
            .filter(|c| !c.is_control())
            .take(255)
            .collect::<String>();
        let mime = media["mime"].as_str().unwrap_or("application/octet-stream");
        if mime.starts_with("audio/") {
            let file = tokio::fs::File::open(&path).await?;
            let mut bytes = Vec::new();
            file.take(crate::voice::transcription::MAX_NOTE_BYTES as u64 + 1).read_to_end(&mut bytes).await?;
            let speech =
                tokio::time::timeout(Duration::from_secs(90), crate::voice::transcription::note(&hub.store, &bytes))
                    .await;
            match speech {
                Ok(Ok(transcript))=>text.push_str(&format!("\n\n[Voice note transcript]\n{transcript}")),
                _=>text.push_str("\n\n[Voice note could not be transcribed. Keep the original attachment and ask the sender to clarify.]"),
            }
        }
        let name = if name.starts_with('.') || name.len() > 200 || name.contains(['/', '\\', ':']) {
            format!("attachment-{}", files.len() + 1)
        } else {
            name
        };
        let file = tokio::fs::File::open(&path).await?;
        let mut bytes = Vec::new();
        file.take(20 * 1024 * 1024 + 1).read_to_end(&mut bytes).await?;
        ensure!(bytes.len() as u64 == size, "Native attachment changed while copying");
        let upload_root = crate::chat::uploads::root(cfg);
        let upload = uuid::Uuid::new_v4().to_string();
        let meta = tokio::task::spawn_blocking(move || {
            crate::chat::uploads::append(&upload_root, &upload, &name, 0, &bytes, true)
        })
        .await??
        .context("Native upload was not completed")?;
        files.push(meta);
    }
    if !files.is_empty() {
        let names = files.iter().filter_map(|f| f["name"].as_str()).collect::<Vec<_>>().join(", ");
        text.push_str(&format!("\n\n[Attached files saved on this computer: {names}]"));
    }
    ensure!(!text.trim().is_empty() || !files.is_empty(), "Empty native message");
    Ok((text, files))
}

pub(super) async fn deliver(hub: &Arc<Hub>, c: &Connector, generation: &str, cache: &Path) -> Result<()> {
    for (expected, event, bot, payload) in hub.store.native_pending(&c.id)? {
        let valid = expected == generation
            && config::matches(&hub.store, c)?
            && config::bot(hub, c).ok().as_deref() == Some(&bot)
            && config::authorized(hub, c, generation, payload["user"].as_str().unwrap_or_default())?;
        if !valid {
            hub.store.native_reject(&c.id, &expected, &event)?;
            continue;
        }
        let entry_id = format!("native-input-{}", config::digest(&format!("{}\0{generation}\0{bot}\0{event}", c.id)));
        let prepared = if hub.store.entry_checked(&entry_id)?.is_none() {
            let revoked = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if !config::matches(&hub.store, c)?
                        || !config::authorized(hub, c, generation, payload["user"].as_str().unwrap_or_default())?
                    {
                        return anyhow::Ok(());
                    }
                }
            };
            let cfg = hub.store.bot(&bot)?.context("Native bot is missing")?.config;
            let content = tokio::select! {
                result=contents(hub,cache,&payload,&cfg)=>Some(result),
                result=revoked=>{result?;None}
            };
            let Some(content) = content else {
                hub.store.native_reject(&c.id, &expected, &event)?;
                continue;
            };
            Some(content.unwrap_or_else(|_| {
                (
                    format!(
                        "{}\n\n[Attachments could not be read or transcribed. Ask the sender to clarify.]",
                        payload["text"].as_str().unwrap_or_default()
                    ),
                    vec![],
                )
            }))
        } else {
            None
        };
        let _guard = hub.store.connector_lock.locked();
        if !config::matches(&hub.store, c)?
            || !config::authorized(hub, c, generation, payload["user"].as_str().unwrap_or_default())?
            || config::bot(hub, c).ok().as_deref() != Some(&bot)
        {
            hub.store.native_reject(&c.id, &expected, &event)?;
            continue;
        }
        let chat = payload["chat"].as_str().context("Native conversation is missing")?;
        let user = payload["user"].as_str().context("Native sender is missing")?;
        let root = format!("native-channel-{}", config::digest(&format!("{}\0{generation}\0{bot}\0{chat}", c.id)));
        hub.add_entry_once(&root,&Lane::main(&bot),EntryKind::User,hub.store.max_turn(&bot),&json!({"text":format!("{} conversation · {chat}",config::kind(c).unwrap_or("Native")),"source":"native-channel","status":"sent"}))?;
        hub.store.native_route(&root, &c.id, generation, &bot, chat, user, &event)?;
        let lane = Lane::in_thread(&bot, &root);
        let entry = match prepared {
            Some((text, files)) => {
                hub.add_entry_once(
                    &entry_id,
                    &lane,
                    EntryKind::User,
                    hub.store.max_turn(&bot) + 1,
                    &json!({
                        "text":text,"attachments":files,"source":"native-channel","status":"queued",
                        "nativeFence":{"connection":c.id,"generation":generation,"event":event,"user":user,"chat":chat}
                    }),
                )?
                .0
            }
            None => hub.store.entry_checked(&entry_id)?.context("Native queued input disappeared")?,
        };
        if entry.data["status"] != "queued" {
            hub.store.native_reject(&c.id, &expected, &event)?;
            continue;
        }
        let cfg = hub.store.bot(&bot)?.context("Bound bot is missing")?.config;
        let text = crate::chat::prompt::of_entry(hub, &cfg, &entry.data);
        hub.send_cmd(&bot, Cmd::Send { lane, entry_id: entry.id, text })?;
    }
    Ok(())
}
