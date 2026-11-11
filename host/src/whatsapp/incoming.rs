//! Incoming `WhatsApp` messages, routed through the host bot actor.

use super::{Arc, Hub, Value, WhatsApp, allowed, entry_id, inbox, json, save_media, voice_notes};

impl WhatsApp {
    pub(super) async fn on_message(&self, hub: &Arc<Hub>, event: &Value, expected: &str) {
        let text = event["text"].as_str().unwrap_or_default().trim().to_owned();
        if text.is_empty() && event["media"].is_null() {
            return;
        }
        let Some(chat) = event["chat"].as_str().filter(|c| !c.is_empty()) else { return };
        let sender = event["sender"].as_str().unwrap_or_default();
        let group = event["group"].as_bool().unwrap_or(false);
        if group && !event["mentioned"].as_bool().unwrap_or(false) && !event["quoted"].as_bool().unwrap_or(false) {
            return;
        }
        // With a list, only the owner, allowed people and allowed groups get through.
        let allowed = allowed(hub);
        let me = self.state.lock().await.me.clone().unwrap_or_default();
        let own = !me.is_empty() && (chat == me || sender == me);
        if !allowed.is_empty() && !own {
            let listed = if group { allowed.iter().any(|a| a == chat) } else { allowed.iter().any(|a| a == sender) };
            if !listed {
                return;
            }
        }
        // The orchestrator: an `@name` in the message picks that bot, anything else the chosen one.
        let roster: Vec<crate::store::BotRow> = hub
            .store
            .bots()
            .unwrap_or_default()
            .into_iter()
            .filter(|bot| {
                !bot.deleted
                    && !bot.config.hidden
                    && !bot.config.is_group()
                    && !crate::external::is_external(&bot.config)
            })
            .collect();
        let uses_default = event["_codyncUsesDefault"] == true;
        let bot =
            event["_codyncBotId"].as_str().map(str::to_owned).filter(|id| roster.iter().any(|b| &b.config.id == id));
        let Some(bot) = bot else {
            let _ = self
                .send_message(
                    chat,
                    "No hay ningún bot asignado a este WhatsApp. Elige uno en Ajustes.",
                    None,
                    None,
                    None,
                )
                .await;
            return;
        };
        let media = event["media"].as_object();
        let mut attached = Vec::new();
        let note = if let Some(m) = media {
            if let Ok(Some(path)) = save_media(m).await {
                attached.push(path.clone());
                format!("{text}\n\n[Attached: {path}]").trim().to_owned()
            } else {
                let kind = m.get("kind").and_then(Value::as_str).unwrap_or("file");
                format!("[{kind}] {}", m.get("name").and_then(Value::as_str).unwrap_or(&text))
            }
        } else {
            text.clone()
        };
        let note = if let Some(media) = media { voice_notes::append(&hub.store, media, note).await } else { note };
        // Audio work can outlive a configuration edit or unlink. Do not hand the old message
        // to a bot after its connection, allowlist, or default binding has been revoked.
        let current_allowed = crate::whatsapp::allowed(hub);
        let current_link = self.state().await;
        if current_link.state != "connected"
            || current_link.me.as_deref().unwrap_or_default() != me
            || (!current_allowed.is_empty()
                && !own
                && !current_allowed.iter().any(|allowed| allowed == if group { chat } else { sender }))
            || (uses_default && hub.store.kv_get("whatsapp.bot").as_deref() != Some(bot.as_str()))
        {
            return;
        }
        // From this fence through enqueue there are no await points. Setting/link mutations use
        // the same lock, and the actor checks this fence again before starting a model turn.
        let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
        match inbox::snapshot(hub, event, current_link.me.as_deref().unwrap_or_default()) {
            Ok(Some(current)) if current == expected => {}
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "WhatsApp authorization could not be checked");
                return;
            }
        }
        let routine_event = json!({
            "source": "whatsapp",
            "event": if group { "groupMention" } else { "message" },
            "chat": event["chat"],
            "from": event["sender"],
            "name": event["name"],
            "text": text,
            "group": event["group"],
            "media": media.cloned(),
        });
        hub.routines.receive_event(&hub.store, &routine_event, event["id"].as_str().map(str::to_owned));
        let Some(bot_row) = hub.store.bot(&bot).ok().flatten().filter(|row| !row.deleted && !row.config.hidden) else {
            return;
        };
        let root = match hub.store.whatsapp_root(chat, &bot) {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!(%error, "Could not read the WhatsApp conversation");
                return;
            }
        };
        let root = if let Some(root) = root {
            root
        } else {
            let root = entry_id(&["root", chat, &bot, expected]);
            if let Err(error) = hub.add_entry_once(
                &root,
                &crate::store::Lane::main(&bot),
                crate::store::EntryKind::User,
                hub.store.max_turn(&bot),
                &json!({"text":format!("WhatsApp conversation · {chat}"), "source":"whatsapp", "status":"sent", "whatsappChat": chat}),
            ) {
                tracing::warn!(%error, "Could not create the WhatsApp conversation");
                return;
            }
            root
        };
        if let Err(error) = hub.store.map_whatsapp_thread(chat, &bot, &root, sender) {
            tracing::warn!(%error, "Could not route the WhatsApp conversation");
            return;
        }
        let id = entry_id(&["message", chat, event["id"].as_str().unwrap_or_default()]);
        let prompt = format!(
            "WhatsApp message from {}:\n\n{note}",
            event["name"].as_str().unwrap_or_else(|| event["sender"].as_str().unwrap_or("someone"))
        );
        let lane = crate::store::Lane::in_thread(&bot, &root);
        let Ok((entry, _)) = hub.add_entry_once(
            &id,
            &lane,
            crate::store::EntryKind::User,
            hub.store.max_turn(&bot) + 1,
            &json!({"text":prompt, "status":"queued", "source":"whatsapp", "whatsappChat": chat,
                "whatsappFiles": attached, "whatsappGeneration":expected,
                "whatsappFence":{"_codyncBotId":bot,"_codyncUsesDefault":uses_default,"_codyncMe":me,
                    "chat":chat,"sender":sender,"group":group}}),
        ) else {
            return;
        };
        if entry.data["status"] == "queued" {
            let text = crate::chat::prompt::of_entry(hub, &bot_row.config, &entry.data);
            if let Err(error) = hub.send_cmd(&bot, crate::agent::bot::Cmd::Send { lane, entry_id: entry.id, text }) {
                tracing::warn!(%error, "Could not deliver a WhatsApp message to a bot");
            }
        }
    }
}
