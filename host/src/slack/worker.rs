use super::{Arc, Hub, LockExt, Result, SLOT, Value, ingress, json, load};
use crate::{
    agent::bot::Cmd,
    store::{EntryKind, Lane, slack::SlackIncoming},
};
use std::time::Duration;

pub(crate) async fn run(hub: Arc<Hub>) {
    loop {
        if let Err(error) = tick(&hub).await {
            tracing::warn!(%error,"Slack channel needs attention");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
pub(crate) fn claim_prompt(hub: &Hub, entry: &str) -> Result<bool> {
    let _configuration = hub.store.connector_lock.locked();
    let Some(c) = load(&hub.store)? else { return Ok(false) };
    let Some(e) = hub.store.entry_checked(entry)? else { return Ok(false) };
    if e.data["slackGeneration"] != c.generation
        || !c.bindings.iter().any(|b| {
            b.bot_id == e.bot_id
                && e.data["slackChannel"] == b.channel_id
                && e.data["slackUser"].as_str().is_some_and(|user| b.allowed_users.iter().any(|u| u == user))
        })
    {
        return Ok(false);
    }
    hub.store.claim_slack_prompt(entry, &c.generation)
}
async fn tick(hub: &Arc<Hub>) -> Result<()> {
    if hub.store.kv_read(SLOT)?.is_none() {
        return Ok(());
    }
    crate::market::vault::unlock(hub.clone()).await?;
    let Some(c) = load(&hub.store)? else { return Ok(()) };
    for mut event in hub.store.pending_slack()? {
        if let Err(error) = deliver_audio(hub, &mut event).await {
            tracing::warn!(%error,event=%event.event,"Slack prompt was not queued");
        }
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()?;
    for reply in hub.store.pending_slack_replies(&c.generation)? {
        {
            let _configuration = hub.store.connector_lock.locked();
            if load(&hub.store)?.is_none_or(|current| current.generation != c.generation) {
                return Ok(());
            }
            if hub.store.bot(&reply.bot)?.is_none_or(|bot| bot.deleted || bot.config.is_group()) {
                continue;
            }
            if !hub.store.claim_slack_reply(&reply.entry)? {
                continue;
            }
        }
        if reply.text.chars().count() > 35_000 {
            hub.store.finish_slack_reply(&reply.entry, "failed", None)?;
            continue;
        }
        let request = client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&c.bot_token)
            .json(&json!({"channel":reply.channel,"thread_ts":reply.thread,"text":reply.text,
                "unfurl_links":false,"unfurl_media":false}))
            .send();
        let revoked = async {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if load(&hub.store)?.is_none_or(|current| current.generation != c.generation) {
                    return anyhow::Ok(());
                }
            }
        };
        let sent = tokio::select! {result=request=>Some(result),result=revoked=>{result?;None}};
        let (state, ts) = match sent {
            Some(Ok(response)) if response.status().is_success() => match response.json::<Value>().await {
                Ok(value) if value["ok"] == true && value["ts"].is_string() => {
                    ("sent", value["ts"].as_str().map(str::to_owned))
                }
                Ok(value) if value["ok"] == false => ("failed", None),
                _ => ("unknown", None),
            },
            _ => ("unknown", None),
        };
        hub.store.finish_slack_reply(&reply.entry, state, ts.as_deref())?;
    }
    Ok(())
}
async fn deliver_audio(hub: &Arc<Hub>, event: &mut SlackIncoming) -> Result<()> {
    if let Some(audio) = &event.audio
        && hub.store.entry_checked(&event.entry)?.is_none()
    {
        let Some(c) = load(&hub.store)? else {
            hub.store.reject_slack(&event.event)?;
            return Ok(());
        };
        let text = crate::channels::voice_notes::append(
            &hub.store,
            crate::channels::voice_notes::Platform::Slack,
            audio,
            Some(&c.bot_token),
            &event.text,
            || {
                let _configuration = hub.store.connector_lock.locked();
                Ok(load(&hub.store)?.is_some_and(|c| {
                    c.generation == event.generation
                        && c.bindings.iter().any(|b| {
                            b.channel_id == event.channel
                                && b.bot_id == event.bot
                                && b.allowed_users.contains(&event.user)
                        })
                }) && hub.store.bot(&event.bot)?.is_some_and(|b| !b.deleted && !b.config.is_group()))
            },
        )
        .await?;
        let Some(text) = text else {
            hub.store.reject_slack(&event.event)?;
            return Ok(());
        };
        event.text = text;
    }
    deliver(hub, event)
}

fn deliver(hub: &Hub, event: &SlackIncoming) -> Result<()> {
    let _configuration = hub.store.connector_lock.locked();
    let Some(c) = load(&hub.store)? else {
        hub.store.reject_slack(&event.event)?;
        return Ok(());
    };
    if c.generation != event.generation
        || !c
            .bindings
            .iter()
            .any(|b| b.channel_id == event.channel && b.bot_id == event.bot && b.allowed_users.contains(&event.user))
    {
        hub.store.reject_slack(&event.event)?;
        return Ok(());
    }
    let Some(bot) = hub.store.bot(&event.bot)?.filter(|b| !b.deleted && !b.config.is_group()) else {
        hub.store.reject_slack(&event.event)?;
        return Ok(());
    };
    let root = ingress::stable_id(&[&c.generation, &event.channel, &event.thread, "root"]);
    hub.add_entry_once(
        &root,
        &Lane::main(&event.bot),
        EntryKind::User,
        hub.store.max_turn(&event.bot),
        &json!({"text":format!("Slack conversation · {}",event.channel),
        "source":"slack","status":"sent","slackChannel":event.channel,"slackThread":event.thread}),
    )?;
    let lane = Lane::in_thread(&event.bot, &root);
    let (entry, _) = hub.add_entry_once(
        &event.entry,
        &lane,
        EntryKind::User,
        hub.store.max_turn(&event.bot) + 1,
        &json!({"text":event.text,"status":"queued","source":"slack",
        "slackGeneration":event.generation,"slackChannel":event.channel,"slackUser":event.user}),
    )?;
    if entry.data["status"] != "queued" {
        hub.store.reject_slack(&event.event)?;
        return Ok(());
    }
    let text = crate::chat::prompt::of_entry(hub, &bot.config, &entry.data);
    hub.send_cmd(&event.bot, Cmd::Send { lane, entry_id: entry.id, text })?;
    Ok(())
}
