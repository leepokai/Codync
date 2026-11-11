//! Outbound chat replies and approval cards stay bound to their original WhatsApp link.

use super::{Arc, Hub, Result, WhatsApp, approvals, delivery, json};

impl WhatsApp {
    /// Sends the bot replies of this chat back to it. A file that left the chat follows its text.
    pub(super) async fn replies(&self, hub: &Hub) -> Result<()> {
        let (binding, replies) = {
            let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
            let event = hub.store.snapshot_whatsapp_output(&json!({}))?;
            (event["whatsappBinding"].clone(), hub.store.pending_whatsapp_replies()?)
        };
        let account = binding[0].as_str().unwrap_or_default();
        for reply in replies {
            if !hub.store.claim_whatsapp_reply(&reply.entry)? {
                continue;
            }
            let mut sent = Ok(());
            if !reply.text.trim().is_empty() {
                sent = self.send_bound_text(&hub.store, account, &binding, &reply.chat, &reply.text).await;
            }
            if sent.is_ok() {
                for file in &reply.files {
                    let path = file["path"].as_str().unwrap_or_default();
                    if path.is_empty() {
                        continue;
                    }
                    sent = self.send_bound_file(&hub.store, &binding, &reply.chat, file).await;
                    if sent.is_err() {
                        break;
                    }
                }
            }
            hub.store.finish_whatsapp_reply(&reply.entry, if sent.is_ok() { "sent" } else { "failed" })?;
            if let Err(error) = sent {
                tracing::warn!(%error,entry=%reply.entry,"WhatsApp reply was not sent");
            }
        }
        self.cards(hub).await?;
        Ok(())
    }

    /// Shows a pending approval as a numbered request, once, to the person who wrote.
    async fn cards(&self, hub: &Hub) -> Result<()> {
        let (binding, cards) = {
            let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
            let event = hub.store.snapshot_whatsapp_output(&json!({}))?;
            (event["whatsappBinding"].clone(), hub.store.pending_whatsapp_cards()?)
        };
        let account = binding[0].as_str().unwrap_or_default();
        for (chat, entry) in cards {
            let Some(card) = hub.store.entry(&entry) else { continue };
            let Some(options) = card.data["options"].as_array().filter(|options| !options.is_empty()) else { continue };
            let Some(sender) = approvals::sender(hub, &card, &chat)? else {
                hub.store.exclude_whatsapp_card(&entry)?;
                continue;
            };
            let mut lines = vec![format!(
                "{}\n\nResponde con el número de una opción:",
                card.data["title"].as_str().unwrap_or("Aprobación")
            )];
            for (index, option) in options.iter().enumerate() {
                lines.push(format!("{}) {}", index + 1, option["name"].as_str().unwrap_or_default()));
            }
            // Reserve before handing anything to the bridge. A missing acknowledgement can
            // mean the card was posted, so a later tick must not post it again automatically.
            {
                let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
                if !delivery::matches_locked(&hub.store, account, &binding)? {
                    break;
                }
                hub.store.mark_whatsapp_card(&chat, &entry, &sender)?;
            }
            self.send_bound_text(&hub.store, account, &binding, &chat, &lines.join("\n")).await?;
        }
        Ok(())
    }

    /// Selects the original ACP option id by its displayed number, including Hermes clarifications.
    pub(super) async fn answer_card(&self, hub: &Arc<Hub>, chat: &str, sender: &str, text: &str) -> bool {
        let Ok(number) = text.trim().parse::<usize>() else { return false };
        let Some(id) = hub.store.whatsapp_card(chat, sender) else { return false };
        let card = hub.store.entry(&id).filter(|e| e.kind == "permission" && e.data["status"] == "pending");
        let Some(card) = card else {
            hub.store.forget_whatsapp_card(chat, &id).ok();
            let _ = self.send_message(chat, "Esa aprobación ya no está pendiente.", None, None, None).await;
            return true;
        };
        match hub.store.whatsapp_root(chat, &card.bot_id) {
            Ok(Some(root)) if card.thread_id.as_deref() == Some(root.as_str()) => {}
            _ => {
                tracing::warn!("WhatsApp approval has no current conversation binding");
                return true;
            }
        }
        let options = card.data["options"].as_array();
        let option = number.checked_sub(1).and_then(|index| options.and_then(|options| options.get(index)));
        let Some(option) = option else {
            let message = format!("Elige una opción entre 1 y {}.", options.map_or(0, Vec::len));
            let _ = self.send_message(chat, &message, None, None, None).await;
            return true;
        };
        let option_id = option["optionId"].as_str().unwrap_or_default().to_owned();
        let answered = crate::api::dispatch(
            hub,
            &crate::api::devices::Caller::Local,
            "respondPermission",
            json!({"entryId": id, "optionId": option_id}),
        )
        .await;
        hub.store.forget_whatsapp_card(chat, &id).ok();
        let line = match answered {
            Ok(_) => "Hecho.".to_owned(),
            Err(error) => format!("{error:#}").chars().take(180).collect(),
        };
        let _ = self.send_message(chat, &line, None, None, None).await;
        true
    }
}
