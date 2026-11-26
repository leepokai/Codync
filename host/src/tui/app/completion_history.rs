//! Reply evidence and history lookups on hosts without completion IDs.

use serde_json::json;
use tokio::task::AbortHandle;
use uuid::Uuid;

use super::bot::CompletionReply;
use super::{App, Bot, Entry, Msg, Status};
use crate::tui::net::Client;

const PAGE_SIZE: usize = 500;

#[derive(Clone, Copy)]
pub(super) struct LegacyReply {
    pub(super) rev: i64,
    pub(super) created_at: i64,
}

pub(super) struct ObservedReply {
    pub(super) latest_rev: Option<i64>,
    identity: Option<(String, i64)>,
    pub(super) reply: Option<LegacyReply>,
}

impl ObservedReply {
    fn visible_identity(&self) -> Option<(&str, i64)> {
        self.identity.as_ref().map(|(id, created_at)| (id.as_str(), *created_at))
    }
}

impl LegacyReply {
    pub(super) fn from_entry(entry: &Entry) -> Option<Self> {
        let rev = entry.rev.filter(|&rev| rev > 0)?;
        Self::is_visible(entry).then_some(Self { rev, created_at: entry.created_at })
    }

    fn is_visible(entry: &Entry) -> bool {
        entry.seq > 0
            && entry.created_at > 0
            && entry.is_final()
            && entry.data["call"].is_null()
            && entry.data["removed"] != true
    }

    // The caller groups observations by chat and sequence and supplies the newest available row.
    pub(super) fn observed<'a>(
        latest: &'a Entry,
        versions: impl IntoIterator<Item = &'a Entry>,
        saved: Option<&ObservedReply>,
    ) -> ObservedReply {
        let (latest_rev, identity) = if let Some(newer) = saved.filter(|old| old.latest_rev > latest.rev) {
            (newer.latest_rev, newer.visible_identity())
        } else {
            (latest.rev, Self::is_visible(latest).then_some((latest.id.as_str(), latest.created_at)))
        };
        let reply = identity.and_then(|(id, created_at)| {
            let versions = std::iter::once(latest)
                .chain(versions)
                .filter(|old| old.id == id && old.created_at == created_at)
                .filter_map(Self::from_entry);
            let previous = saved.filter(|old| old.visible_identity() == identity).and_then(|old| old.reply);
            let current = latest_rev.filter(|&rev| rev > 0).map(|rev| Self { rev, created_at });
            versions.chain(previous).chain(current).min_by_key(|reply| reply.rev)
        });
        ObservedReply { latest_rev, identity: identity.map(|(id, at)| (id.to_owned(), at)), reply }
    }

    pub(super) fn completes_turn(self, started_at: i64, idle_rev: i64) -> bool {
        self.created_at > started_at && self.rev <= idle_rev
    }
}

pub(super) struct CompletionLookup {
    pub(super) started_at: i64,
    request: Uuid,
    task: Option<AbortHandle>,
}

impl CompletionLookup {
    pub(super) fn cancel(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for CompletionLookup {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl App {
    pub(super) fn can_show_legacy_completion(&self, bot: &Bot) -> bool {
        self.online
            && bot.notify
            && bot.status == Status::Idle
            && bot.works_here()
            && matches!(bot.completion_reply, CompletionReply::Legacy)
            && !self.watching_chat(&bot.id)
    }

    pub(super) fn look_up_legacy_completion(&mut self, bot: &Bot, started_at: i64, idle_rev: i64) {
        if !self.can_show_legacy_completion(bot) || bot.last_at <= started_at {
            return;
        }
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), bot.id.clone());
        let (request, epoch) = (Uuid::new_v4(), self.mirror_epoch);
        let task = tokio::spawn(async move {
            let replied = reply_in_history(&client, &id, idle_rev, started_at).await;
            let _ = tx.send(Msg::CompletionHistory { bot: id, epoch, request, replied });
        })
        .abort_handle();
        self.completion_lookups.insert(bot.id.clone(), CompletionLookup { started_at, request, task: Some(task) });
    }

    pub(super) fn completion_history_result(&mut self, id: &str, epoch: u64, request: Uuid, replied: bool) {
        if epoch != self.mirror_epoch {
            return;
        }
        let Some(lookup) = self.completion_lookups.get_mut(id).filter(|lookup| lookup.request == request) else {
            return;
        };
        if lookup.task.take().is_some()
            && replied
            && let Some(bot) = self.bots.get(id).filter(|bot| self.can_show_legacy_completion(bot))
        {
            self.toast(format!("{} is done", bot.name), false);
        }
    }
}

async fn reply_in_history(client: &Client, bot: &str, idle_rev: i64, started_at: i64) -> bool {
    let mut before = i64::MAX;
    loop {
        let Ok(page) = client.call("history", &json!({"botId": bot, "beforeSeq": before, "limit": PAGE_SIZE})).await
        else {
            return false;
        };
        let Some(entries) = page["entries"].as_array() else { return false };
        for value in entries.iter().rev() {
            let Some((id, entry)) = Entry::parse(value) else { return false };
            if id != bot || entry.seq <= 0 || entry.seq >= before {
                return false;
            }
            before = entry.seq;
            // History can already include rows saved after this Idle snapshot.
            if entry.rev.is_some_and(|rev| rev > idle_rev) {
                continue;
            }
            if entry.created_at <= started_at {
                return false;
            }
            if LegacyReply::from_entry(&entry).is_some_and(|reply| reply.completes_turn(started_at, idle_rev)) {
                return true;
            }
        }
        if entries.len() < PAGE_SIZE || before <= 1 {
            return false;
        }
    }
}
