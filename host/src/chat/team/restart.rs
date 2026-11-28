//! Requests between this computer's bots that the last host process left open, settled once its bots run again, so
//! none ends without its sender hearing. One that never started is sent again: nothing of it ran. One that was
//! running fails, since its work may be half done, and its sender hears so. An outcome its sender's reply turn never
//! took (`relaying`) goes to that turn again. Requests to or from another computer, or the cloud assistant, end as
//! `Store::expire_pending` leaves them.

use super::{
    Back, BotReply, MessageLifetime, REQUEST_QUOTE_BYTES, RequestStatus, finish_entry, relayed, relaying_key,
    request_prompt, visible_bot,
};
use crate::agent::bot::Cmd;
use crate::analytics::errors::host_error;
use crate::hub::Hub;
use crate::store::{Entry, INTERRUPTED};
use codync_agent::acp::truncate;
use codync_network::Access;
use std::sync::Arc;

/// Why a request the last host process left waiting can't be sent again.
const RECIPIENT_GONE: &str = "The recipient was deleted or hidden before it could answer.";

/// At host start, after the bots' actors and boxes: replies first, so a request this ends isn't relayed twice.
pub fn restore(hub: &Arc<Hub>) {
    match hub.store.unrelayed_replies() {
        Ok(notices) => notices.iter().for_each(|notice| relay(hub, notice)),
        Err(error) => host_error!(hub, "couldn't read the replies bots never took", error = format!("{error:#}")),
    }
    match hub.store.unfinished_requests() {
        Ok(notices) => requests(notices).iter().for_each(|notices| settle(hub, notices)),
        Err(error) => host_error!(hub, "couldn't read the bot requests left open", error = format!("{error:#}")),
    }
}

/// Notices by request, in the order they were saved: the sender's first, as `message_bot` saves them.
fn requests(notices: Vec<Entry>) -> Vec<Vec<Entry>> {
    let mut requests: Vec<Vec<Entry>> = Vec::new();
    for notice in notices {
        let of = |request: &&mut Vec<Entry>| request[0].data["delegationId"] == notice.data["delegationId"];
        match requests.iter_mut().find(of) {
            Some(request) => request.push(notice),
            None => requests.push(vec![notice]),
        }
    }
    requests
}

/// One request left open: sent again if it never started, else failed. Either way its sender hears how it ends.
fn settle(hub: &Arc<Hub>, notices: &[Entry]) {
    let status = |notice: &Entry| serde_json::from_value::<RequestStatus>(notice.data["status"].clone()).ok();
    let open = |notice: &&Entry| matches!(status(notice), Some(RequestStatus::Queued | RequestStatus::Sent));
    // It ended, but the last process stopped before saving all its notices: the rest end the same way. Its sender
    // already heard, or hears from its notice (`relay`).
    if let Some(ended) = notices.iter().find(|notice| !open(notice))
        && let Some(outcome) = status(ended)
    {
        // From the saved fields, as `complete` wrote them: the text's heading may hold the request's own lines.
        let message = &ended.data["botMessage"];
        let reply = message["reply"].as_str();
        let detail = match (outcome, reply) {
            (RequestStatus::Completed, Some(reply)) => {
                let target = message["targetBotId"].as_str().unwrap_or_default();
                format!("Reply from {}:\n{reply}", hub.bot_name(target))
            }
            _ => message["detail"].as_str().unwrap_or_default().to_owned(),
        };
        for notice in notices.iter().filter(open) {
            finish_entry(hub, &notice.id, outcome, &detail, reply, None);
        }
        return;
    }
    let data = &notices[0].data;
    if data["hopLift"] == true {
        // The refusal and its grant lived only in the old process. Never turn its saved text into work for the
        // lead or a reply turn for the blocked bot: close the notice and let the user start a fresh chain.
        for notice in notices.iter().filter(open) {
            finish_entry(hub, &notice.id, RequestStatus::Failed, INTERRUPTED, None, None);
        }
        return;
    }
    let message = &data["botMessage"];
    let (Some(id), Some(source), Some(target), Some(text)) = (
        data["delegationId"].as_str(),
        message["sourceBotId"].as_str(),
        message["targetBotId"].as_str(),
        message["text"].as_str(),
    ) else {
        return;
    };
    let hops = data["hops"].as_u64().and_then(|hops| u8::try_from(hops).ok()).unwrap_or(0);
    let recipient = visible_bot(hub, target);
    let name = recipient.as_ref().map_or_else(|_| hub.bot_name(target), |cfg| cfg.name.clone());
    hub.team.readmit(source);
    let back = Back::Local { sender: source.to_owned() };
    let mut lifetime = MessageLifetime::new(hub, id, text, back, &name, hops);
    lifetime.entries = notices.iter().map(|notice| notice.id.clone()).collect();
    lifetime.admitted = true;
    let started = notices.iter().any(|notice| notice.data["status"] == "sent");
    match recipient {
        Ok(_) if started => lifetime.end(RequestStatus::Failed, INTERRUPTED, None, Err(INTERRUPTED.to_owned())),
        Ok(cfg) => {
            let sender = hub.bot_name(source);
            let who = format!("Another Codync bot, {sender},");
            let prompt = request_prompt(&who, &sender, Access::Allow, text);
            // Not taken: the lifetime ends it, and the sender hears so.
            if let Err(error) = lifetime.deliver(&cfg.id, Access::Allow, prompt) {
                tracing::warn!(request = id, error = format!("{error:#}"), "couldn't send a bot request again");
            }
        }
        Err(_) => lifetime.end(RequestStatus::Failed, RECIPIENT_GONE, None, Err(RECIPIENT_GONE.to_owned())),
    }
}

/// An outcome the sender's reply turn never took goes to that turn again, as its notice keeps it.
fn relay(hub: &Hub, notice: &Entry) {
    let (data, message) = (&notice.data, &notice.data["botMessage"]);
    let (Some(id), Some(sender), Some(target)) =
        (data["delegationId"].as_str(), message["sourceBotId"].as_str(), message["targetBotId"].as_str())
    else {
        return;
    };
    let outcome = if data["status"] == "completed" {
        // The whole reply, not the start the notice shows (that, from a host before the reply was kept whole).
        Ok(hub
            .store
            .kv_get(&relaying_key(&notice.id))
            .unwrap_or_else(|| message["reply"].as_str().unwrap_or_default().to_owned()))
    } else {
        Err(message["detail"].as_str().unwrap_or("it ended without a reply").to_owned())
    };
    let reply = BotReply {
        request_id: id.to_owned(),
        quote: truncate(message["text"].as_str().unwrap_or_default(), REQUEST_QUOTE_BYTES),
        from_name: hub.bot_name(target),
        outcome,
        hops: data["hops"].as_u64().and_then(|hops| u8::try_from(hops).ok()).unwrap_or(0),
        access: Access::Allow,
        entry: Some(notice.id.clone()),
    };
    // A sender that was deleted has no turn to take it.
    if let Err(cmd) = hub.offer_cmd(sender, Cmd::BotReply(reply))
        && let Cmd::BotReply(reply) = *cmd
    {
        relayed(hub, &[reply], &[]);
    }
}
