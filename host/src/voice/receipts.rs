//! A client-reported call receipt; it is never proof that audio was spoken or heard.
use crate::{
    hub::Hub,
    store::{EntryKind, Lane},
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub(crate) fn log(hub: &Hub, body: &Value) -> Result<Value> {
    let bot = body["botId"].as_str().context("botId is required")?;
    hub.store.bot(bot)?.filter(|b| !b.deleted && !b.config.is_group()).context("Unknown bot.")?;
    let seconds = body["seconds"].as_u64().filter(|s| *s <= 604_800).context("seconds must be between 0 and 604800")?;
    let call = match body.get("callId") {
        Some(value) => {
            uuid::Uuid::parse_str(value.as_str().context("callId must be a UUID")?).context("Invalid callId")?
        }
        None => uuid::Uuid::new_v4(),
    };
    let id = format!("voice-receipt-{bot}-{call}");
    let mut data = json!({"text":"Voice chat","callSeconds":seconds,"source":"voice_receipt",
        "voiceReceipt":{"version":1,"callId":call.to_string(),"kind":"client_reported_duration","seconds":seconds}});
    if let Some(value) = body.get("transcript") {
        if body.get("callId").is_none() {
            bail!("Transcript receipts require a stable callId.");
        }
        let transcript = super::transcript::Transcript::parse(value)?;
        data["voiceTranscriptText"] = json!(transcript.display());
        data["voiceReceipt"]["kind"] = json!("client_reported_transcript");
        data["voiceReceipt"]["transcript"] = serde_json::to_value(transcript)?;
    }
    // New clients end the host-owned call first. Enrich its existing row so its conversation,
    // generated title and the owner's decision confirmation share one receipt.
    let call_id = call.to_string();
    if let Some(entry) = hub.store.call_notice(bot, &call_id)? {
        let entry = hub.edit_entry(&entry.id, |entry| {
            if !entry.data["voiceReceipt"].is_null() {
                if entry.data["voiceReceipt"] != data["voiceReceipt"] {
                    bail!("The call receipt already exists with different content.");
                }
                return Ok(false);
            }
            entry.data["source"] = data["source"].clone();
            entry.data["voiceReceipt"] = data["voiceReceipt"].clone();
            if let Some(text) = data.get("voiceTranscriptText") {
                entry.data["voiceTranscriptText"] = text.clone();
            }
            Ok(true)
        })?;
        return Ok(json!({"entry":entry}));
    }
    let (entry, added) =
        hub.add_entry_once(&id, &Lane::main(bot), EntryKind::Notice, hub.store.max_turn(bot), &data)?;
    if !added && entry.data["voiceReceipt"] != data["voiceReceipt"] {
        bail!("The call receipt already exists with different content.");
    }
    Ok(json!({"entry":entry}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{BotConfig, Store};

    #[test]
    fn a_host_call_gets_one_idempotent_receipt_without_losing_its_title() {
        let dir = std::env::temp_dir().join(format!("codync-receipt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let cfg: BotConfig = serde_json::from_value(json!({"id":"bot","name":"Bot","backend":"claude"})).unwrap();
        store.save_bot(&cfg).unwrap();
        let hub = Hub::new(
            store,
            "test".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "token".into(),
            19223,
        );
        let call = uuid::Uuid::new_v4().to_string();
        let original = hub
            .add_entry(
                &Lane::main("bot"),
                EntryKind::Notice,
                0,
                &json!({"call":call,"callSeconds":10,"text":"A useful title","transcript":[]}),
            )
            .unwrap();
        let body = json!({"botId":"bot","callId":call,"seconds":10});
        let first = log(&hub, &body).unwrap();
        let repeated = log(&hub, &body).unwrap();
        assert_eq!(first["entry"]["id"], original.id);
        assert_eq!(repeated["entry"]["id"], original.id);
        assert_eq!(first["entry"]["data"]["text"], "A useful title");
        assert_eq!(first["entry"]["data"]["source"], "voice_receipt");
        assert_eq!(hub.store.history("bot", i64::MAX, 100).unwrap().len(), 1);
        assert!(log(&hub, &json!({"botId":"bot","callId":call,"seconds":11})).is_err());
        drop(hub);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
