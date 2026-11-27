//! Host messages: the event stream and command replies.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};

use crate::api::CATCH_UP_PER_BOT;

use super::bot::CompletionReply;
use super::completion_history::{LegacyReply, ObservedReply};
use super::{
    After, App, Bot, Dir, Entry, Focus, Kind, Msg, Overlay, ReplayChat, SendReceiptCheck, Status, merge_toggles,
};

const UNCONFIRMED_SEND: &str = "Couldn't confirm the send. Draft kept. Enter retries.";

fn chat_turn_started_at(bot: &Bot) -> Option<i64> {
    bot.started_at
        .filter(|&at| at > 0 && matches!(bot.status, Status::Working | Status::NeedsInput) && bot.works_here())
}

fn keep_legacy_reply(bot: &Bot, reply: &LegacyReply) -> bool {
    // Catch-up can already hold the next reply when a buffered Idle arrives.
    matches!(bot.completion_reply, CompletionReply::Legacy)
        && chat_turn_started_at(bot)
            .map_or_else(|| bot.rev.is_none_or(|rev| reply.rev > rev), |started_at| reply.created_at > started_at)
}

fn keep_legacy_observation(bot: Option<&Bot>, observed: &mut ObservedReply, outside: bool) -> bool {
    observed.reply = observed.reply.filter(|reply| {
        (outside || Some(reply.rev) != observed.latest_rev) && bot.is_none_or(|bot| keep_legacy_reply(bot, reply))
    });
    // Below the window, keep the revision even if it invalidates a final that may arrive later.
    observed.reply.is_some()
        || outside
            && observed.latest_rev.is_some_and(|rev| {
                rev > 0
                    && bot.is_none_or(|bot| {
                        matches!(bot.completion_reply, CompletionReply::Legacy)
                            && (chat_turn_started_at(bot).is_some() || bot.rev.is_none_or(|seen| rev > seen))
                    })
            })
}

/// An entry the mirror doesn't hold, below the floor its bot's loaded chat had when this
/// connection began: `seq` grows with insertion, so it predates the window and only arrived because
/// its rev was bumped (a rewrite, a reaction). History paging brings it.
fn outside_window(floor: Option<i64>, held: bool, e: &Entry) -> bool {
    !held && floor.is_some_and(|f| e.seq < f)
}

impl App {
    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Online => {
                self.online = true;
                self.error = None;
                // Every reconnect can reach a newer host or another one.
                self.call("hello", json!({}), After::Hello);
            }
            Msg::Disconnected { since } => {
                self.online = false;
                self.reset_send_checks(since);
                self.replay_chats = None;
                if let Some(receipts) = self.replay_sends.as_mut() {
                    receipts.clear();
                    self.replayed_deletions.clear();
                }
                // The next connection may reach another host: nothing goes by this one's version.
                self.host_version.clear();
                for lookup in self.completion_lookups.values_mut() {
                    lookup.cancel();
                }
                if self.error.is_none() && self.bots.is_empty() {
                    // Say why instead of spinning on "Connecting…".
                    self.call("hello", json!({}), After::Hello);
                }
            }
            Msg::Rewind(send_receipts) => {
                self.bots.clear();
                self.entries.clear();
                self.reading.clear();
                self.pending_history.clear();
                self.history_done.clear();
                self.mirror_epoch += 1;
                self.rewind_threads();
                self.send_receipts = send_receipts;
                self.send_receipt_ready = false;
                self.floors.clear();
                self.legacy_replies.clear();
                self.completion_lookups.clear();
                self.reply = None;
                self.pick = None;
                self.find = None;
                self.replayed_deletions.clear();
                self.replay_sends = Some(HashSet::new());
                self.replay_chats = None;
                for send in self.sends.values_mut() {
                    send.receipt = SendReceiptCheck::Unverified;
                    if let Some(task) = send.task.take() {
                        task.abort();
                    }
                }
                for overlay in &mut self.overlays {
                    match overlay {
                        Overlay::Form(f) => f.saving = false,
                        Overlay::NewBot(f) => f.saving = false,
                        Overlay::Group(f) => f.saving = false,
                        _ => {}
                    }
                }
            }
            Msg::ReplayStarted => {
                self.replay_sends = Some(HashSet::new());
                self.replay_chats = Some(HashMap::new());
            }
            Msg::ReplayComplete => {
                self.finish_window_refresh();
                if let Some(receipts) = self.replay_sends.take() {
                    let deleted: Vec<_> = self
                        .replayed_deletions
                        .iter()
                        .filter(|(id, rev)| self.bots.get(*id).and_then(|bot| bot.rev).is_none_or(|seen| seen <= **rev))
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in deleted {
                        self.remove_bot(&id);
                        self.close_idle_work_page();
                    }
                    for (bot, nonce) in receipts {
                        self.acknowledge_send(&bot, &nonce);
                    }
                }
                self.start_send_checks();
                if !self.host_version.is_empty() {
                    self.reload_thread_details();
                }
            }
            Msg::Connected { since, connection, send_receipts } => {
                self.reset_send_checks(since);
                self.stream_since = since;
                self.send_receipt_connection = connection;
                self.send_receipts = send_receipts;
                self.floors.clear();
                self.reload_workflows();
            }
            // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
            Msg::Event(mut v) => {
                crate::tui::net::old_host::incoming(&mut v);
                self.on_event(&v);
            }
            Msg::Reply(after, mut r) => {
                if let Ok(v) = &mut r {
                    crate::tui::net::old_host::incoming(v);
                }
                self.on_reply(after, r);
            }
            Msg::CompletionHistory { bot, epoch, request, replied } => {
                self.completion_history_result(&bot, epoch, request, replied);
            }
            Msg::FileDownload { id, status, done } => {
                if self.download.as_ref().is_some_and(|job| job.id == id) {
                    self.flash(&status.unwrap_or_else(|error| format!("Download failed: {error}")));
                    if done {
                        self.download = None;
                    }
                }
            }
            Msg::Term(id, v) => self.on_term(&id, &v),
        }
    }

    fn on_event(&mut self, v: &Value) {
        let catch_up_left = self.catch_up_left;
        if let Some(left) = &mut self.catch_up_left {
            *left = left.saturating_sub(1);
        }
        match v["type"].as_str() {
            Some("hello") => {
                self.replay_rev = v["rev"].as_i64().unwrap_or_default();
                self.catch_up_left = v["catchUp"].as_u64();
                self.send_receipt_ready = true;
                self.send_receipts.received_hello(self.send_receipt_connection);
                self.replayed_deletions.clear();
                self.completions_before_live.clear();
                if self.catch_up_left.is_some() {
                    self.completions_before_live.extend(
                        self.bots.values().filter_map(|bot| bot.completion_reply.exact_id().map(str::to_owned)),
                    );
                }
                self.snapshot_floors();
                self.network = v["network"].clone();
                self.workflows = v["workflows"].as_bool().unwrap_or(false);
                if !v["usage"].is_null() {
                    self.usage = v["usage"].clone();
                }
                self.start_send_checks();
                if !self.host_version.is_empty() {
                    self.reload_thread_details();
                }
            }
            // Not a rev event. An open workflow page loads again; every other screen ignores it.
            Some("workflow") => self.on_workflow_event(v),
            Some("usage") => self.usage = v["usage"].clone(),
            // Not a rev event: a model list the computer read again.
            Some("models") => {
                if let Some(backend) = v["backend"].as_str() {
                    self.keep_models(Self::model_key(backend, v["loginProfile"].as_str()), &v["models"]);
                }
            }
            Some("screen") => self.screen = v["screen"].clone(),
            Some("network") => self.network_changed(&v["network"]),
            Some("cloud") => self.cloud = v["cloud"].clone(),
            Some("name") => {
                if let Some(name) = v["name"].as_str() {
                    name.clone_into(&mut self.host);
                }
            }
            Some("autoOrder") => self.auto_order = v["enabled"].as_bool(),
            Some("threadKeepDays") => {
                self.thread_keep_days = super::thread_keep::days(&v["days"]);
                self.relist_older();
            }
            Some("pairingRequests") => self.pairing_requests_changed(v),
            Some("bot") => self.on_bot_event(v, catch_up_left),
            Some("artifact") => self.artifact_changed(&v["artifact"]),
            Some("threadEntry") => self.thread_changed(v["thread"].as_str(), &v["entry"]),
            Some("entry") => self.on_entry_event(&v["entry"]),
            _ => {}
        }
    }

    fn on_bot_event(&mut self, event: &Value, catch_up_left: Option<u64>) {
        let bot = &event["bot"];
        let rev = event["rev"].as_i64();
        let live = catch_up_left.map_or_else(|| rev.is_none_or(|rev| rev > self.replay_rev), |left| left == 0);
        let replaying = catch_up_left.is_some_and(|left| left > 0);
        let recovering = self.replay_sends.is_some();
        let deleted = bot["deleted"] == true;
        let refreshing = !live && !deleted && self.replay_chats.is_some();
        if replaying && let Some(reply) = bot["completionReplyId"].as_str() {
            self.completions_before_live.insert(reply.to_owned());
        }
        if let Some(id) = bot["id"].as_str() {
            let bot_rev = rev.or_else(|| bot["rev"].as_i64());
            let deletion_rev = self.replayed_deletions.get(id).copied();
            let held_rev = self.bots.get(id).and_then(|bot| bot.rev);
            let previous_rev = held_rev.max(deletion_rev);
            if (catch_up_left == Some(0) || recovering || deletion_rev.is_some())
                && bot_rev.zip(previous_rev).is_some_and(|(incoming, previous)| {
                    incoming < previous || (incoming == previous && (!refreshing || deletion_rev == Some(incoming)))
                })
            {
                return;
            }
            if (replaying || recovering) && deleted {
                self.replayed_deletions.insert(id.to_owned(), bot_rev.unwrap_or_default());
                if recovering {
                    return;
                }
            }
            if refreshing && let Some(chats) = self.replay_chats.as_mut() {
                let chat = chats.entry(id.to_owned()).or_default();
                if let Some(rev) = bot_rev.filter(|&rev| held_rev == Some(rev)) {
                    chat.snapshot = Some((rev, bot.clone()));
                    return;
                }
            }
        }
        self.apply_bot_event(bot, live);
    }

    fn apply_bot_event(&mut self, bot: &Value, live: bool) {
        self.upsert_bot(bot, live);
        self.close_idle_work_page();
        self.memory_moved(bot);
    }

    /// Keep each loaded chat's floor across catch-up and buffered live updates.
    fn snapshot_floors(&mut self) {
        self.floors = if self.stream_since > 0 || self.replay_chats.is_some() {
            self.entries
                .iter()
                .filter_map(|(bot, loaded)| {
                    let oldest = loaded.values().filter(|e| e.seq > 0).map(|e| e.seq).min();
                    Some((bot.clone(), oldest?))
                })
                .collect()
        } else {
            HashMap::new()
        };
    }

    fn on_entry_event(&mut self, row: &Value) {
        let Some((bot, entry)) = Entry::parse(row) else { return };
        if let Some(chats) = self.replay_chats.as_mut() {
            let window = &mut chats.entry(bot).or_default().entries;
            if window.get(&entry.seq).is_none_or(|held| held.rev <= entry.rev) {
                window.insert(entry.seq, entry);
            }
        } else {
            self.apply_stream_entry(bot, entry);
        }
    }

    fn finish_window_refresh(&mut self) {
        let Some(chats) = self.replay_chats.take() else { return };
        for (id, ReplayChat { entries, snapshot, verified_sends }) in chats {
            if let Some((rev, bot)) = snapshot
                && self.bots.get(&id).is_some_and(|held| held.rev == Some(rev))
                && self.replayed_deletions.get(&id).is_none_or(|&deleted| deleted < rev)
            {
                self.apply_bot_event(&bot, false);
            }
            self.replace_chat_window(&id, entries, &verified_sends);
        }
    }

    fn replace_chat_window(&mut self, id: &str, mut window: BTreeMap<i64, Entry>, verified_sends: &HashSet<i64>) {
        let older = if self.catch_up_left.is_none() {
            // Older hosts can buffer rewrites below the snapshot's tail before reading hello.rev.
            let first = window.range(1..).rev().take(usize::from(CATCH_UP_PER_BOT)).last().map(|(&seq, _)| seq);
            let tail = first.map_or_else(BTreeMap::new, |seq| window.split_off(&seq));
            std::mem::replace(&mut window, tail)
        } else {
            BTreeMap::new()
        };
        self.pending_history.remove(id);
        self.history_done.remove(id);
        if let Some((&first, _)) = window.first_key_value() {
            self.floors.insert(id.to_owned(), first);
        } else {
            self.floors.remove(id);
        }
        let last = window.last_key_value().map(|(&seq, _)| seq);
        let after_tail = |seq: &i64| last.is_some_and(|last| *seq > last);
        if let Some(replies) = self.legacy_replies.get_mut(id) {
            let held = self.entries.get(id);
            replies.retain(|seq, _| {
                window.contains_key(seq)
                    || older.contains_key(seq)
                    || (after_tail(seq) && held.is_some_and(|rows| rows.contains_key(seq)))
            });
        }
        for entry in older.into_values().filter(|entry| entry.seq > 0) {
            self.observe_send_receipt(id, &entry);
            self.on_outside_entry(id, entry);
        }
        let keep_entry =
            |seq: &i64| window.contains_key(seq) || after_tail(seq) || (last.is_none() && verified_sends.contains(seq));
        if let Some(held) = self.entries.get_mut(id) {
            // A send reply can already have added a row beyond the snapshot.
            held.retain(|seq, _| keep_entry(seq));
        }
        self.send_receipts.retain(id, keep_entry);
        for entry in window.into_values() {
            self.apply_stream_entry(id.to_owned(), entry);
        }
    }

    fn apply_stream_entry(&mut self, bot: String, entry: Entry) {
        self.observe_send_receipt(&bot, &entry);
        if self.outside_window(&bot, &entry) {
            self.on_outside_entry(&bot, entry);
            return;
        }
        if let Some(rows) = self.entries.get_mut(&bot)
            && self.send_receipts.observe(&bot, &entry, rows.get(&entry.seq))
        {
            rows.remove(&entry.seq);
        }
        if !self.entries.get(&bot).is_some_and(|m| m.contains_key(&entry.seq)) {
            self.entry_added(&bot, &entry);
            // A routine notice can change what an open work page shows.
            if entry.kind == Kind::Notice {
                self.reload_details(&bot);
            }
        }
        self.merge_stream_entry(bot, entry);
    }

    fn observe_send_receipt(&mut self, bot: &str, entry: &Entry) {
        if entry.kind == Kind::User
            && entry.seq > 0
            && !entry.id.is_empty()
            && entry.data["removed"] != true
            && let Some(nonce) = entry.data["clientNonce"].as_str()
        {
            if let Some(receipts) = self.replay_sends.as_mut() {
                if self.sends.get(bot).is_some_and(|send| send.nonce == nonce) {
                    receipts.insert((bot.to_owned(), nonce.to_owned()));
                }
            } else {
                self.acknowledge_send(bot, nonce);
            }
        }
    }

    fn outside_window(&self, bot: &str, e: &Entry) -> bool {
        let held = self.entries.get(bot).is_some_and(|m| m.contains_key(&e.seq));
        outside_window(self.floors.get(bot).copied(), held, e)
    }

    fn on_outside_entry(&mut self, bot: &str, entry: Entry) {
        let buffered = self.pending_history.get(bot).and_then(|page| page.updates.get(&entry.seq));
        let held = self.entries.get(bot).and_then(|rows| rows.get(&entry.seq));
        let newer = buffered.into_iter().chain(held).max_by_key(|row| row.rev).filter(|row| row.rev > entry.rev);
        let latest = newer.unwrap_or(&entry);
        let replace = newer.is_none();
        let observed = self.observe_legacy_reply(bot, latest, std::iter::once(&entry).chain(buffered).chain(held));
        self.remember_legacy_reply(bot, entry.seq, observed, true);
        if replace && let Some(page) = self.pending_history.get_mut(bot).filter(|page| entry.seq < page.before_seq) {
            page.updates.insert(entry.seq, entry);
        }
    }

    fn observe_legacy_reply<'a>(
        &self,
        id: &str,
        latest: &'a Entry,
        versions: impl IntoIterator<Item = &'a Entry>,
    ) -> ObservedReply {
        let saved = self.legacy_replies.get(id).and_then(|replies| replies.get(&latest.seq));
        LegacyReply::observed(latest, versions, saved)
    }

    fn remember_legacy_reply(&mut self, id: &str, seq: i64, mut observed: ObservedReply, outside: bool) {
        if keep_legacy_observation(self.bots.get(id), &mut observed, outside) {
            self.legacy_replies.entry(id.to_owned()).or_default().insert(seq, observed);
        } else if let Some(replies) = self.legacy_replies.get_mut(id) {
            replies.remove(&seq);
            if replies.is_empty() {
                self.legacy_replies.remove(id);
            }
        }
    }

    fn merge_stream_entry(&mut self, bot: String, entry: Entry) {
        let held = self.entries.get(&bot).and_then(|entries| entries.get(&entry.seq));
        let newer = held.filter(|previous| previous.rev > entry.rev);
        let latest = newer.unwrap_or(&entry);
        let replace = newer.is_none();
        let observed = self.observe_legacy_reply(&bot, latest, std::iter::once(&entry).chain(held));
        self.remember_legacy_reply(&bot, entry.seq, observed, false);
        if replace {
            self.entries.entry(bot).or_default().insert(entry.seq, entry);
        }
    }

    fn insert_missing_entry(&mut self, bot: String, entry: Entry) {
        if !self.entries.get(&bot).is_some_and(|entries| entries.contains_key(&entry.seq)) {
            self.merge_stream_entry(bot, entry);
        }
    }

    fn finish_history(&mut self, id: &str, request: uuid::Uuid, result: Result<Value, String>) {
        if self.pending_history.get(id).is_none_or(|page| page.request != request) {
            return;
        }
        let Some(mut page) = self.pending_history.remove(id) else { return };
        let Ok(value) = result else { return };
        let entries = value["entries"].as_array().map(Vec::as_slice).unwrap_or_default();
        if entries.is_empty() {
            self.history_done.insert(id.to_owned());
        }
        for value in entries {
            let Some((bot, row)) = Entry::parse(value) else { continue };
            if bot != id || row.seq <= 0 || row.seq >= page.before_seq {
                continue;
            }
            let (candidate, other) = match page.updates.remove(&row.seq) {
                Some(buffered) if buffered.rev >= row.rev => (buffered, Some(row)),
                buffered => (row, buffered),
            };
            let held = self.entries.get(id).and_then(|entries| entries.get(&candidate.seq));
            let newer = held.filter(|entry| entry.rev >= candidate.rev);
            let latest = newer.unwrap_or(&candidate);
            let replace = newer.is_none();
            let versions = std::iter::once(&candidate).chain(other.as_ref()).chain(held);
            let observed = self.observe_legacy_reply(id, latest, versions);
            self.remember_legacy_reply(id, candidate.seq, observed, false);
            if replace {
                self.entries.entry(bot).or_default().insert(candidate.seq, candidate);
            }
        }
    }

    fn legacy_turn_replied(&mut self, old: &Bot, idle: &Bot) -> bool {
        let (Some(started_at), Some(idle_rev)) = (chat_turn_started_at(old), idle.rev.filter(|&rev| rev > 0)) else {
            return false;
        };
        if self.completion_lookups.get(&idle.id).is_some_and(|lookup| lookup.started_at >= started_at) {
            return false;
        }
        let completes = |reply: LegacyReply| reply.completes_turn(started_at, idle_rev);
        let replied = self
            .legacy_replies
            .get(&idle.id)
            .is_some_and(|replies| replies.values().filter_map(|observed| observed.reply).any(completes))
            || self
                .entries
                .get(&idle.id)
                .is_some_and(|entries| entries.values().rev().filter_map(LegacyReply::from_entry).any(completes));
        if !replied {
            self.look_up_legacy_completion(idle, started_at, idle_rev);
        }
        replied
    }

    fn upsert_bot(&mut self, v: &Value, live: bool) {
        let Some(id) = v["id"].as_str() else { return };
        if v["deleted"].as_bool().unwrap_or(false) {
            self.remove_bot(id);
            return;
        }
        let Some(new) = Bot::parse(v) else { return };
        if !self.can_show_legacy_completion(&new)
            && let Some(lookup) = self.completion_lookups.get_mut(id)
        {
            lookup.cancel();
        }
        let previous = self.bots.get(id);
        // Marked read once per change.
        if previous.is_none_or(|old| old.unread != new.unread) {
            self.reading.remove(&new.id);
        }
        if let Some(old) = self.bots.insert(new.id.clone(), new.clone()) {
            self.threads_ended(&old.work, &new.work);
            self.keep_details_cursor(&new.id, (&old.work, old.max_threads));
            self.notify_bot_change(&old, &new, live);
        }
        self.follow_threads(&new.id);
        if let Some(replies) = self.legacy_replies.get_mut(id) {
            let entries = self.entries.get(id);
            replies.retain(|seq, observed| {
                let outside = entries.is_none_or(|entries| !entries.contains_key(seq));
                keep_legacy_observation(Some(&new), observed, outside)
            });
            if replies.is_empty() {
                self.legacy_replies.remove(id);
            }
        }
    }

    fn notify_bot_change(&mut self, old: &Bot, new: &Bot, live: bool) {
        // A member's turn in a group is the group's: it alone says done / needs you.
        if !new.notify || self.watching_chat(&new.id) {
            return;
        }
        let away = old.away().is_some() || new.away().is_some();
        if live && new.status == Status::Idle {
            let replied = match &new.completion_reply {
                CompletionReply::Exact(reply) => reply.as_deref().is_some_and(|reply| {
                    old.completion_reply.exact_id() != Some(reply) && !self.completions_before_live.contains(reply)
                }),
                CompletionReply::Legacy => !away && old.status == Status::Working && self.legacy_turn_replied(old, new),
            };
            if replied {
                self.toast(format!("{} is done", new.name), false);
            }
        }
        if away {
            return;
        }
        if new.status == Status::NeedsInput && old.status != Status::NeedsInput {
            self.toast(format!("{} needs you", new.name), true);
        } else if old.status != Status::Error && new.status == Status::Error {
            self.toast(format!("{} hit an error", new.name), true);
        }
    }

    fn remove_bot(&mut self, id: &str) {
        if let Some(gone) = self.bots.remove(id) {
            self.threads_ended(&gone.work, &[]);
        }
        self.entries.remove(id);
        self.send_receipts.retain(id, |_| false);
        self.pending_history.remove(id);
        self.history_done.remove(id);
        self.floors.remove(id);
        self.legacy_replies.remove(id);
        self.completion_lookups.remove(id);
        self.drafts.remove(id);
        self.files.remove(id);
        self.pasted.remove(id);
        if let Some(task) = self.sends.remove(id).and_then(|send| send.task) {
            task.abort();
        }
        if self.selected.as_deref() == Some(id) {
            self.close_find();
            self.selected = None;
            self.reply = None;
            self.pick = None;
            self.chat_page = false;
            self.typing = false;
        }
    }

    fn on_reply(&mut self, after: After, r: Result<Value, String>) {
        if let After::Connection(id, step) = after {
            self.connection_reply(&id, step, r);
            return;
        }
        if let After::Sent { key, nonce, epoch } = after {
            if epoch == self.mirror_epoch {
                self.sent(&key, &nonce, r);
            }
            return;
        }
        if let After::CheckSendReceipt { key, request, epoch, connection } = after {
            if epoch == self.mirror_epoch {
                self.finish_send_check(&key, request, connection, r);
            }
            return;
        }
        if let After::Answered(entry) = after {
            self.answering.remove(&entry);
            if let Err(e) = r {
                self.flash(&e);
            }
            return;
        }
        if let After::Sheet(s) = after {
            self.on_sheet_reply(s, r);
            return;
        }
        if let After::Network(n) = after {
            self.network_reply(n, r);
            return;
        }
        if let After::UsageOrder(to) = after {
            self.usage_order_answered(to, r);
            return;
        }
        if matches!(after, After::Pairing | After::Cloud | After::PairingRequests | After::PairingDecided) {
            self.pair_reply(&after, r);
            return;
        }
        if let After::History(id, request) = after {
            self.finish_history(&id, request, r);
            return;
        }
        if let After::Saved(epoch) = after
            && epoch != self.mirror_epoch
        {
            return;
        }
        if let After::Queued(epoch) = after
            && !self.accepts_mirror_reply(epoch)
        {
            return;
        }
        let v = match r {
            Ok(v) => v,
            Err(e) => {
                match after {
                    After::Hello => self.error = Some(e),
                    After::Tracked => {}
                    After::Saved(_) => match self.overlays.last_mut() {
                        Some(Overlay::Form(f)) => {
                            f.saving = false;
                            f.error = Some(e);
                        }
                        Some(Overlay::NewBot(n)) => {
                            n.saving = false;
                            n.error = Some(e);
                        }
                        Some(Overlay::Group(g)) => {
                            g.saving = false;
                            g.error = Some(e);
                        }
                        _ => {}
                    },
                    After::Dirs(id) | After::WorkspaceSelected(id) => {
                        if let Some(Overlay::Folder(p)) = self.overlays.last_mut()
                            && p.request_id == id
                        {
                            p.error = Some(e);
                            p.busy = false;
                        }
                    }
                    _ => self.flash(&e),
                }
                return;
            }
        };
        match after {
            After::Nothing
            | After::Queued(_)
            | After::Tracked
            | After::Answered(_)
            | After::Sheet(_)
            | After::Network(_)
            | After::UsageOrder(_)
            | After::Sent { .. }
            | After::CheckSendReceipt { .. }
            | After::History(_, _)
            | After::Connection(_, _)
            | After::Pairing
            | After::Cloud
            | After::PairingRequests
            | After::PairingDecided => {}
            After::Installed => {
                self.error = None;
                self.flash("Host installed; connecting…");
                self.call("hello", json!({}), After::Hello);
            }
            After::Hello => {
                self.custom_icons = v["icons"].is_object();
                self.mismatch = crate::compat::check(env!("CARGO_PKG_VERSION"), crate::compat::MIN_HOST, &v);
                v["name"].as_str().unwrap_or("computer").clone_into(&mut self.host);
                v["version"].as_str().unwrap_or_default().clone_into(&mut self.host_version);
                v["home"].as_str().unwrap_or_default().clone_into(&mut self.home);
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
                self.analytics_hello(&v);
                self.auto_order = v["autoOrder"].as_bool();
                self.thread_keep_days = super::thread_keep::days(&v["threadKeepDays"]);
                self.workflows = v["workflows"].as_bool().unwrap_or(false);
                self.credentials_reset = v["credentialsReset"].as_bool().unwrap_or(false);
                self.routine_folder = v["routineFolder"].as_bool().unwrap_or(false);
                self.projects = v["projects"].as_bool().unwrap_or(false);
                self.worktree_fill = v["worktreeFill"] == true;
                self.screen = v["screen"].clone();
                // A thread's entries aren't in a connection's catch-up: the open transcripts ask for what they
                // missed, now that the host's version says which name `threadEntries` goes by.
                self.reload_thread_details();
            }
            After::Analytics => self.analytics_set(&v),
            After::AutoOrder => self.auto_order_set(&v),
            After::ThreadKeep => self.thread_keep_set(&v),
            After::CredentialsReset => {
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
                self.flash("Saved credentials reset. Enter them again where you use them.");
            }
            After::Backends => {
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
                self.flash("Agents refreshed");
            }
            After::Update => match v["state"]["availableVersion"].as_str() {
                Some(version) => self.flash(&format!("Version {version} is available: run codync-host update")),
                None => self.flash("The host is up to date."),
            },
            After::WorkspaceSelected(id) => {
                if matches!(self.overlays.last(), Some(Overlay::Folder(p)) if p.request_id == id)
                    && let Some(path) = v["path"].as_str().filter(|p| !p.is_empty())
                {
                    self.overlays.pop();
                    self.folder_chosen(path.to_owned());
                }
            }
            After::Dirs(id) => {
                if let Some(Overlay::Folder(p)) = self.overlays.last_mut()
                    && p.request_id == id
                {
                    v["path"].as_str().unwrap_or_default().clone_into(&mut p.path);
                    p.busy = false;
                    p.remembers = v["recent"].is_array();
                    p.recent = v["recent"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|item| {
                            let path = item["path"].as_str()?;
                            Some(Dir { name: path.to_owned(), path: path.to_owned(), git: false })
                        })
                        .collect();
                    p.parent = v["parent"].as_str().map(str::to_owned);
                    p.dirs = v["dirs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|d| Dir {
                            name: d["name"].as_str().unwrap_or_default().to_owned(),
                            path: d["path"].as_str().unwrap_or_default().to_owned(),
                            git: d["isGit"].as_bool().unwrap_or(false),
                        })
                        .collect();
                    p.query.clear();
                    p.cursor = 0;
                    p.error = None;
                }
            }
            After::Saved(_) => {
                let saved = self
                    .overlays
                    .iter()
                    .rposition(|o| matches!(o, Overlay::Form(_) | Overlay::NewBot(_) | Overlay::Group(_)));
                // A new bot or group opens ready to type; an edit goes back to where it was.
                let created = saved.is_some_and(|i| match &self.overlays[i] {
                    Overlay::NewBot(_) => true,
                    Overlay::Group(g) => g.group_id.is_none(),
                    _ => false,
                });
                if let Some(i) = saved {
                    self.overlays.truncate(i);
                }
                if let Some(id) = v["bot"]["id"].as_str() {
                    let id = id.to_owned();
                    self.upsert_bot(&v["bot"], false);
                    if created {
                        self.select(&id);
                        self.chat_page = true;
                        self.focus = Focus::Chat;
                        self.typing = true;
                    }
                }
            }
            After::Connectors | After::Skills => {
                let connectors = matches!(after, After::Connectors);
                let list = self.set_plugins(connectors, v["items"].as_array().cloned().unwrap_or_default());
                if connectors {
                    self.market.0 = list;
                } else {
                    self.market.1 = list;
                }
                for o in &mut self.overlays {
                    if let Overlay::Form(f) = o {
                        if connectors {
                            merge_toggles(&mut f.connectors, &self.market.0);
                        } else {
                            merge_toggles(&mut f.skills, &self.market.1);
                        }
                    }
                }
            }
        }
    }

    fn acknowledge_send(&mut self, key: &str, nonce: &str) {
        if self.confirm_send(key, nonce) {
            super::kofi::sent();
            self.flash("Sent");
        }
    }

    fn confirm_send(&mut self, key: &str, nonce: &str) -> bool {
        let Some(sent) = self.sends.get(key).filter(|s| s.nonce == nonce) else { return false };
        if self.drafts.get(key).is_some_and(|d| d.text == sent.text)
            && self.files.get(key).map_or(&[][..], Vec::as_slice) == sent.files
        {
            self.drafts.remove(key);
            self.files.remove(key);
            self.pasted.remove(key);
            if self.selected.as_deref() == Some(key) {
                self.reply = None;
            }
        }
        if let Some(task) = self.sends.remove(key).and_then(|send| send.task) {
            task.abort();
        }
        true
    }

    fn sent(&mut self, key: &str, nonce: &str, result: Result<Value, String>) {
        let Some(sent) = self.sends.get_mut(key).filter(|s| s.nonce == nonce) else { return };
        sent.task = None;
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.flash(&format!("{error} Draft kept; Enter retries, Ctrl+C clears text."));
                return;
            }
        };
        let entry = Entry::parse(&value["entry"]).filter(|(bot, row)| {
            bot == key
                && row.kind == Kind::User
                && row.data["clientNonce"].as_str().is_none_or(|returned| returned == nonce)
        });
        let evidence = entry
            .as_ref()
            .filter(|(bot, row)| self.entries.get(bot).is_none_or(|rows| !rows.contains_key(&row.seq)))
            .map(|(bot, row)| (bot.as_str(), row));
        if let SendReceiptCheck::Direct(connection) = sent.receipt
            && self.send_receipts.confirm(connection, evidence)
        {
            self.confirm_send(key, nonce);
            // The stream may already hold a newer copy (its delivery status).
            if let Some((bot, row)) = entry {
                super::kofi::sent();
                self.insert_missing_entry(bot, row);
            }
            self.flash("Sent");
        } else if let Some((_, row)) = entry
            .filter(|(_, row)| !row.id.is_empty() && (1..i64::MAX).contains(&row.seq) && row.data["removed"] != true)
        {
            sent.receipt = SendReceiptCheck::Pending { id: row.id, seq: row.seq, request: None };
            self.start_send_checks();
        } else {
            self.flash(UNCONFIRMED_SEND);
        }
    }

    fn reset_send_checks(&mut self, since: i64) {
        self.send_receipt_ready = false;
        for send in self.sends.values_mut() {
            if let SendReceiptCheck::Pending { request, .. } = &mut send.receipt {
                *request = None;
            } else if since == 0 {
                send.receipt = SendReceiptCheck::Unverified;
            }
        }
    }

    fn start_send_checks(&mut self) {
        // Give counted replay a chance to confirm the send directly.
        if !self.send_receipt_ready
            || (self.replay_sends.is_some() && self.catch_up_left.is_some())
            || !self.send_receipts.is_current(self.send_receipt_connection)
        {
            return;
        }
        for (key, send) in &mut self.sends {
            let SendReceiptCheck::Pending { seq, request, .. } = &mut send.receipt else { continue };
            if request.is_some() {
                continue;
            }
            let request_id = uuid::Uuid::new_v4();
            *request = Some(request_id);
            self.client.spawn_call(
                "history",
                json!({"botId": key, "beforeSeq": *seq + 1, "limit": 1}),
                After::CheckSendReceipt {
                    key: key.clone(),
                    request: request_id,
                    epoch: self.mirror_epoch,
                    connection: self.send_receipt_connection,
                },
                self.tx.clone(),
            );
        }
    }

    fn finish_send_check(&mut self, key: &str, request: uuid::Uuid, connection: u64, result: Result<Value, String>) {
        let Some(send) = self.sends.get_mut(key) else { return };
        let SendReceiptCheck::Pending { id, seq, request: Some(active) } = &send.receipt else { return };
        if request != *active {
            return;
        }
        let matches = |row: &Entry| {
            row.id == *id
                && row.seq == *seq
                && row.kind == Kind::User
                && row.data["clientNonce"].as_str() == Some(send.nonce.as_str())
                && row.data["removed"] != true
        };
        let staged = self.replay_chats.as_ref().and_then(|chats| chats.get(key)).and_then(|chat| chat.entries.get(seq));
        let held = self.entries.get(key).and_then(|rows| rows.get(seq));
        let checked = result
            .ok()
            .and_then(|value| {
                let [row] = value["entries"].as_array()?.as_slice() else { return None };
                Entry::parse(row)
            })
            .filter(|(bot, row)| bot == key && matches(row) && staged.or(held).is_none_or(matches));
        let evidence = checked
            .as_ref()
            .filter(|(_, row)| held.is_none() && !outside_window(self.floors.get(key).copied(), false, row))
            .map(|(bot, row)| (bot.as_str(), row));
        if !self.send_receipts.confirm(connection, evidence) {
            return;
        }
        let Some((bot, row)) = checked else {
            send.receipt = SendReceiptCheck::Unverified;
            self.flash(UNCONFIRMED_SEND);
            return;
        };
        let nonce = send.nonce.clone();
        if !self.outside_window(key, &row) {
            if let Some(chats) = self.replay_chats.as_mut() {
                chats.entry(bot.clone()).or_default().verified_sends.insert(row.seq);
            }
            self.insert_missing_entry(bot, row);
        }
        self.acknowledge_send(key, &nonce);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Editor, FIELDS, Field, PendingSend, SendReceipts, test_app};
    use super::*;
    use axum::http::StatusCode;
    use std::time::Duration;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio::sync::oneshot;

    fn entry_event(seq: i64) -> Value {
        json!({"type": "entry", "entry": {"botId": "a", "id": format!("e{seq}"), "seq": seq, "turn": seq,
            "kind": "user"}})
    }

    fn final_reply_event(seq: i64, created_at: i64, text: &str) -> Value {
        json!({"type": "entry", "entry": {
            "botId": "a", "id": format!("e{seq}"), "seq": seq, "rev": seq, "turn": seq, "kind": "agent",
            "data": {"final": true, "text": text}, "createdAt": created_at, "updatedAt": created_at
        }})
    }

    fn entry_at_rev(mut event: Value, rev: i64) -> Value {
        event["rev"] = rev.into();
        event["entry"]["rev"] = rev.into();
        event
    }

    fn legacy_bot_event(rev: i64, started_at: Option<i64>, last_at: i64, preview: &str) -> Value {
        let status = if started_at.is_some() { "working" } else { "idle" };
        json!({"type": "bot", "rev": rev, "bot": {
            "id": "a", "rev": rev, "name": "Alpha", "status": status,
            "startedAt": started_at, "lastAt": last_at, "lastMessage": preview
        }})
    }

    fn seqs(app: &App) -> Vec<i64> {
        app.entries.get("a").map(|m| m.keys().copied().collect()).unwrap_or_default()
    }

    fn prepare_history(app: &mut App, bot: &str) -> After {
        let page = super::super::PendingHistory::new(app.oldest(bot).unwrap_or(i64::MAX));
        let request = page.request;
        app.pending_history.insert(bot.to_owned(), page);
        After::History(bot.to_owned(), request)
    }

    fn apply_history_row(app: &mut App, row: &Value) {
        let after = prepare_history(app, "a");
        app.on_reply(after, Ok(json!({"entries": [row]})));
    }

    fn connect(app: &mut App, since: i64) {
        app.on_msg(Msg::Connected {
            since,
            connection: app.send_receipt_connection,
            send_receipts: app.send_receipts.clone(),
        });
        app.on_event(&json!({"type": "hello"}));
    }

    #[test]
    fn rewritten_entry_below_the_floor_waits_for_paging() {
        let mut app = test_app();
        for seq in [5, 6] {
            app.on_event(&entry_event(seq));
        }
        connect(&mut app, 40);
        app.on_event(&entry_event(2));
        assert_eq!(seqs(&app), [5, 6], "below the floor: dropped");
        let mut update = entry_event(5);
        update["entry"]["reactions"] = json!(["x"]);
        app.on_event(&update);
        app.on_event(&entry_event(7));
        assert_eq!(seqs(&app), [5, 6, 7], "held updates and newer entries apply");
        apply_history_row(&mut app, &entry_event(3)["entry"]);
        assert_eq!(seqs(&app), [3, 5, 6, 7], "history inserts older entries");
    }

    #[test]
    fn a_full_catch_up_accepts_entries_in_rev_order() {
        let mut app = test_app();
        app.on_event(&entry_event(9)); // a sent message's reply, before catch-up
        connect(&mut app, 0);
        for seq in [4, 2, 7] {
            app.on_event(&entry_event(seq));
        }
        assert_eq!(seqs(&app), [2, 4, 7, 9]);
        connect(&mut app, 40);
        app.on_event(&entry_event(1));
        assert_eq!(seqs(&app), [2, 4, 7, 9], "a later resume floors at the loaded window");
        connect(&mut app, 0);
        app.on_event(&entry_event(1));
        assert_eq!(seqs(&app), [1, 2, 4, 7, 9], "each connection replaces the floors");
    }

    #[test]
    fn a_model_list_the_computer_read_again_replaces_the_one_shown() {
        let mut app = test_app();
        let key = App::model_key("codex", None);
        app.keep_models(key.clone(), &json!({"models": [{"id": "old", "name": "Old"}], "fastMode": true}));
        app.on_event(&json!({"type": "models", "backend": "codex", "loginProfile": null,
            "models": {"models": [{"id": "new", "name": "New"}]}}));
        let ids: Vec<_> = app
            .models_for("codex", None)
            .and_then(Option::as_ref)
            .into_iter()
            .flatten()
            .map(|m| m.id.clone())
            .collect();
        assert_eq!(ids, ["new"]);
        assert!(!app.has_fast_mode("codex", None), "Fast mode goes with the list that dropped it");
        app.on_event(&json!({"type": "models", "loginProfile": null, "models": {"models": []}}));
        assert!(app.models_for("codex", None).is_some(), "an event without its agent is passed over");
    }

    #[test]
    fn floor_rule_is_pure() {
        let (_, e) = Entry::parse(&entry_event(3)["entry"]).expect("parses");
        assert!(outside_window(Some(5), false, &e));
        assert!(!outside_window(Some(5), true, &e));
        assert!(!outside_window(Some(3), false, &e));
        assert!(!outside_window(None, false, &e));
    }

    #[tokio::test]
    async fn send_failure_keeps_text_files_and_nonce_for_retry() {
        let mut app = test_app();
        let key = "bot";
        let files = vec![std::path::PathBuf::from("/tmp/report.txt")];
        app.drafts.insert(key.into(), Editor::with("draft"));
        app.files.insert(key.into(), files.clone());
        app.sends.insert(
            key.into(),
            PendingSend {
                text: "draft".into(),
                files: files.clone(),
                nonce: "same-nonce".into(),
                task: Some(tokio::spawn(async {}).abort_handle()),
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        let reply = After::Sent { key: key.into(), nonce: "same-nonce".into(), epoch: app.mirror_epoch };
        app.on_reply(reply.clone(), Err("Disconnected".into()));
        assert_eq!(app.drafts[key].text, "draft");
        assert_eq!(app.files[key], files);
        assert_eq!(app.sends[key].nonce, "same-nonce");
        assert!(app.sends[key].task.is_none());
        app.on_reply(reply, Ok(json!({})));
        assert!(!app.drafts.contains_key(key));
        assert!(!app.files.contains_key(key));
    }

    #[test]
    fn late_send_success_does_not_erase_a_new_draft() {
        let mut app = test_app();
        app.drafts.insert("bot".into(), Editor::with("new draft"));
        app.sends.insert(
            "bot".into(),
            PendingSend {
                text: "old draft".into(),
                files: vec![],
                nonce: "n".into(),
                task: None,
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        app.on_reply(After::Sent { key: "bot".into(), nonce: "n".into(), epoch: app.mirror_epoch }, Ok(json!({})));
        assert_eq!(app.drafts["bot"].text, "new draft");
    }

    #[tokio::test]
    async fn restored_chat_keeps_a_pending_send_when_its_old_reply_arrives() {
        let mut app = test_app();
        let files = vec![std::path::PathBuf::from("report.txt")];
        app.selected = Some("a".into());
        app.reply = Some("abandoned-quote".into());
        app.pick = app.reply.clone();
        app.drafts.insert("a".into(), Editor::with("draft"));
        app.files.insert("a".into(), files.clone());
        let old_task = tokio::spawn(std::future::pending::<()>());
        app.sends.insert(
            "a".into(),
            PendingSend {
                text: "draft".into(),
                files: files.clone(),
                nonce: "pending".into(),
                task: Some(old_task.abort_handle()),
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        let old_reply = After::Sent { key: "a".into(), nonce: "pending".into(), epoch: app.mirror_epoch };
        let old_save = After::Saved(app.mirror_epoch);
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        assert!(old_task.await.unwrap_err().is_cancelled());
        let mut old = entry_event(10);
        old["entry"]["data"] = json!({"clientNonce": "pending", "text": "draft"});
        app.on_reply(old_reply.clone(), Ok(json!({"entry": old["entry"]})));
        app.on_reply(old_save, Ok(json!({"bot": {"id": "abandoned"}})));
        assert!(app.bots.is_empty());
        assert!(app.entries.is_empty(), "the old reply can't put its entry back");
        assert!(app.reply.is_none() && app.pick.is_none());
        assert_eq!(app.drafts["a"].text, "draft");
        assert_eq!(app.files["a"], files);
        assert_eq!(app.sends["a"].nonce, "pending");
        assert!(app.sends["a"].task.is_none());
        let retry = tokio::spawn(std::future::pending::<()>());
        app.sends.get_mut("a").unwrap().task = Some(retry.abort_handle());
        app.on_reply(old_reply, Err("Abandoned request".into()));
        assert!(app.sends["a"].task.is_some(), "the old reply cannot unlock the retry");
        let mut restored = entry_event(2);
        restored["entry"]["data"] = json!({"clientNonce": "pending", "text": "draft"});
        app.on_msg(Msg::ReplayComplete);
        app.on_event(&restored);
        assert!(retry.await.unwrap_err().is_cancelled());
        assert_eq!(seqs(&app), [2]);
        assert!(!app.sends.contains_key("a"));
        assert!(!app.drafts.contains_key("a"));
        assert!(!app.files.contains_key("a"));
    }

    #[test]
    fn restored_nonce_confirms_the_old_send_and_keeps_the_edited_draft() {
        let mut app = test_app();
        let files = vec![std::path::PathBuf::from("draft.txt")];
        app.drafts.insert("a".into(), Editor::with("original "));
        app.files.insert("a".into(), files.clone());
        app.sends.insert(
            "a".into(),
            PendingSend {
                text: "original".into(),
                files: files.clone(),
                nonce: "pending".into(),
                task: None,
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        app.on_msg(Msg::ReplayComplete);
        let mut restored = entry_event(2);
        restored["entry"]["data"] = json!({"clientNonce": "unrelated", "text": "other message"});
        app.on_event(&restored);
        assert!(app.sends.contains_key("a"), "another nonce leaves this send pending");
        let mut confirmed = entry_event(3);
        confirmed["entry"]["data"] = json!({"clientNonce": "pending", "text": "original"});
        app.on_event(&confirmed);
        assert!(!app.sends.contains_key("a"));
        assert_eq!(app.drafts["a"].text, "original ");
        assert_eq!(app.files["a"], files);
    }

    #[tokio::test]
    async fn rewind_cancels_plain_sends_waiting_for_connection_or_reply() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use tokio::io::AsyncReadExt as _;
        use tokio::net::TcpListener;

        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = reserved.local_addr().unwrap();
        drop(reserved);
        let client = crate::tui::net::Client::new(&format!("http://{addr}"), Some("fixture"));
        let (tx, _rx) = unbounded_channel();
        let mut app = App::new(client, tx, String::new());
        let bot = json!({"id": "a", "name": "Alpha", "status": "idle"});
        app.upsert_bot(&bot, false);
        app.selected = Some("a".into());
        app.typing = true;
        app.drafts.insert("a".into(), Editor::with("My draft"));
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.on_key(enter);
        let nonce = app.sends["a"].nonce.clone();
        let old_task = app.sends["a"].task.clone().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!old_task.is_finished());
        app.on_msg(Msg::Rewind(SendReceipts::default()));

        let listener = TcpListener::bind(addr).await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(700), listener.accept()).await.is_err());
        assert!(old_task.is_finished());
        app.upsert_bot(&bot, false);
        app.on_key(enter);
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept()).await.unwrap().unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 4096];
        tokio::time::timeout(Duration::from_secs(5), async {
            while !request.ends_with(b"}") {
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
            }
        })
        .await
        .unwrap();
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("POST /api/send ") && request.contains(&nonce));
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        assert_eq!(tokio::time::timeout(Duration::from_secs(5), socket.read(&mut chunk)).await.unwrap().unwrap(), 0);
        assert_eq!(app.drafts["a"].text, "My draft");
        assert_eq!(app.sends["a"].nonce, nonce);
    }

    #[test]
    fn computer_permission_survives_loading_the_editor() {
        let mut app = test_app();
        app.upsert_bot(&json!({"id":"b", "computer":true}), false);
        assert!(app.bots["b"].computer);
        assert!(FIELDS.contains(&Field::Computer));
    }

    #[test]
    fn fast_mode_survives_loading_the_editor() {
        let mut app = test_app();
        app.upsert_bot(&json!({"id":"b", "fastMode":true}), false);
        assert!(app.bots["b"].fast_mode);
        assert!(FIELDS.contains(&Field::Fast));
    }

    #[test]
    fn a_refused_fast_mode_reads_its_reason() {
        let mut app = test_app();
        app.upsert_bot(
            &json!({"id":"b", "fastMode":true, "fastModeSupport":"refused", "fastModeIssue":"requires extra usage"}),
            false,
        );
        assert!(app.bots["b"].fast_shown());
        assert!(app.bots["b"].fast_refusal().is_some_and(|why| why.contains("requires extra usage")));
        app.upsert_bot(&json!({"id":"g", "kind":"group", "fastModeSupport":"offered"}), false);
        assert!(!app.bots["g"].fast_shown());
    }

    #[tokio::test]
    async fn a_dropped_connection_forgets_the_hosts_version() {
        let mut app = test_app();
        app.host_version = "2.50.1".into();
        app.on_msg(Msg::Disconnected { since: 0 });
        assert!(app.host_version.is_empty(), "the host that answers next may be another build");
    }

    #[test]
    fn a_reaction_on_an_old_reply_does_not_toast_as_done() {
        let mut app = test_app();
        let mut reply = final_reply_event(1, 100, "Old reply");
        app.on_event(&reply);
        app.on_event(&legacy_bot_event(2, Some(200), 100, "Old reply"));
        reply["entry"]["data"]["reactions"] = json!(["👍"]);
        reply["entry"]["updatedAt"] = 210.into();
        app.on_event(&reply);
        app.on_event(&legacy_bot_event(4, None, 210, "Old reply"));
        assert_eq!(app.toasts.iter().map(|toast| toast.text.as_str()).collect::<Vec<_>>(), Vec::<&str>::new());
    }

    #[test]
    fn a_legacy_saved_reply_toasts_on_live_idle_even_with_the_same_text() {
        for catch_up in [None, Some(3)] {
            let mut app = test_app();
            let text = "Same reply";
            app.on_event(&json!({"type": "hello", "rev": 3, "catchUp": catch_up}));
            app.on_event(&legacy_bot_event(1, Some(50), 0, text));
            let previous = entry_at_rev(final_reply_event(1, 100, text), 2);
            app.on_event(&previous);
            app.on_event(&legacy_bot_event(3, None, 100, text));
            assert!(app.toasts.is_empty(), "historical working through idle stays quiet");

            app.on_event(&legacy_bot_event(4, Some(200), 100, text));
            let mut reply = entry_at_rev(final_reply_event(2, 210, text), 5);
            reply["entry"]["data"]["final"] = false.into();
            app.on_event(&reply);
            reply["entry"]["data"]["final"] = true.into();
            let reply = entry_at_rev(reply, 6);
            app.on_event(&reply);
            app.on_event(&legacy_bot_event(7, Some(200), 210, text));
            assert!(app.toasts.is_empty(), "a saved reply still needs a live idle snapshot");
            let idle = legacy_bot_event(8, None, 210, text);
            app.on_event(&idle);
            assert_eq!(app.toasts.len(), 1);
            assert_eq!(app.toasts[0].text, "Alpha is done");
            app.toasts.clear();
            app.on_event(&reply);
            app.on_event(&idle);
            assert!(app.toasts.is_empty(), "repeated entry and idle events stay quiet");
        }
    }

    fn legacy_chat_window() -> App {
        let mut app = test_app();
        app.on_event(&json!({"type": "hello", "rev": 210}));
        app.on_event(&legacy_bot_event(9, Some(200), 100, "Old reply"));
        for seq in 11..=210 {
            let mut tool = entry_event(seq);
            tool["rev"] = seq.into();
            tool["entry"]["kind"] = "tool".into();
            app.on_event(&tool);
        }
        app
    }

    #[test]
    fn a_legacy_reply_below_the_window_toasts_after_reconnect() {
        let window: Vec<_> = (11..=210).collect();
        for (catch_up, reply_rev) in [(None, 211), (Some(2), 211), (None, 213), (Some(1), 213)] {
            let mut app = legacy_chat_window();
            app.on_msg(Msg::Connected { since: 210, connection: 0, send_receipts: SendReceipts::default() });
            app.on_event(&json!({"type": "hello", "rev": 212, "catchUp": catch_up}));

            let reply = entry_at_rev(final_reply_event(10, 210, "Fresh reply"), reply_rev);
            let working = legacy_bot_event(212, Some(200), 215, "Fresh reply");
            if reply_rev < 212 {
                app.on_event(&reply);
                app.on_event(&working);
            } else {
                app.on_event(&working);
                app.on_event(&reply);
            }
            assert_eq!(seqs(&app), window, "the saved final stays below the loaded window");
            assert!(app.toasts.is_empty(), "the working turn may have a saved final");

            let idle = legacy_bot_event(214, None, 215, "Fresh reply");
            app.on_event(&idle);
            assert_eq!(
                app.toasts.iter().map(|toast| toast.text.as_str()).collect::<Vec<_>>(),
                ["Alpha is done"],
                "live idle needs the saved reply from catchUp={catch_up:?}, rev={reply_rev}"
            );
            app.toasts.clear();
            app.on_event(&reply);
            app.on_event(&idle);
            assert!(app.toasts.is_empty(), "duplicates are quiet");
            assert_eq!(seqs(&app), window);
            apply_history_row(&mut app, &reply["entry"]);
            assert_eq!(seqs(&app), (10..=210).collect::<Vec<_>>(), "normal paging loads the final");
            assert!(app.toasts.is_empty(), "history is quiet");
        }
    }

    #[test]
    fn an_outside_legacy_completion_that_ended_in_replay_stays_quiet() {
        for catch_up in [None, Some(2)] {
            let mut app = legacy_chat_window();
            app.on_msg(Msg::Connected { since: 210, connection: 0, send_receipts: SendReceipts::default() });
            app.on_event(&json!({"type": "hello", "rev": 212, "catchUp": catch_up}));
            let reply = entry_at_rev(final_reply_event(10, 210, "Fresh reply"), 211);
            app.on_event(&reply);
            app.on_event(&legacy_bot_event(212, None, 215, "Fresh reply"));
            assert!(app.toasts.is_empty(), "replayed final through idle stays quiet");

            app.on_event(&legacy_bot_event(213, None, 215, "Fresh reply"));
            app.on_event(&legacy_bot_event(214, Some(300), 215, "Fresh reply"));
            let mut reply = entry_at_rev(reply, 215);
            reply["entry"]["updatedAt"] = 310.into();
            reply["entry"]["data"]["reactions"] = json!(["👍"]);
            app.on_event(&reply);
            app.on_event(&legacy_bot_event(216, None, 310, "Fresh reply"));
            assert!(app.toasts.is_empty(), "a replayed reply reacted to in the next turn stays quiet");
            assert_eq!(seqs(&app), (11..=210).collect::<Vec<_>>());
        }
    }

    #[test]
    fn an_outside_legacy_reply_is_discarded_with_its_turn() {
        let next_turn = legacy_bot_event(212, Some(300), 215, "Fresh reply");
        let deletion = json!({"type": "bot", "rev": 212, "bot": {"id": "a", "rev": 212, "deleted": true}});
        for (change, started_at) in
            [(Msg::Event(next_turn), 300), (Msg::Event(deletion), 200), (Msg::Rewind(SendReceipts::default()), 200)]
        {
            let mut app = legacy_chat_window();
            connect(&mut app, 210);
            app.on_event(&final_reply_event(10, 210, "Fresh reply"));
            app.on_msg(change);
            app.on_msg(Msg::ReplayComplete);
            app.on_event(&legacy_bot_event(213, Some(started_at), 215, "Fresh reply"));
            app.on_event(&legacy_bot_event(214, None, 215, "Fresh reply"));
            assert!(app.toasts.is_empty(), "a replaced or removed turn cannot complete");
        }
    }

    #[test]
    fn a_legacy_reply_removed_after_paging_stays_quiet() {
        let mut app = legacy_chat_window();
        connect(&mut app, 210);
        let mut reply = final_reply_event(10, 210, "Fresh reply");
        app.on_event(&reply);
        apply_history_row(&mut app, &reply["entry"]);
        reply["entry"]["data"]["removed"] = true.into();
        app.on_event(&reply);
        app.on_event(&legacy_bot_event(213, None, 215, "Fresh reply"));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn a_legacy_reply_removed_before_working_stays_quiet() {
        let mut app = legacy_chat_window();
        app.bots.remove("a");
        connect(&mut app, 210);
        let mut reply = final_reply_event(10, 210, "Fresh reply");
        app.on_event(&reply);
        reply["entry"]["data"]["removed"] = true.into();
        app.on_event(&reply);
        app.on_event(&legacy_bot_event(213, Some(200), 240, "Fresh reply"));
        app.on_event(&legacy_bot_event(214, None, 240, "Fresh reply"));
        assert!(app.toasts.is_empty());
        assert_eq!(seqs(&app), (11..=210).collect::<Vec<_>>());
    }

    #[test]
    fn a_legacy_completion_requires_positive_raw_revisions_in_the_mirror_and_outside_it() {
        for seq in [10, 211] {
            for (entry_rev, idle_rev) in [(None, Some(214)), (Some(0), Some(214)), (Some(212), None)] {
                let mut app = legacy_chat_window();
                connect(&mut app, 210);
                let mut reply = entry_at_rev(final_reply_event(seq, 210, "Fresh reply"), 212);
                reply["entry"]["rev"] = json!(entry_rev);
                app.on_event(&reply);
                let mut idle = legacy_bot_event(214, None, 210, "Fresh reply");
                idle["bot"]["rev"] = json!(idle_rev);
                app.on_event(&idle);
                assert!(app.toasts.is_empty(), "seq {seq}, entry rev {entry_rev:?}, Idle rev {idle_rev:?}");
            }
        }
    }

    #[test]
    fn an_exact_completion_field_governs_the_idle_snapshot_after_a_legacy_turn() {
        for reply_id in [None, Some("exact-reply")] {
            let mut app = legacy_chat_window();
            connect(&mut app, 210);
            app.on_event(&final_reply_event(10, 210, "Fresh reply"));
            let mut idle = legacy_bot_event(212, None, 215, "Fresh reply");
            idle["bot"]["completionReplyId"] = json!(reply_id);
            app.on_event(&idle);
            assert_eq!(app.toasts.len(), usize::from(reply_id.is_some()));
        }
    }

    #[test]
    fn a_legacy_turn_with_paged_history_and_a_queued_message_stays_quiet() {
        let mut app = test_app();
        app.on_event(&entry_event(5));
        connect(&mut app, 8);
        app.on_event(&legacy_bot_event(9, Some(200), 100, "Old reply"));
        let mut reaction = final_reply_event(2, 100, "Old reply");
        reaction["entry"]["data"]["reactions"] = json!(["👍"]);
        reaction["entry"]["updatedAt"] = 210.into();
        app.on_event(&reaction);
        assert_eq!(seqs(&app), [5], "the older reaction waits for history");
        let mut queued = entry_event(6);
        queued["entry"]["createdAt"] = 220.into();
        queued["entry"]["data"] = json!({"text": "Queued message", "status": "queued"});
        app.on_event(&queued);
        let mut idle = legacy_bot_event(12, None, 220, "You: Queued message");
        idle["bot"]["unread"] = 1.into();
        app.on_event(&idle);
        apply_history_row(&mut app, &reaction["entry"]);
        assert_eq!(seqs(&app), [2, 5, 6]);
        assert!(app.toasts.is_empty(), "the queue, unread count, and history are quiet");
    }

    #[test]
    fn a_legacy_idle_snapshot_without_saved_evidence_stays_quiet() {
        let mut app = test_app();
        app.on_event(&legacy_bot_event(1, Some(200), 100, "Old reply"));
        app.on_event(&legacy_bot_event(2, None, 210, "Unsynced reply"));
        assert!(app.toasts.is_empty());
        let reply = final_reply_event(1, 210, "Unsynced reply");
        apply_history_row(&mut app, &reply["entry"]);
        assert!(app.toasts.is_empty(), "history after idle cannot trigger a late toast");

        let mut working = legacy_bot_event(3, None, 210, "Unsynced reply");
        working["bot"]["status"] = "working".into();
        app.on_event(&working);
        app.on_event(&final_reply_event(2, 230, "Unattributed reply"));
        app.on_event(&legacy_bot_event(5, None, 230, "Unattributed reply"));
        assert!(app.toasts.is_empty(), "a missing turn start cannot identify the reply");
    }

    #[test]
    fn a_legacy_turn_in_another_chat_does_not_toast_here() {
        let mut app = test_app();
        let mut working = legacy_bot_event(1, Some(200), 100, "Old reply");
        working["bot"]["workingChat"] = "group".into();
        app.on_event(&working);
        app.on_event(&final_reply_event(1, 210, "A concurrent reply"));
        app.on_event(&legacy_bot_event(3, None, 210, "A concurrent reply"));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn stopped_turn_with_a_reply_does_not_toast_as_done() {
        let mut app = test_app();
        app.upsert_bot(
            &json!({
                "id": "a", "name": "Alpha", "status": "working", "startedAt": 200,
                "lastAt": 210, "lastMessage": "Sent before Stop", "unread": 1, "completionReplyId": null,
            }),
            true,
        );
        app.on_event(&final_reply_event(1, 210, "Sent before Stop"));
        app.upsert_bot(
            &json!({
                "id": "a", "name": "Alpha", "status": "idle", "completionReplyId": null,
                "lastAt": 210, "lastMessage": "Sent before Stop", "unread": 1,
            }),
            true,
        );
        assert!(app.toasts.is_empty(), "a stopped turn is not done");
        assert_eq!(app.bots["a"].last_message, "Sent before Stop");
    }

    #[test]
    fn a_completion_toast_only_uses_a_new_live_reply() {
        let mut app = test_app();
        app.on_event(&json!({"type":"bot", "rev":5, "bot":{"id":"a", "status":"working", "completionReplyId":null}}));
        app.on_event(&json!({"type":"hello", "rev":8}));
        let mut event = json!({"type":"bot", "rev":7, "bot":{
            "id":"a", "name":"Alpha", "status":"idle", "completionReplyId":"replayed"
        }});
        app.on_event(&event);
        event["rev"] = 9.into();
        app.on_event(&event);
        assert!(app.toasts.is_empty(), "catch-up and later bot metadata stay quiet");

        event["rev"] = 10.into();
        event["bot"]["completionReplyId"] = "new-reply".into();
        app.on_event(&event);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Alpha is done");
        app.toasts.clear();
        app.on_event(&event);
        assert!(app.toasts.is_empty(), "the same completion only toasts once");
    }

    fn completion_event(id: &str, rev: i64, status: &str, reply: Option<&str>) -> Msg {
        Msg::Event(json!({"type": "bot", "rev": rev, "bot": {
            "id": id, "rev": rev, "name": id, "status": status, "completionReplyId": reply
        }}))
    }

    type StreamRequest = (i64, UnboundedSender<Value>);
    type HistoryRequest = (Value, oneshot::Sender<(StatusCode, Value)>);

    async fn stream_host() -> (crate::tui::net::Client, UnboundedReceiver<StreamRequest>) {
        let (client, events, _) = stream_and_history_host("2.54.0").await;
        (client, events)
    }

    async fn stream_and_history_host(
        version: &'static str,
    ) -> (crate::tui::net::Client, UnboundedReceiver<StreamRequest>, UnboundedReceiver<HistoryRequest>) {
        use axum::extract::Query;
        use axum::response::sse::{Event, Sse};
        use axum::routing::{get, post};
        use axum::{Json, Router};
        use futures::StreamExt;
        use tokio_stream::wrappers::UnboundedReceiverStream;

        let (opened, requests) = unbounded_channel();
        let (history, pages) = unbounded_channel::<HistoryRequest>();
        let router = Router::new().route("/api/hello", post(move || async move { Json(json!({"version": version})) })).route(
            "/api/history",
            post(move |Json(body): Json<Value>| {
                let (send, receive) = oneshot::channel();
                let _ = history.send((body, send));
                async move {
                    let (status, body) = receive.await.unwrap_or((StatusCode::NOT_FOUND, Value::Null));
                    (status, Json(body))
                }
            }),
        ).route(
            "/events",
            get(move |Query(query): Query<HashMap<String, String>>| {
                let (send, receive) = unbounded_channel::<Value>();
                opened.send((query["since"].parse().unwrap(), send)).unwrap();
                async move { Sse::new(UnboundedReceiverStream::new(receive).map(|v| Event::default().json_data(v))) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (crate::tui::net::Client::new(&url, Some("fixture")), requests, pages)
    }

    async fn expect_stream(requests: &mut UnboundedReceiver<StreamRequest>, since: i64) -> UnboundedSender<Value> {
        let (requested, send) = tokio::time::timeout(Duration::from_secs(5), requests.recv())
            .await
            .expect("the client starts another events stream")
            .unwrap();
        assert_eq!(requested, since);
        send
    }

    async fn read_events(app: &mut App, messages: &mut UnboundedReceiver<Msg>, mut count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while count > 0 {
                let m = messages.recv().await.unwrap();
                if matches!(m, Msg::Event(_)) {
                    count -= 1;
                }
                app.on_msg(m);
            }
        })
        .await
        .expect("the client receives the expected events");
    }

    struct HistoryTurn {
        app: App,
        events: UnboundedSender<Value>,
        messages: UnboundedReceiver<Msg>,
        pages: UnboundedReceiver<HistoryRequest>,
        idle: Value,
    }

    fn history_tool(seq: i64) -> Value {
        let mut tool = entry_at_rev(entry_event(seq), seq);
        tool["entry"]["kind"] = "tool".into();
        tool["entry"]["createdAt"] = 211.into();
        tool
    }

    fn stored_history(replies: impl IntoIterator<Item = Value>, next_seq: i64) -> Vec<Value> {
        replies.into_iter().chain((11..next_seq).map(|seq| history_tool(seq)["entry"].clone())).collect()
    }

    fn history_page(rows: &[Value], before: i64, limit: usize) -> (StatusCode, Value) {
        let end = rows.partition_point(|row| row["seq"].as_i64().unwrap() < before);
        (StatusCode::OK, json!({"entries": &rows[end.saturating_sub(limit)..end]}))
    }

    async fn history_turn(floor: i64, last_at: i64) -> HistoryTurn {
        let (client, mut requests, mut pages) = stream_and_history_host("2.20.1").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);

        let events = expect_stream(&mut requests, 0).await;
        let working_rev = floor + 200;
        events.send(json!({"type": "hello", "rev": working_rev})).unwrap();
        for seq in floor..working_rev {
            events.send(history_tool(seq)).unwrap();
        }
        events.send(legacy_bot_event(working_rev, Some(200), last_at, "Saved reply")).unwrap();
        read_events(&mut app, &mut messages, 202).await;
        assert_eq!(seqs(&app), (floor..working_rev).collect::<Vec<_>>());
        assert_eq!(app.bots["a"].status, Status::Working);
        assert!(pages.try_recv().is_err());
        let idle = legacy_bot_event(working_rev + 1, None, last_at, "Saved reply");
        HistoryTurn { app, events, messages, pages, idle }
    }

    async fn truncated_history_turn(preview: &str) -> (HistoryTurn, Value) {
        let (client, mut requests, pages) = stream_and_history_host("2.20.1").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);

        let initial = expect_stream(&mut requests, 0).await;
        let mut tool = history_tool(11);
        tool["entry"]["createdAt"] = 205.into();
        tool["entry"]["turn"] = 1.into();
        let tool_row = tool["entry"].clone();
        initial.send(json!({"type": "hello", "rev": 13, "hostId": "legacy"})).unwrap();
        initial.send(tool).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!(seqs(&app), [11]);
        assert!(app.bots.is_empty());
        drop(initial);

        let events = expect_stream(&mut requests, 13).await;
        events.send(json!({"type": "hello", "rev": 14, "hostId": "legacy"})).unwrap();
        events.send(legacy_bot_event(14, Some(200), 210, preview)).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!(app.bots["a"].status, Status::Working);
        assert_eq!(seqs(&app), [11]);
        let idle = legacy_bot_event(15, None, 210, preview);
        (HistoryTurn { app, events, messages, pages, idle }, tool_row)
    }

    impl HistoryTurn {
        async fn react_to_old_reply(&mut self) -> Value {
            let rev = self.idle["rev"].as_i64().unwrap();
            let mut reaction = entry_at_rev(final_reply_event(5, 100, "Saved reply"), rev);
            reaction["entry"]["updatedAt"] = 240.into();
            reaction["entry"]["data"]["reactions"] = json!(["👍"]);
            self.events.send(reaction.clone()).unwrap();
            read_events(&mut self.app, &mut self.messages, 1).await;
            self.idle["rev"] = (rev + 1).into();
            self.idle["bot"]["rev"] = (rev + 1).into();
            reaction["entry"].clone()
        }

        async fn idle(&mut self) {
            self.events.send(self.idle.clone()).unwrap();
            read_events(&mut self.app, &mut self.messages, 1).await;
        }

        async fn next_page(&mut self) -> HistoryRequest {
            let Ok(Some((request, send))) = tokio::time::timeout(Duration::from_secs(2), self.pages.recv()).await
            else {
                panic!("live Idle produced no history request and {} toasts", self.app.toasts.len());
            };
            (request, send)
        }

        async fn page(&mut self, before: i64, limit: i64) -> oneshot::Sender<(StatusCode, Value)> {
            let (request, send) = self.next_page().await;
            assert_eq!(request, json!({"botId": "a", "beforeSeq": before, "limit": limit}));
            send
        }

        async fn completion(&mut self) -> Msg {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let msg = self.messages.recv().await.unwrap();
                    if matches!(msg, Msg::CompletionHistory { .. }) {
                        break msg;
                    }
                    self.app.on_msg(msg);
                }
            })
            .await
            .expect("the completion lookup has a result")
        }

        async fn no_history_request(&mut self) {
            assert!(tokio::time::timeout(Duration::from_millis(100), self.pages.recv()).await.is_err());
        }

        async fn manual_page(&mut self, before: i64, rows: &[Value]) {
            self.app.tick();
            self.page(before, 200).await.send(history_page(rows, before, 200)).unwrap();
            self.read_manual_page().await;
        }

        async fn read_manual_page(&mut self) {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let msg = self.messages.recv().await.unwrap();
                    let history = matches!(msg, Msg::Reply(After::History(_, _), _));
                    self.app.on_msg(msg);
                    if history {
                        break;
                    }
                }
            })
            .await
            .expect("the manual history page arrives");
        }
    }

    async fn before_paging_reconnect() -> (HistoryTurn, UnboundedReceiver<StreamRequest>) {
        let (client, mut requests, pages) = stream_and_history_host("2.53.0").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);

        let initial = expect_stream(&mut requests, 0).await;
        let mut snapshot: Vec<_> = (11..=210).map(history_tool).collect();
        snapshot.push(legacy_bot_event(211, Some(200), 190, "You: First message"));
        send_counted_replay(&initial, 211, snapshot);
        read_events(&mut app, &mut messages, 203).await;
        assert_eq!(seqs(&app), (11..=210).collect::<Vec<_>>());
        let idle = legacy_bot_event(214, None, 220, "Finished reply");
        let mut turn = HistoryTurn { app, events: initial, messages, pages, idle };
        turn.app.selected = Some("a".into());
        turn.app.chat_page = true;
        turn.app.chat_top = true;
        (turn, requests)
    }

    async fn reconnect_paging(
        turn: HistoryTurn,
        requests: &mut UnboundedReceiver<StreamRequest>,
        rev: i64,
        snapshot: Vec<Value>,
    ) -> HistoryTurn {
        let HistoryTurn { mut app, events, mut messages, pages, idle } = turn;
        drop(events);
        let events = expect_stream(requests, 211).await;
        let count = snapshot.len() + 2;
        send_counted_replay(&events, rev, snapshot);
        read_events(&mut app, &mut messages, count).await;
        HistoryTurn { app, events, messages, pages, idle }
    }

    async fn paging_after_reconnect() -> HistoryTurn {
        let (turn, mut requests) = before_paging_reconnect().await;
        reconnect_paging(turn, &mut requests, 211, vec![]).await
    }

    fn draft_and_promotion() -> (Value, Value) {
        let mut draft = entry_at_rev(final_reply_event(10, 210, "Unfinished draft"), 10);
        draft["entry"]["data"]["final"] = false.into();
        let mut promoted = entry_at_rev(draft.clone(), 212);
        promoted["entry"]["data"] = json!({"final": true, "text": "Finished reply"});
        promoted["entry"]["updatedAt"] = 220.into();
        (draft, promoted)
    }

    #[tokio::test]
    async fn an_older_buffered_final_can_prove_completion_with_a_manual_page_pending() {
        for pending in [false, true] {
            let (mut turn, mut requests) = before_paging_reconnect().await;
            let (draft, promoted) = draft_and_promotion();
            let page = if pending {
                turn.app.tick();
                Some(turn.page(11, 200).await)
            } else {
                None
            };
            let reacted = react_to_reply(promoted.clone(), 215);
            let mut turn = reconnect_paging(turn, &mut requests, 215, vec![reacted.clone()]).await;
            turn.events.send(promoted).unwrap();
            read_events(&mut turn.app, &mut turn.messages, 1).await;
            assert!(!turn.app.entries["a"].contains_key(&10));
            if pending {
                assert_eq!(turn.app.pending_history["a"].updates[&10].rev, Some(215));
            }

            turn.app.selected = None;
            turn.idle().await;
            let history = served_completion_history(&mut turn, &stored_history([reacted["entry"].clone()], 211)).await;
            let toasts = turn.app.toasts.len();
            if let Some(page) = page {
                page.send(history_page(&[draft["entry"].clone()], 11, 200)).unwrap();
                turn.read_manual_page().await;
                assert_eq!(turn.app.entries["a"][&10].rev, Some(215));
                assert_eq!(turn.app.entries["a"][&10].data["reactions"], json!(["👍"]));
            }
            assert_eq!((toasts, history), (1, 0), "R212 delivered after replay R215 with a pending page: {pending}");
        }
    }

    #[tokio::test]
    async fn an_older_manual_page_can_prove_completion_after_the_newer_final_is_replayed() {
        for early in [true, false] {
            let (mut turn, mut requests) = before_paging_reconnect().await;
            let (_, promoted) = draft_and_promotion();
            let rows = [promoted["entry"].clone()];
            if early {
                turn.manual_page(11, &rows).await;
            }
            let page = if early {
                None
            } else {
                turn.app.tick();
                Some(turn.page(11, 200).await)
            };
            let reacted = react_to_reply(promoted, 215);
            let mut turn = reconnect_paging(turn, &mut requests, 215, vec![reacted.clone()]).await;
            if let Some(page) = page {
                page.send(history_page(&rows, 11, 200)).unwrap();
                turn.read_manual_page().await;
            }
            assert_eq!(turn.app.entries["a"][&10].rev, Some(215));
            assert_eq!(turn.app.entries["a"][&10].data["reactions"], json!(["👍"]));
            assert_eq!(turn.app.oldest("a"), Some(10));

            turn.app.selected = None;
            turn.idle().await;
            let history = served_completion_history(&mut turn, &stored_history([reacted["entry"].clone()], 211)).await;
            assert_eq!((turn.app.toasts.len(), history), (1, 0), "R212 page before replay R215: {early}");
        }
    }

    #[tokio::test]
    async fn a_stale_manual_page_keeps_the_live_final_observed_below_the_window() {
        let mut turn = paging_after_reconnect().await;
        turn.app.tick();
        let page = turn.page(11, 200).await;
        let (draft, promoted) = draft_and_promotion();
        let held = history_page(&[draft["entry"].clone()], 11, 200);

        turn.events.send(promoted.clone()).unwrap();
        let mut unrelated = entry_at_rev(entry_event(9), 213);
        unrelated["entry"]["data"]["removed"] = true.into();
        turn.events.send(unrelated).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 2).await;
        assert!(!turn.app.entries["a"].contains_key(&10), "the update is outside the loaded window");
        assert_eq!(turn.app.legacy_replies["a"][&10].reply.map(|reply| reply.rev), Some(212));

        page.send(held).unwrap();
        turn.read_manual_page().await;
        assert!(turn.app.pending_history.is_empty());
        assert_eq!(turn.app.oldest("a"), Some(10));
        assert!(!turn.app.entries["a"].contains_key(&9), "only rows on the page are merged");
        assert!(!turn.app.history_complete("a"), "one row still leaves older history to try");
        turn.manual_page(10, &[]).await;
        assert!(turn.app.history_complete("a"));

        let visible = &turn.app.entries["a"][&10];
        let merged = (visible.rev, visible.text().to_owned());
        turn.app.selected = None;
        let reacted = react_to_reply(promoted, 215);
        let rows = stored_history([reacted["entry"].clone()], 211);
        turn.idle().await;
        let history = served_completion_history(&mut turn, &rows).await;
        assert_eq!(
            (merged, turn.app.toasts.len(), history),
            ((Some(212), "Finished reply".to_owned()), 1, 0),
            "merged row, toasts and automatic history requests at Idle R214"
        );
    }

    #[tokio::test]
    async fn a_manual_page_arriving_first_applies_the_later_stream_promotion() {
        let mut turn = paging_after_reconnect().await;
        let (draft, promoted) = draft_and_promotion();
        turn.manual_page(11, &[draft["entry"].clone()]).await;
        assert_eq!(turn.app.entries["a"][&10].rev, Some(10));
        assert!(!turn.app.entries["a"][&10].is_final());
        turn.events.send(promoted).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 1).await;
        assert_eq!(turn.app.entries["a"][&10].rev, Some(212));
        assert!(turn.app.entries["a"][&10].is_final());
        turn.app.selected = None;
        turn.idle().await;
        turn.no_history_request().await;
        assert_eq!(turn.app.toasts.len(), 1);
    }

    #[tokio::test]
    async fn a_stale_manual_page_cannot_restore_a_withdrawn_reply() {
        let mut turn = paging_after_reconnect().await;
        let (_, promoted) = draft_and_promotion();
        turn.app.tick();
        let page = turn.page(11, 200).await;
        let held = history_page(&[promoted["entry"].clone()], 11, 200);
        let mut withdrawn = entry_at_rev(promoted.clone(), 213);
        withdrawn["entry"]["data"]["removed"] = true.into();
        for change in [promoted.clone(), withdrawn, promoted] {
            turn.events.send(change).unwrap();
        }
        read_events(&mut turn.app, &mut turn.messages, 3).await;
        assert!(turn.app.legacy_replies["a"][&10].reply.is_none(), "the withdrawal defeats the older stream duplicate");
        page.send(held).unwrap();
        turn.read_manual_page().await;
        assert_eq!(turn.app.entries["a"][&10].rev, Some(213));
        assert_eq!(turn.app.entries["a"][&10].data["removed"], true);
        assert!(turn.app.lane("a").iter().all(|entry| entry.seq != 10));
        assert_eq!(turn.app.oldest("a"), Some(10), "a hidden row still advances paging");
        turn.manual_page(10, &[]).await;

        turn.app.selected = None;
        turn.idle().await;
        let history = served_completion_history(&mut turn, &stored_history([], 211)).await;
        assert_eq!((turn.app.toasts.len(), history), (0, 1));
        assert!(!turn.app.legacy_replies.contains_key("a"));
    }

    #[tokio::test]
    async fn a_manual_page_uses_the_latest_raw_revision_without_losing_earlier_completion_evidence() {
        let mut turn = paging_after_reconnect().await;
        let (_, promoted) = draft_and_promotion();
        turn.app.tick();
        let page = turn.page(11, 200).await;
        let revision = entry_at_rev(promoted.clone(), 213);
        let reacted = react_to_reply(promoted.clone(), 215);
        turn.events.send(promoted).unwrap();
        turn.events.send(reacted.clone()).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 2).await;
        page.send(history_page(&[revision["entry"].clone()], 11, 200)).unwrap();
        turn.read_manual_page().await;

        let row = &turn.app.entries["a"][&10];
        assert_eq!(row.rev, Some(215));
        assert_eq!(row.data["reactions"], json!(["👍"]));
        assert_eq!(turn.app.legacy_replies["a"][&10].reply.map(|reply| reply.rev), Some(212));
        turn.app.selected = None;
        turn.idle().await;
        turn.no_history_request().await;
        assert_eq!(turn.app.toasts.len(), 1, "the observed final R212 proves completion at R214");
    }

    #[test]
    fn manual_paging_compares_the_page_with_a_row_already_in_the_mirror() {
        for (held_rev, page_rev) in [(12, 13), (13, 12)] {
            let mut app = test_app();
            app.on_event(&entry_event(11));
            let after = prepare_history(&mut app, "a");
            let reply = final_reply_event(10, 210, "Reply");
            app.on_event(&entry_at_rev(reply.clone(), held_rev));
            let row = entry_at_rev(reply, page_rev);
            app.on_reply(after, Ok(json!({"entries": [row["entry"]]})));
            assert_eq!(app.entries["a"][&10].rev, Some(13));
            assert!(app.pending_history.is_empty());
        }
    }

    #[test]
    fn a_manual_page_adds_evidence_to_the_newer_row_already_in_the_mirror() {
        let mut app = legacy_chat_window();
        let page = prepare_history(&mut app, "a");
        let (_, promoted) = draft_and_promotion();
        let reacted = react_to_reply(promoted.clone(), 215);
        app.on_event(&reacted);
        app.on_reply(page, Ok(json!({"entries": [promoted["entry"]]})));
        assert_eq!(app.legacy_replies["a"][&10].reply.map(|reply| reply.rev), Some(212));

        app.on_event(&promoted);
        assert_eq!(app.entries["a"][&10].rev, Some(215));
        assert_eq!(app.entries["a"][&10].data, reacted["entry"]["data"]);
        app.on_event(&legacy_bot_event(214, None, 220, "Finished reply"));
        assert_eq!(app.toasts.len(), 1);
    }

    #[test]
    fn an_older_final_cannot_restore_evidence_after_the_latest_row_changes() {
        for change in [
            json!({"id": "replacement"}),
            json!({"createdAt": 211}),
            json!({"data": {"final": false}}),
            json!({"data": {"final": true, "removed": true}}),
            json!({"data": {"final": true, "call": "voice"}}),
        ] {
            for (pending, from_page) in [(false, false), (true, false), (true, true)] {
                let mut app = legacy_chat_window();
                connect(&mut app, 210);
                let (draft, promoted) = draft_and_promotion();
                let page = pending.then(|| prepare_history(&mut app, "a"));
                let mut latest = react_to_reply(promoted.clone(), 215);
                for (key, value) in change.as_object().unwrap() {
                    latest["entry"][key] = value.clone();
                }
                app.on_event(&latest);
                if !from_page {
                    app.on_event(&promoted);
                }
                if let Some(page) = page {
                    let row = if from_page { &promoted } else { &draft };
                    app.on_reply(page, Ok(json!({"entries": [row["entry"]]})));
                    let row = &app.entries["a"][&10];
                    assert_eq!(row.rev, Some(215));
                    assert_eq!(row.data, latest["entry"]["data"]);
                    assert_eq!(row.id, latest["entry"]["id"]);
                    assert_eq!(row.created_at, latest["entry"]["createdAt"]);
                } else {
                    assert!(!app.entries["a"].contains_key(&10));
                }
                app.on_event(&legacy_bot_event(214, None, 220, "Finished reply"));
                assert!(app.toasts.is_empty(), "change {change}, pending page {pending}, from page {from_page}");
            }
        }
    }

    #[test]
    fn history_errors_and_deletions_discard_pending_updates_and_stale_replies() {
        let mut app = legacy_chat_window();
        connect(&mut app, 210);
        let (_, promoted) = draft_and_promotion();
        let failed = prepare_history(&mut app, "a");
        app.on_event(&promoted);
        assert!(app.pending_history["a"].updates.contains_key(&10));
        app.on_reply(failed, Err("Disconnected".into()));
        assert!(app.pending_history.is_empty());

        let deleted = prepare_history(&mut app, "a");
        assert!(app.pending_history["a"].updates.is_empty());
        app.on_event(&promoted);
        let deletion = json!({"type": "bot", "rev": 213, "bot": {"id": "a", "rev": 213, "deleted": true}});
        app.on_event(&deletion);
        assert!(app.pending_history.is_empty());
        let next = prepare_history(&mut app, "a");
        app.on_reply(deleted, Ok(json!({"entries": [promoted["entry"]]})));
        assert_eq!(seqs(&app), [] as [i64; 0]);
        assert!(app.pending_history["a"].updates.is_empty());
        app.on_reply(next, Ok(json!({"entries": []})));
        assert!(app.history_complete("a"));
        assert!(app.pending_history.is_empty());
    }

    async fn overlapping_legacy_turns(has_first_reply: bool, outside: bool) {
        let (client, mut requests, pages) = stream_and_history_host("2.53.0").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let initial = expect_stream(&mut requests, 0).await;

        let queued_seq = if has_first_reply { 3 } else { 2 };
        let late_seq = queued_seq + 2;
        let mut late_message = entry_at_rev(entry_event(late_seq), 17);
        late_message["entry"]["createdAt"] = 231.into();
        late_message["entry"]["data"] = json!({"text": "Another message", "status": "queued"});
        let events = if outside {
            initial.send(json!({"type": "hello", "rev": 8, "catchUp": 2})).unwrap();
            initial.send(json!({"type": "icons", "icons": {}})).unwrap();
            initial.send(legacy_bot_event(8, None, 0, "")).unwrap();
            read_events(&mut app, &mut messages, 3).await;

            // A confirmed send can set the floor for the next catch-up.
            app.sends.insert(
                "a".into(),
                PendingSend {
                    text: "Another message".into(),
                    files: vec![],
                    nonce: "late".into(),
                    task: None,
                    receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
                },
            );
            app.on_msg(Msg::Reply(
                After::Sent { key: "a".into(), nonce: "late".into(), epoch: app.mirror_epoch },
                Ok(json!({"entry": late_message["entry"]})),
            ));
            assert_eq!(seqs(&app), [late_seq]);
            drop(initial);
            expect_stream(&mut requests, 8).await
        } else {
            initial
        };

        let mut first_message = entry_at_rev(entry_event(1), 9);
        first_message["entry"]["createdAt"] = 190.into();
        first_message["entry"]["data"] = json!({"text": "First message", "status": "sent"});
        let mut first_reply = entry_at_rev(final_reply_event(2, 210, "First reply"), 11);
        first_reply["entry"]["turn"] = 1.into();
        let mut queued = entry_at_rev(entry_event(queued_seq), 12);
        queued["entry"]["createdAt"] = 210.into();
        queued["entry"]["turn"] = 2.into();
        queued["entry"]["data"] = json!({"text": "Queued message", "status": "queued"});
        let mut sent = entry_at_rev(queued.clone(), 14);
        sent["entry"]["updatedAt"] = 220.into();
        sent["entry"]["data"]["status"] = "sent".into();
        let mut next_reply = entry_at_rev(final_reply_event(queued_seq + 1, 230, "Next reply"), 16);
        next_reply["entry"]["turn"] = 2.into();

        let mut replay = vec![
            json!({"type": "icons", "icons": {}}),
            first_message.clone(),
            legacy_bot_event(10, Some(200), 190, "You: First message"),
        ];
        if has_first_reply {
            replay.push(first_reply.clone());
        }
        replay.extend([sent.clone(), next_reply.clone()]);
        let count = replay.len();
        let hello_rev = if outside { 17 } else { 16 };
        events.send(json!({"type": "hello", "rev": hello_rev, "catchUp": count})).unwrap();
        for event in replay {
            events.send(event).unwrap();
        }
        read_events(&mut app, &mut messages, count + 1).await;
        let window = if outside { vec![late_seq] } else { (1..late_seq).collect() };
        assert_eq!(seqs(&app), window);
        assert_eq!(app.bots["a"].rev, Some(10));
        assert!(app.toasts.is_empty());
        let idle = legacy_bot_event(13, None, 210, "You: Queued message");
        let mut turn = HistoryTurn { app, events, messages, pages, idle };

        if has_first_reply {
            turn.events.send(first_reply.clone()).unwrap();
        }
        turn.events.send(queued).unwrap();
        turn.events.send(turn.idle.clone()).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 2 + usize::from(has_first_reply)).await;
        assert_eq!(turn.app.bots["a"].rev, Some(13), "buffered Idle is live after the counted replay");
        let first_lookup = turn.app.completion_lookups.contains_key("a");
        if first_lookup {
            let mut rows = vec![first_message["entry"].clone()];
            if has_first_reply {
                rows.push(first_reply["entry"].clone());
            }
            rows.extend([sent["entry"].clone(), next_reply["entry"].clone()]);
            if outside {
                rows.push(late_message["entry"].clone());
            }
            turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
            let completion = turn.completion().await;
            turn.app.on_msg(completion);
        }
        let first_toasts = turn.app.toasts.len();
        turn.app.toasts.clear();
        turn.no_history_request().await;
        assert_eq!(seqs(&turn.app), window);
        if outside {
            let saved = &turn.app.legacy_replies["a"];
            let carried: Vec<_> = saved.iter().filter_map(|(seq, observed)| observed.reply.map(|_| *seq)).collect();
            assert_eq!(carried, [queued_seq + 1], "only the reply beyond A's Idle carries over");
        }

        let idle_b = if outside {
            legacy_bot_event(18, None, 231, "You: Another message")
        } else {
            legacy_bot_event(17, None, 230, "Next reply")
        };
        turn.events.send(sent).unwrap();
        turn.events.send(legacy_bot_event(15, Some(220), 220, "You: Queued message")).unwrap();
        turn.events.send(next_reply).unwrap();
        if outside {
            turn.events.send(late_message).unwrap();
        }
        turn.events.send(idle_b.clone()).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 4 + usize::from(outside)).await;
        let second_toasts = turn.app.toasts.len();
        turn.no_history_request().await;
        assert_eq!(
            [first_toasts, second_toasts],
            [usize::from(has_first_reply), 1],
            "A history lookup: {first_lookup}, replies below the window: {outside}"
        );
        assert_eq!(first_lookup, !has_first_reply);
        assert_eq!(turn.app.toasts[0].text, "Alpha is done");
        assert_eq!(seqs(&turn.app), window);
        turn.app.toasts.clear();
        turn.events.send(idle_b).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 1).await;
        turn.no_history_request().await;
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn counted_legacy_replay_does_not_complete_the_previous_turn_with_the_next_reply() {
        overlapping_legacy_turns(false, false).await;
    }

    #[tokio::test]
    async fn counted_legacy_replay_still_completes_both_turns_with_saved_replies() {
        overlapping_legacy_turns(true, false).await;
    }

    #[tokio::test]
    async fn counted_legacy_replay_checks_the_idle_revision_below_the_window() {
        overlapping_legacy_turns(false, true).await;
        overlapping_legacy_turns(true, true).await;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ReplyUpdate {
        Reaction,
        UnseenReaction,
        Withdrawal,
    }

    fn send_counted_replay(events: &UnboundedSender<Value>, rev: i64, replay: Vec<Value>) {
        let count = replay.len() + 1;
        events.send(json!({"type": "hello", "hostId": "observed-reply", "rev": rev, "catchUp": count})).unwrap();
        events.send(json!({"type": "icons", "icons": {}})).unwrap();
        for event in replay {
            events.send(event).unwrap();
        }
    }

    fn react_to_reply(event: Value, rev: i64) -> Value {
        let mut event = entry_at_rev(event, rev);
        event["entry"]["updatedAt"] = 240.into();
        event["entry"]["data"]["reactions"] = json!(["👍"]);
        event
    }

    async fn served_completion_history(turn: &mut HistoryTurn, rows: &[Value]) -> usize {
        let requested = turn.app.completion_lookups.contains_key("a");
        if requested {
            turn.page(i64::MAX, 500).await.send(history_page(rows, i64::MAX, 500)).unwrap();
            let completion = turn.completion().await;
            turn.app.on_msg(completion);
        }
        turn.no_history_request().await;
        usize::from(requested)
    }

    async fn reply_updated_on_reconnect(update: ReplyUpdate) {
        let (client, mut requests, pages) = stream_and_history_host("2.53.0").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let saw_original = update != ReplyUpdate::UnseenReaction;

        let mut user = entry_at_rev(entry_event(1), 7);
        user["entry"]["createdAt"] = 190.into();
        user["entry"]["data"] = json!({"text": "First message", "status": "sent"});
        let mut reply = entry_at_rev(final_reply_event(2, 210, "Saved reply"), 9);
        reply["entry"]["turn"] = 1.into();
        let initial = expect_stream(&mut requests, 0).await;
        send_counted_replay(&initial, 8, vec![user.clone(), legacy_bot_event(8, Some(200), 190, "You: First message")]);
        if saw_original {
            initial.send(reply.clone()).unwrap();
        }
        read_events(&mut app, &mut messages, 4 + usize::from(saw_original)).await;
        assert_eq!(app.entries["a"].get(&2).and_then(|entry| entry.rev), saw_original.then_some(9));
        assert_eq!(app.bots["a"].status, Status::Working);
        assert!(app.toasts.is_empty());
        drop(initial);

        let latest = if update == ReplyUpdate::Withdrawal {
            let mut latest = entry_at_rev(reply, 11);
            latest["entry"]["updatedAt"] = 240.into();
            latest["entry"]["data"]["removed"] = true.into();
            latest
        } else {
            react_to_reply(reply, 11)
        };
        let events = expect_stream(&mut requests, if saw_original { 9 } else { 8 }).await;
        send_counted_replay(&events, 11, vec![latest.clone()]);
        read_events(&mut app, &mut messages, 3).await;
        assert_eq!(app.entries["a"][&2].rev, Some(11));
        assert_eq!(app.entries["a"][&2].data, latest["entry"]["data"]);
        assert!(app.toasts.is_empty(), "the rewrite is replayed while the buffered Idle waits");

        let idle = legacy_bot_event(10, None, 210, "Saved reply");
        let mut turn = HistoryTurn { app, events, messages, pages, idle };
        turn.idle().await;
        assert_eq!(turn.app.bots["a"].rev, Some(10));
        let mut rows = vec![user["entry"].clone()];
        if update != ReplyUpdate::Withdrawal {
            rows.push(latest["entry"].clone());
        }
        let history = served_completion_history(&mut turn, &rows).await;
        let expected = usize::from(update == ReplyUpdate::Reaction);
        assert_eq!(
            (turn.app.toasts.len(), history),
            (expected, 1 - expected),
            "toasts and history requests at Idle R10"
        );
        if expected > 0 {
            assert_eq!(turn.app.toasts[0].text, "Alpha is done");
        }
        turn.app.toasts.clear();
        turn.events.send(latest).unwrap();
        turn.events.send(turn.idle.clone()).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 2).await;
        turn.no_history_request().await;
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn a_legacy_completion_uses_the_reply_seen_before_a_reconnect_reaction() {
        reply_updated_on_reconnect(ReplyUpdate::Reaction).await;
    }

    #[tokio::test]
    async fn a_legacy_completion_stays_quiet_when_reconnect_only_delivers_the_newer_reply() {
        reply_updated_on_reconnect(ReplyUpdate::UnseenReaction).await;
    }

    #[tokio::test]
    async fn a_legacy_completion_stays_quiet_when_reconnect_replays_a_withdrawal() {
        reply_updated_on_reconnect(ReplyUpdate::Withdrawal).await;
    }

    #[tokio::test]
    async fn an_observed_reply_below_the_window_survives_a_reconnect_reaction() {
        for saw_earlier in [true, false] {
            let (client, mut requests, pages) = stream_and_history_host("2.53.0").await;
            let (tx, mut messages) = unbounded_channel();
            let mut app = App::new(client.clone(), tx.clone(), String::new());
            client.spawn_stream(tx);
            let initial = expect_stream(&mut requests, 0).await;
            let mut snapshot: Vec<_> = (11..=210).map(history_tool).collect();
            snapshot.push(legacy_bot_event(211, Some(200), 210, "Saved reply"));
            send_counted_replay(&initial, 211, snapshot);
            read_events(&mut app, &mut messages, 203).await;
            drop(initial);

            let reply = final_reply_event(10, 210, "Saved reply");
            let window: Vec<_> = (11..=210).collect();
            if saw_earlier {
                let mid = expect_stream(&mut requests, 211).await;
                send_counted_replay(&mid, 212, vec![react_to_reply(reply.clone(), 212)]);
                read_events(&mut app, &mut messages, 3).await;
                assert_eq!(seqs(&app), window);
                assert!(app.toasts.is_empty());
                drop(mid);
            }

            let mut latest = react_to_reply(reply, 214);
            latest["entry"]["data"]["reactions"] = json!(["👍", "🎉"]);
            let events = expect_stream(&mut requests, if saw_earlier { 212 } else { 211 }).await;
            send_counted_replay(&events, 214, vec![latest.clone()]);
            read_events(&mut app, &mut messages, 3).await;
            assert_eq!(seqs(&app), window);
            let idle = legacy_bot_event(213, None, 210, "Saved reply");
            let mut turn = HistoryTurn { app, events, messages, pages, idle };
            turn.idle().await;
            let rows = stored_history([latest["entry"].clone()], 211);
            let history = served_completion_history(&mut turn, &rows).await;
            assert_eq!((turn.app.toasts.len(), history), (usize::from(saw_earlier), usize::from(!saw_earlier)));
            assert_eq!(seqs(&turn.app), window);
        }
    }

    #[test]
    fn repeated_reactions_keep_the_first_observed_legacy_final() {
        for seq in [10, 211] {
            let mut app = legacy_chat_window();
            connect(&mut app, 210);
            let original = entry_at_rev(final_reply_event(seq, 210, "Saved reply"), 211);
            app.on_event(&original);
            app.on_event(&react_to_reply(original.clone(), 213));
            let mut latest = react_to_reply(original, 214);
            latest["entry"]["data"]["reactions"] = json!(["👍", "🎉"]);
            app.on_event(&latest);
            app.on_event(&legacy_bot_event(212, None, 210, "Saved reply"));
            assert_eq!(app.toasts.len(), 1, "sequence {seq}");
            if seq == 211 {
                assert_eq!(app.entries["a"][&seq].rev, Some(214));
                assert_eq!(app.entries["a"][&seq].data, latest["entry"]["data"]);
            }
        }
    }

    #[test]
    fn changes_to_a_legacy_reply_discard_earlier_completion_evidence() {
        for seq in [10, 211] {
            for change in [
                json!({"id": "replacement"}),
                json!({"createdAt": 211}),
                json!({"data": {"final": false}}),
                json!({"data": {"final": true, "removed": true}}),
                json!({"data": {"final": true, "call": "voice"}}),
            ] {
                let mut app = legacy_chat_window();
                connect(&mut app, 210);
                let original = entry_at_rev(final_reply_event(seq, 210, "Saved reply"), 211);
                app.on_event(&original);
                let latest = react_to_reply(original, 213);
                app.on_event(&latest);
                let mut replacement = entry_at_rev(latest.clone(), 214);
                for (key, value) in change.as_object().unwrap() {
                    replacement["entry"][key] = value.clone();
                }
                app.on_event(&replacement);
                if change["data"].is_object() {
                    app.on_event(&entry_at_rev(latest, 215));
                }
                app.on_event(&legacy_bot_event(212, None, 211, "Saved reply"));
                assert!(app.toasts.is_empty(), "sequence {seq}, change {change}");
            }
        }
    }

    #[test]
    fn manual_history_replacing_a_retained_reply_discards_its_evidence() {
        let mut app = legacy_chat_window();
        connect(&mut app, 210);
        let reply = entry_at_rev(final_reply_event(10, 210, "Saved reply"), 211);
        app.on_event(&reply);
        let mut replacement = react_to_reply(reply, 213);
        replacement["entry"]["id"] = "replacement".into();
        apply_history_row(&mut app, &replacement["entry"]);
        assert_eq!(app.entries["a"][&10].id, "replacement");
        app.on_event(&legacy_bot_event(212, None, 210, "Saved reply"));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn a_mirror_rewind_discards_the_earlier_reply_observation() {
        let mut app = legacy_chat_window();
        let reply = entry_at_rev(final_reply_event(211, 210, "Saved reply"), 211);
        app.on_event(&reply);
        let latest = react_to_reply(reply, 213);
        app.on_event(&latest);
        assert!(app.legacy_replies["a"].contains_key(&211));
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        app.on_msg(Msg::ReplayComplete);
        app.on_event(&legacy_bot_event(9, Some(200), 100, "Old reply"));
        app.on_event(&latest);
        app.on_event(&legacy_bot_event(212, None, 210, "Saved reply"));
        assert!(app.toasts.is_empty());
    }

    #[tokio::test]
    async fn a_legacy_completion_finds_the_saved_tail_after_truncated_initial_replay() {
        let (mut turn, tool) = truncated_history_turn("Saved reply").await;
        let mut saved = final_reply_event(12, 210, "Saved reply")["entry"].clone();
        saved["turn"] = 1.into();
        let rows = [tool, saved];

        turn.idle().await;
        assert_eq!(turn.app.bots["a"].rev, Some(15));
        assert!(turn.app.toasts.is_empty());
        let (request, send) = turn.next_page().await;
        let before = request["beforeSeq"].as_i64().unwrap();
        let limit = usize::try_from(request["limit"].as_u64().unwrap()).unwrap();
        send.send(history_page(&rows, before, limit)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1, "history cursor {before} after reconnecting with since=13");
        assert_eq!(turn.app.toasts[0].text, "Alpha is done");
        assert_eq!(request, json!({"botId": "a", "beforeSeq": i64::MAX, "limit": 500}));
        assert_eq!(seqs(&turn.app), [11], "the lookup leaves the transcript alone");
    }

    #[tokio::test]
    async fn legacy_completion_history_ignores_a_later_turn_before_its_working_event() {
        for saved_reply in [false, true] {
            let preview = if saved_reply { "Saved reply" } else { "You: Queued message" };
            let (mut turn, tool) = truncated_history_turn(preview).await;
            turn.idle().await;
            let page = turn.page(i64::MAX, 500).await;

            let mut rows = vec![tool];
            if saved_reply {
                let mut saved = final_reply_event(12, 210, "Saved reply")["entry"].clone();
                saved["turn"] = 1.into();
                rows.push(saved);
            }
            // The user message is now sent and the next Working at rev 18 is still buffered in SSE.
            let queued_seq = if saved_reply { 13 } else { 12 };
            let mut sent = entry_event(queued_seq)["entry"].clone();
            sent["rev"] = 17.into();
            sent["turn"] = 2.into();
            sent["createdAt"] = 210.into();
            sent["updatedAt"] = 210.into();
            sent["data"] = json!({"text": "Queued message", "status": "sent"});
            let mut later = final_reply_event(queued_seq + 1, 210, "Saved reply")["entry"].clone();
            later["rev"] = 19.into();
            later["turn"] = 2.into();
            rows.extend([sent, later]);
            page.send(history_page(&rows, i64::MAX, 500)).unwrap();
            let completion = turn.completion().await;
            assert_eq!(turn.app.bots["a"].status, Status::Idle);
            assert_eq!(turn.app.bots["a"].rev, Some(15));
            turn.app.on_msg(completion);
            assert_eq!(turn.app.toasts.len(), usize::from(saved_reply));
            assert_eq!(seqs(&turn.app), [11]);
        }
    }

    #[tokio::test]
    async fn legacy_completion_history_requires_positive_host_revisions() {
        let mut turn = history_turn(11, 210).await;
        turn.idle["bot"].as_object_mut().unwrap().remove("rev");
        turn.idle().await;
        turn.no_history_request().await;
        assert!(turn.app.toasts.is_empty());

        let mut turn = history_turn(11, 210).await;
        turn.idle().await;
        let mut saved = final_reply_event(10, 210, "Saved reply")["entry"].clone();
        saved["rev"] = 0.into();
        let rows = stored_history([saved], 211);
        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn a_legacy_completion_finds_a_saved_reply_in_a_sparse_initial_window() {
        let mut turn = history_turn(11, 210).await;
        let old_reply = turn.react_to_old_reply().await;
        let window: Vec<_> = std::iter::once(5).chain(11..=210).collect();
        assert_eq!(seqs(&turn.app), window);
        assert_eq!(turn.app.bots["a"].status, Status::Working);

        turn.idle().await;
        assert!(turn.app.toasts.is_empty(), "the same preview text and newer reaction prove nothing");
        let saved = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([old_reply, saved["entry"].clone()], 211);
        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1);
        assert_eq!(turn.app.toasts[0].text, "Alpha is done");
        assert_eq!(seqs(&turn.app), window, "the reaction and the tail still bracket the same gap");
        turn.app.toasts.clear();
        turn.idle().await;
        turn.no_history_request().await;
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn completed_manual_paging_below_a_gap_does_not_skip_the_completion_lookup() {
        let mut turn = history_turn(11, 210).await;
        let old_reply = turn.react_to_old_reply().await;
        let mut earlier = entry_event(2);
        earlier["entry"]["rev"] = 2.into();
        earlier["entry"]["createdAt"] = 50.into();
        let saved = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([earlier["entry"].clone(), old_reply, saved["entry"].clone()], 211);
        turn.app.selected = Some("a".into());
        turn.app.chat_page = true;
        turn.app.chat_top = true;
        turn.manual_page(5, &rows).await;
        assert_eq!(turn.app.oldest("a"), Some(2));
        turn.manual_page(2, &rows).await;
        assert!(turn.app.history_complete("a"));
        let window = seqs(&turn.app);
        assert!(!window.contains(&10));
        turn.app.selected = None;
        turn.idle().await;

        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1);
        assert_eq!(seqs(&turn.app), window);
        assert!(turn.app.history_complete("a"), "the lookup leaves the manual paging state intact");
    }

    #[tokio::test]
    async fn sparse_history_with_only_the_old_reaction_stays_quiet() {
        let mut turn = history_turn(11, 100).await;
        let old_reply = turn.react_to_old_reply().await;
        turn.idle["bot"]["lastAt"] = 240.into();
        turn.idle().await;
        let rows = stored_history([old_reply], 211);
        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert!(turn.app.toasts.is_empty());
        turn.no_history_request().await;
    }

    #[tokio::test]
    async fn a_legacy_completion_looks_up_a_reply_omitted_from_the_initial_window() {
        let mut turn = history_turn(11, 210).await;
        turn.idle().await;
        assert!(turn.app.toasts.is_empty(), "the preview by itself cannot prove a reply");
        let saved = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([saved["entry"].clone()], 211);
        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1);
        assert_eq!(turn.app.toasts[0].text, "Alpha is done");
        assert_eq!(seqs(&turn.app), (11..=210).collect::<Vec<_>>(), "the lookup leaves the window intact");
        turn.app.toasts.clear();
        turn.idle().await;
        turn.no_history_request().await;
        assert!(turn.app.toasts.is_empty(), "duplicate Idle stays quiet");

        turn.app.selected = Some("a".into());
        turn.app.chat_page = true;
        turn.app.chat_top = true;
        turn.manual_page(11, &rows).await;
        assert_eq!(turn.app.oldest("a"), Some(10));
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn a_legacy_completion_pages_past_a_full_page_of_tools() {
        let mut turn = history_turn(511, 210).await;
        let old_reply = turn.react_to_old_reply().await;
        turn.idle().await;
        let first = turn.page(i64::MAX, 500).await;
        turn.no_history_request().await;
        let reply = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([old_reply, reply["entry"].clone()], 711);
        first.send(history_page(&rows, i64::MAX, 500)).unwrap();
        turn.page(211, 500).await.send(history_page(&rows, 211, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1);
        assert_eq!(seqs(&turn.app), std::iter::once(5).chain(511..711).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn legacy_completion_history_with_an_old_reaction_or_api_failure_stays_quiet() {
        let mut reply = final_reply_event(10, 100, "Saved reply");
        reply["entry"]["updatedAt"] = 240.into();
        reply["entry"]["data"]["reactions"] = json!(["👍"]);
        let rows = stored_history([reply["entry"].clone()], 211);
        for response in [
            history_page(&rows, i64::MAX, 500),
            (StatusCode::SERVICE_UNAVAILABLE, json!({"error": "fixture history unavailable"})),
        ] {
            let mut turn = history_turn(11, 240).await;
            turn.idle().await;
            turn.page(i64::MAX, 500).await.send(response).unwrap();
            let completion = turn.completion().await;
            turn.app.on_msg(completion);
            assert!(turn.app.toasts.is_empty());
            assert!(turn.app.hint.is_none());
            turn.events.send(legacy_bot_event(213, Some(200), 240, "Saved reply")).unwrap();
            read_events(&mut turn.app, &mut turn.messages, 1).await;
            turn.idle["rev"] = 214.into();
            turn.idle["bot"]["rev"] = 214.into();
            turn.idle().await;
            turn.no_history_request().await;
            assert!(turn.app.toasts.is_empty(), "the same turn cannot restart its lookup");
        }
    }

    #[tokio::test]
    async fn late_legacy_completion_history_cannot_toast_after_the_turn_changes() {
        let reply = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([reply["entry"].clone()], 211);
        for replacement in [
            Msg::Event(legacy_bot_event(213, Some(300), 210, "Saved reply")),
            Msg::Event(json!({"type": "bot", "rev": 213, "bot": {"id": "a", "deleted": true}})),
            Msg::Rewind(SendReceipts::default()),
        ] {
            let mut turn = history_turn(11, 210).await;
            turn.idle().await;
            turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
            let completion = turn.completion().await;
            turn.app.on_msg(replacement);
            if turn.app.bots.get("a").is_some_and(|bot| bot.status == Status::Working) {
                turn.app.on_msg(Msg::Event(legacy_bot_event(214, None, 210, "Saved reply")));
            }
            turn.app.on_msg(completion);
            assert!(turn.app.toasts.is_empty());
        }
    }

    #[tokio::test]
    async fn a_legacy_history_completion_stays_quiet_for_a_focused_chat() {
        let mut watching = history_turn(11, 210).await;
        watching.app.selected = Some("a".into());
        watching.app.chat_page = true;
        watching.idle().await;
        watching.no_history_request().await;
        assert!(watching.app.toasts.is_empty());

        let mut turn = history_turn(11, 210).await;
        turn.idle().await;
        let reply = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([reply["entry"].clone()], 211);
        turn.page(i64::MAX, 500).await.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.selected = Some("a".into());
        turn.app.chat_page = true;
        assert!(turn.app.watching_chat("a"));
        turn.app.on_msg(completion);
        assert!(turn.app.toasts.is_empty());
    }

    #[tokio::test]
    async fn slow_legacy_completion_history_still_toasts_for_the_same_turn() {
        let mut turn = history_turn(11, 210).await;
        let old_reply = turn.react_to_old_reply().await;
        turn.idle().await;
        let page = turn.page(i64::MAX, 500).await;
        tokio::time::sleep(super::super::TOAST_DURATION + Duration::from_millis(200)).await;
        assert!(turn.app.toasts.is_empty());
        let reply = final_reply_event(10, 210, "Saved reply");
        let rows = stored_history([old_reply, reply["entry"].clone()], 211);
        page.send(history_page(&rows, i64::MAX, 500)).unwrap();
        let completion = turn.completion().await;
        turn.app.on_msg(completion);
        assert_eq!(turn.app.toasts.len(), 1);
    }

    #[tokio::test]
    async fn local_and_exact_completion_evidence_do_not_query_history() {
        let mut turn = history_turn(11, 210).await;
        turn.events.send(final_reply_event(211, 210, "Saved reply")).unwrap();
        read_events(&mut turn.app, &mut turn.messages, 1).await;
        turn.idle().await;
        assert_eq!(turn.app.toasts.len(), 1);
        turn.no_history_request().await;

        for reply_id in [None, Some("server-reply")] {
            let mut turn = history_turn(11, 210).await;
            turn.idle["bot"]["completionReplyId"] = json!(reply_id);
            turn.idle().await;
            assert_eq!(turn.app.toasts.len(), usize::from(reply_id.is_some()));
            turn.no_history_request().await;
        }
    }

    async fn legacy_reply_replayed_before_working(
        previous: Option<Value>,
        created_at: i64,
        replay_rev: i64,
    ) -> Vec<String> {
        let (client, mut requests) = stream_host().await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        if let Some(bot) = previous {
            app.on_msg(Msg::Event(bot));
        }
        let prior = app.bots.get("a").map(|bot| (bot.status, bot.started_at));
        client.spawn_stream(tx);

        let hello = |rev| json!({"type": "hello", "hostId": "legacy-window", "rev": rev});
        let original = expect_stream(&mut requests, 0).await;
        original.send(hello(211)).unwrap();
        for seq in 11..=210 {
            let mut tool = entry_event(seq);
            tool["rev"] = seq.into();
            tool["entry"]["kind"] = "tool".into();
            original.send(tool).unwrap();
        }
        read_events(&mut app, &mut messages, 201).await;
        let window = (11..=210).collect::<Vec<_>>();
        assert_eq!(seqs(&app), window);
        assert_eq!(app.bots.get("a").map(|bot| (bot.status, bot.started_at)), prior);
        drop(original);

        let resumed = expect_stream(&mut requests, 211).await;
        resumed.send(hello(replay_rev)).unwrap();
        let mut reply = entry_at_rev(final_reply_event(10, created_at, "Same reply"), 212);
        reply["entry"]["updatedAt"] = 240.into();
        reply["entry"]["data"]["reactions"] = json!(["👍"]);
        resumed.send(reply.clone()).unwrap();
        resumed.send(legacy_bot_event(213, Some(200), 240, "Same reply")).unwrap();
        read_events(&mut app, &mut messages, 3).await;
        assert!(app.toasts.is_empty(), "the replayed final and Working stay quiet");
        assert_eq!(seqs(&app), window);

        let idle = legacy_bot_event(214, None, 240, "Same reply");
        resumed.send(idle.clone()).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        let notices = app.toasts.drain(..).map(|toast| toast.text).collect();
        resumed.send(reply.clone()).unwrap();
        resumed.send(idle).unwrap();
        resumed.send(legacy_bot_event(215, None, 240, "Same reply")).unwrap();
        read_events(&mut app, &mut messages, 3).await;
        assert!(app.toasts.is_empty(), "duplicates and later idle snapshots stay quiet");
        assert_eq!(seqs(&app), window);
        apply_history_row(&mut app, &reply["entry"]);
        assert_eq!(seqs(&app), (10..=210).collect::<Vec<_>>(), "history loads the saved row");
        assert!(app.toasts.is_empty(), "paging is quiet");
        notices
    }

    #[tokio::test]
    async fn a_legacy_reply_replayed_before_working_toasts_on_live_idle() {
        for previous in [
            None,
            Some(legacy_bot_event(8, None, 100, "Earlier reply")),
            Some(legacy_bot_event(8, Some(50), 100, "Earlier reply")),
        ] {
            let notices = legacy_reply_replayed_before_working(previous, 210, 213).await;
            assert_eq!(notices, ["Alpha is done"]);
        }
    }

    #[tokio::test]
    async fn an_old_legacy_reply_replayed_before_working_stays_quiet() {
        for previous in [
            None,
            Some(legacy_bot_event(8, None, 100, "Earlier reply")),
            Some(legacy_bot_event(8, Some(50), 100, "Earlier reply")),
        ] {
            let notices = legacy_reply_replayed_before_working(previous, 100, 213).await;
            assert!(notices.is_empty(), "an old reaction with an updated lastAt stays quiet");
        }
    }

    #[tokio::test]
    async fn a_legacy_completion_wholly_in_uncounted_replay_stays_quiet() {
        assert_eq!(legacy_reply_replayed_before_working(None, 210, 214).await, Vec::<String>::new());
    }

    async fn finish_recovery(app: &mut App, messages: &mut UnboundedReceiver<Msg>) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let next = messages.recv().await.unwrap();
                let completed = matches!(next, Msg::ReplayComplete);
                app.on_msg(next);
                if completed {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    struct RestoredStream {
        app: App,
        requests: UnboundedReceiver<StreamRequest>,
        messages: UnboundedReceiver<Msg>,
        events: UnboundedSender<Value>,
    }

    fn restore_hello(rev: i64, count: u64) -> Value {
        json!({"type": "hello", "hostId": "host", "rev": rev, "catchUp": count})
    }

    fn restored_bot(id: &str, name: &str, rev: i64) -> Value {
        json!({"type": "bot", "rev": rev, "bot": {"id": id, "name": name, "rev": rev}})
    }

    async fn confirmed_restore_with(
        rev: i64,
        snapshots: Vec<Value>,
    ) -> (RestoredStream, UnboundedReceiver<HistoryRequest>) {
        let (client, mut requests, pages) = stream_and_history_host("2.54.0").await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let original = expect_stream(&mut requests, 0).await;
        let before = 1000;
        original.send(restore_hello(before, 0)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        drop(original);
        let rewind = expect_stream(&mut requests, before).await;
        rewind.send(restore_hello(rev, 0)).unwrap();
        let events = expect_stream(&mut requests, 0).await;
        events.send(restore_hello(rev, snapshots.len().try_into().unwrap())).unwrap();
        for event in snapshots {
            events.send(event).unwrap();
        }
        finish_recovery(&mut app, &mut messages).await;
        (RestoredStream { app, requests, messages, events }, pages)
    }

    async fn confirmed_restore() -> RestoredStream {
        confirmed_restore_with(
            26,
            vec![restored_bot("a", "Alpha", 20), restored_bot("b", "Beta", 21), entry_at_rev(entry_event(2), 22)],
        )
        .await
        .0
    }

    async fn receipt_replay_fixture() -> (RestoredStream, UnboundedReceiver<HistoryRequest>) {
        confirmed_restore_with(301, vec![restored_bot("a", "Alpha", 20), entry_at_rev(entry_event(300), 301)]).await
    }

    fn pending_receipt_send(app: &mut App) -> After {
        let text = "Send receipt";
        let nonce = "send-301";
        let files = vec![std::path::PathBuf::from("receipt.txt")];
        app.drafts.insert("a".into(), Editor::with(text));
        app.files.insert("a".into(), files.clone());
        app.sends.insert(
            "a".into(),
            PendingSend {
                text: text.into(),
                files,
                nonce: nonce.into(),
                task: None,
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        After::Sent { key: "a".into(), nonce: nonce.into(), epoch: app.mirror_epoch }
    }

    fn send_receipt_entry(rev: i64) -> Value {
        let mut entry = entry_at_rev(entry_event(301), rev);
        entry["entry"]["id"] = "rpc-entry".into();
        entry["entry"]["data"] = json!({"text": "Send receipt", "clientNonce": "send-301"});
        entry
    }

    async fn receipt_page(pages: &mut UnboundedReceiver<HistoryRequest>) -> oneshot::Sender<(StatusCode, Value)> {
        let (request, reply) = tokio::time::timeout(Duration::from_secs(2), pages.recv()).await.unwrap().unwrap();
        assert_eq!(request, json!({"botId": "a", "beforeSeq": 302, "limit": 1}));
        reply
    }

    async fn read_receipt_check(app: &mut App, messages: &mut UnboundedReceiver<Msg>) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let msg = messages.recv().await.unwrap();
                let checked = matches!(msg, Msg::Reply(After::CheckSendReceipt { .. }, _));
                app.on_msg(msg);
                if checked {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_late_receipt_missing_from_current_chat_keeps_the_same_draft() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            receipt_replay_fixture().await;
        let after = pending_receipt_send(&mut app);
        drop(events);
        let resumed = expect_stream(&mut requests, 0).await;
        replay_through_300(&resumed);
        finish_recovery(&mut app, &mut messages).await;

        let manual = prepare_history(&mut app, "a");
        app.on_msg(Msg::Reply(after, Ok(json!({"entry": send_receipt_entry(305)["entry"]}))));
        assert_eq!(seqs(&app), [300], "the unverified receipt is hidden");
        let reply = receipt_page(&mut pages).await;
        reply.send((StatusCode::OK, json!({"entries": [entry_at_rev(entry_event(300), 301)["entry"]]}))).unwrap();
        read_receipt_check(&mut app, &mut messages).await;
        assert_eq!(seqs(&app), [300]);
        assert_eq!(app.drafts["a"].text, "Send receipt");
        assert_eq!(app.files["a"], app.sends["a"].files);
        assert_eq!(app.sends["a"].nonce, "send-301");
        assert!(!app.sends["a"].is_busy());
        assert!(app.hint.as_ref().is_some_and(|(text, _)| text.contains("Draft kept")));
        assert!(matches!(manual, After::History(_, id) if id == app.pending_history["a"].request));
    }

    #[tokio::test]
    async fn queued_receipt_from_send_waits_for_the_connection_that_already_reopened() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            receipt_replay_fixture().await;
        let after = pending_receipt_send(&mut app);
        let queued = Msg::Reply(after, Ok(json!({"entry": send_receipt_entry(305)["entry"]})));
        drop(events);
        let resumed = expect_stream(&mut requests, 0).await;

        app.on_msg(queued);
        assert_eq!(seqs(&app), [300]);
        assert!(app.hint.as_ref().is_none_or(|(text, _)| text != "Sent"));
        replay_through_300(&resumed);
        finish_recovery(&mut app, &mut messages).await;
        receipt_page(&mut pages).await.send((StatusCode::OK, json!({"entries": [entry_event(300)["entry"]]}))).unwrap();
        read_receipt_check(&mut app, &mut messages).await;
        assert_eq!(seqs(&app), [300]);
        assert_eq!(app.drafts["a"].text, "Send receipt");
        assert_eq!(app.files["a"], app.sends["a"].files);
        assert_eq!(app.sends["a"].nonce, "send-301");
        assert!(!app.sends["a"].is_busy());
    }

    #[tokio::test]
    async fn queued_receipt_from_history_respects_the_order_of_the_next_request() {
        for confirmed_before_request in [true, false] {
            let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
                receipt_replay_fixture().await;
            let epoch = app.mirror_epoch;
            let after = pending_receipt_send(&mut app);
            drop(events);
            let checking = expect_stream(&mut requests, 0).await;
            replay_through_300(&checking);
            finish_recovery(&mut app, &mut messages).await;
            let row = send_receipt_entry(305);
            app.on_msg(Msg::Reply(after, Ok(json!({"entry": row["entry"]}))));
            receipt_page(&mut pages).await.send((StatusCode::OK, json!({"entries": [row["entry"]]}))).unwrap();
            let queued = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let msg = messages.recv().await.unwrap();
                    if matches!(msg, Msg::Reply(After::CheckSendReceipt { .. }, _)) {
                        break msg;
                    }
                    app.on_msg(msg);
                }
            })
            .await
            .unwrap();
            let queued = if confirmed_before_request {
                app.on_msg(queued);
                assert_eq!(seqs(&app), [300, 301]);
                None
            } else {
                Some(queued)
            };
            drop(checking);
            let restored = expect_stream(&mut requests, 0).await;
            if let Some(queued) = queued {
                app.on_msg(queued);
                assert_eq!(seqs(&app), [300]);
                assert!(app.hint.as_ref().is_none_or(|(text, _)| text != "Sent"));
            }
            replay_through_300(&restored);
            finish_recovery(&mut app, &mut messages).await;
            assert_eq!(app.mirror_epoch, epoch + u64::from(confirmed_before_request));
            if !confirmed_before_request {
                receipt_page(&mut pages).await.send((StatusCode::OK, json!({"entries": []}))).unwrap();
                read_receipt_check(&mut app, &mut messages).await;
                assert_eq!(app.drafts["a"].text, "Send receipt");
                assert_eq!(app.files["a"], app.sends["a"].files);
                assert_eq!(app.sends["a"].nonce, "send-301");
                assert!(!app.sends["a"].is_busy());
            }
            assert_eq!(seqs(&app), [300]);
        }
    }

    fn receive_send_receipt(app: &mut App, rev: i64) {
        let after = pending_receipt_send(app);
        app.on_msg(Msg::Reply(after, Ok(json!({"entry": send_receipt_entry(rev)["entry"]}))));
    }

    fn replay_through_300(events: &UnboundedSender<Value>) {
        events.send(restore_hello(301, 2)).unwrap();
        events.send(restored_bot("a", "Alpha", 20)).unwrap();
        events.send(entry_at_rev(entry_event(300), 301)).unwrap();
    }

    #[tokio::test]
    async fn receipt_replay_rewinds_a_send_confirmed_before_the_request() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, _) = receipt_replay_fixture().await;
        let epoch = app.mirror_epoch;
        receive_send_receipt(&mut app, 302);
        assert_eq!(seqs(&app), [300, 301]);
        drop(events);

        let restored = expect_stream(&mut requests, 0).await;
        replay_through_300(&restored);
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(seqs(&app), [300]);
        assert_eq!(app.mirror_epoch, epoch + 1);
        restored.send(restored_bot("a", "Alpha", 302)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        drop(restored);
        let live = expect_stream(&mut requests, 302).await;
        live.send(restore_hello(302, 0)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.mirror_epoch, epoch + 1);
        assert_eq!(seqs(&app), [300]);
    }

    #[tokio::test]
    async fn receipt_replay_accepts_a_send_confirmed_after_the_request() {
        for hello_rev in [301, 303] {
            let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
                receipt_replay_fixture().await;
            let epoch = app.mirror_epoch;
            let after = pending_receipt_send(&mut app);
            drop(events);

            let resumed = expect_stream(&mut requests, 0).await;
            resumed.send(restore_hello(hello_rev, 2)).unwrap();
            read_events(&mut app, &mut messages, 1).await;
            let buffered = send_receipt_entry(302);
            app.on_msg(Msg::Reply(after, Ok(json!({"entry": buffered["entry"]}))));
            assert_eq!(seqs(&app), [300]);
            assert!(pages.try_recv().is_err());
            resumed.send(restored_bot("a", "Alpha", 20)).unwrap();
            resumed.send(entry_at_rev(entry_event(300), 301)).unwrap();
            finish_recovery(&mut app, &mut messages).await;
            if hello_rev == 303 {
                app.drafts.insert("a".into(), Editor::with("New draft"));
            }
            let checked = send_receipt_entry(303);
            receipt_page(&mut pages).await.send((StatusCode::OK, json!({"entries": [checked["entry"]]}))).unwrap();
            read_receipt_check(&mut app, &mut messages).await;
            assert_eq!(app.mirror_epoch, epoch);
            assert_eq!(app.entries["a"][&301].id, "rpc-entry");
            assert!(!app.sends.contains_key("a"));
            assert_eq!(app.drafts.get("a").map(|d| d.text.as_str()), (hello_rev == 303).then_some("New draft"));
            resumed.send(buffered).unwrap();
            read_events(&mut app, &mut messages, 1).await;
            assert_eq!(app.entries["a"][&301].rev, Some(303));
            drop(resumed);
            let _live = expect_stream(&mut requests, 302).await;
        }
    }

    #[tokio::test]
    async fn receipt_replay_prefers_the_current_entry_when_a_sequence_is_reused() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            receipt_replay_fixture().await;
        let epoch = app.mirror_epoch;
        let after = pending_receipt_send(&mut app);
        drop(events);

        let resumed = expect_stream(&mut requests, 0).await;
        replay_through_300(&resumed);
        finish_recovery(&mut app, &mut messages).await;
        let old = send_receipt_entry(305);
        app.on_msg(Msg::Reply(after, Ok(json!({"entry": old["entry"]}))));
        let reply = receipt_page(&mut pages).await;
        let mut current = entry_at_rev(entry_event(301), 302);
        current["entry"]["id"] = "current-entry".into();
        current["entry"]["data"]["text"] = "New content".into();
        resumed.send(current).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        reply.send((StatusCode::OK, json!({"entries": [old["entry"]]}))).unwrap();
        read_receipt_check(&mut app, &mut messages).await;
        assert_eq!(app.sends["a"].nonce, "send-301");
        assert!(!app.sends["a"].is_busy());
        assert_eq!(app.entries["a"][&301].id, "current-entry");
        assert_eq!(app.entries["a"][&301].text(), "New content");
        drop(resumed);

        let live = expect_stream(&mut requests, 302).await;
        live.send(restore_hello(302, 0)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.mirror_epoch, epoch);
        assert_eq!(app.entries["a"][&301].id, "current-entry");
    }

    #[tokio::test]
    async fn a_current_stream_can_confirm_a_send_before_its_late_receipt() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            receipt_replay_fixture().await;
        let after = pending_receipt_send(&mut app);
        drop(events);
        let resumed = expect_stream(&mut requests, 0).await;
        let entry = send_receipt_entry(302);
        resumed.send(restore_hello(302, 2)).unwrap();
        resumed.send(restored_bot("a", "Alpha", 20)).unwrap();
        resumed.send(entry.clone()).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert!(!app.sends.contains_key("a"));
        assert!(!app.drafts.contains_key("a"));
        app.on_msg(Msg::Reply(after, Ok(json!({"entry": entry["entry"]}))));
        assert_eq!(seqs(&app), [301]);
        assert!(pages.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_lookup_reply_from_an_abandoned_connection_cannot_confirm_a_send() {
        for rewind in [false, true] {
            let mut app = test_app();
            pending_receipt_send(&mut app);
            let request = uuid::Uuid::new_v4();
            let after = After::CheckSendReceipt {
                key: "a".into(),
                request,
                epoch: app.mirror_epoch,
                connection: app.send_receipt_connection,
            };
            app.sends.get_mut("a").unwrap().receipt =
                SendReceiptCheck::Pending { id: "rpc-entry".into(), seq: 301, request: Some(request) };
            if rewind {
                app.on_msg(Msg::Rewind(SendReceipts::default()));
            } else {
                app.on_msg(Msg::Disconnected { since: 0 });
                app.sends.get_mut("a").unwrap().receipt =
                    SendReceiptCheck::Pending { id: "rpc-entry".into(), seq: 301, request: Some(uuid::Uuid::new_v4()) };
            }
            app.on_msg(Msg::Reply(after, Ok(json!({"entries": [send_receipt_entry(302)["entry"]]}))));
            assert_eq!(seqs(&app), [] as [i64; 0]);
            assert_eq!(app.drafts["a"].text, "Send receipt");
            assert_eq!(app.sends["a"].nonce, "send-301");
        }
    }

    #[test]
    fn a_send_check_requires_the_matching_current_row() {
        let entry = send_receipt_entry(302)["entry"].clone();
        let mut wrong_chat = entry.clone();
        wrong_chat["botId"] = "b".into();
        let mut wrong_id = entry.clone();
        wrong_id["id"] = "replacement".into();
        let mut wrong_nonce = entry.clone();
        wrong_nonce["data"]["clientNonce"] = "other".into();
        let mut removed = entry;
        removed["data"]["removed"] = true.into();
        for result in [
            Ok(json!({"entries": [wrong_chat]})),
            Ok(json!({"entries": [wrong_id]})),
            Ok(json!({"entries": [wrong_nonce]})),
            Ok(json!({"entries": [removed]})),
            Err("Can't reach the host.".to_owned()),
        ] {
            let mut app = test_app();
            pending_receipt_send(&mut app);
            let request = uuid::Uuid::new_v4();
            app.sends.get_mut("a").unwrap().receipt =
                SendReceiptCheck::Pending { id: "rpc-entry".into(), seq: 301, request: Some(request) };
            app.on_msg(Msg::Reply(
                After::CheckSendReceipt {
                    key: "a".into(),
                    request,
                    epoch: app.mirror_epoch,
                    connection: app.send_receipt_connection,
                },
                result,
            ));
            assert_eq!(seqs(&app), [] as [i64; 0]);
            assert_eq!(app.drafts["a"].text, "Send receipt");
            assert!(!app.sends["a"].is_busy());
        }
    }

    #[tokio::test]
    async fn receipt_replay_clears_the_previous_hosts_receipts() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, _) = receipt_replay_fixture().await;
        let epoch = app.mirror_epoch;
        events.send(restored_bot("a", "Alpha", 302)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        receive_send_receipt(&mut app, 310);
        drop(events);

        let other_hello = |rev, count| json!({"type": "hello", "hostId": "other", "rev": rev, "catchUp": count});
        let first = expect_stream(&mut requests, 302).await;
        first.send(other_hello(315, 0)).unwrap();
        let other = expect_stream(&mut requests, 0).await;
        other.send(other_hello(315, 2)).unwrap();
        other.send(restored_bot("a", "Other Alpha", 315)).unwrap();
        other.send(entry_at_rev(entry_event(1), 8)).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(app.mirror_epoch, epoch + 1);
        assert_eq!(seqs(&app), [1]);
    }

    #[tokio::test]
    async fn confirmed_window_refresh_pages_from_the_new_tail() {
        let entry = |seq| entry_at_rev(entry_event(seq), seq + 10);
        let mut rows = vec![restored_bot("a", "Alpha", 4), entry_at_rev(final_reply_event(1, 100, "Old reply"), 11)];
        rows.extend((2..=200).map(entry));
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            confirmed_restore_with(220, rows).await;
        app.selected = Some("a".into());
        app.load_history();
        assert!(app.history_complete("a"));
        drop(events);

        let replay = expect_stream(&mut requests, 0).await;
        replay.send(restore_hello(450, 201)).unwrap();
        replay.send(restored_bot("a", "Alpha", 4)).unwrap();
        for seq in 202..=401 {
            replay.send(entry(seq)).unwrap();
        }
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!((app.oldest("a"), seqs(&app).len()), (Some(202), 200));
        assert!(!app.history_complete("a"));

        let mut out_of_window = entry(50);
        out_of_window["rev"] = 417.into();
        out_of_window["entry"]["rev"] = 417.into();
        replay.send(out_of_window).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.oldest("a"), Some(202));

        app.load_history();
        let (request, send_page) = tokio::time::timeout(Duration::from_secs(2), pages.recv()).await.unwrap().unwrap();
        assert_eq!(request, json!({"botId": "a", "beforeSeq": 202, "limit": 200}));
        let old_reply = react_to_reply(final_reply_event(1, 100, "Old reply"), 415);
        let history: Vec<_> = std::iter::once(old_reply["entry"].clone())
            .chain((2..=201).filter(|&seq| seq != 3).map(|seq| entry(seq)["entry"].clone()))
            .collect();
        send_page.send(history_page(&history, 202, 200)).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while app.pending_history.contains_key("a") {
                app.on_msg(messages.recv().await.unwrap());
            }
        })
        .await
        .unwrap();
        assert!(app.entries["a"].contains_key(&201));
        assert!(!app.entries["a"].contains_key(&3), "history omits the withdrawn row");
        assert_eq!(app.entries["a"][&1].data["reactions"], json!(["👍"]));
        drop(replay);
        let _live = expect_stream(&mut requests, 417).await;
    }

    #[tokio::test]
    async fn confirmed_window_refresh_updates_equal_bot_revision_without_live_duplicates() {
        let mut old = restored_bot("a", "Alpha", 20);
        old["bot"]["unread"] = 1.into();
        old["bot"]["lastMessage"] = "Old preview".into();
        old["bot"]["completionReplyId"] = "old-reply".into();
        let (RestoredStream { mut app, mut requests, mut messages, events }, _) =
            confirmed_restore_with(26, vec![old.clone()]).await;
        app.term_focused = true;
        drop(events);

        let refreshed = expect_stream(&mut requests, 0).await;
        let mut snapshot = old.clone();
        snapshot["bot"]["unread"] = 0.into();
        snapshot["bot"]["lastMessage"] = "Updated preview".into();
        snapshot["bot"]["completionReplyId"] = "reply-during-disconnect".into();
        let tombstone = json!({"type": "bot", "rev": 20, "bot": {"id": "a", "rev": 20, "deleted": true}});
        refreshed.send(restore_hello(28, 2)).unwrap();
        refreshed.send(snapshot).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!((app.bots["a"].unread, app.bots["a"].last_message.as_str()), (1, "Old preview"));
        refreshed.send(tombstone.clone()).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!((app.bots["a"].unread, app.bots["a"].last_message.as_str()), (0, "Updated preview"));
        assert!(app.toasts.is_empty());

        refreshed.send(old).unwrap();
        refreshed.send(tombstone).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!((app.bots["a"].unread, app.bots["a"].last_message.as_str()), (0, "Updated preview"));
        assert!(app.toasts.is_empty());
    }

    async fn page_after_zero_replay(counted: bool, reaction: Option<Value>) {
        let entry = |seq| entry_at_rev(entry_event(seq), seq + 1);
        let mut snapshots = vec![legacy_bot_event(1, Some(200), 100, "Alpha")];
        snapshots.extend((1..=100).map(entry));
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            confirmed_restore_with(102, snapshots).await;
        app.selected = Some("a".into());
        drop(events);

        let refreshed = expect_stream(&mut requests, 0).await;
        let mut hello = restore_hello(302, 202);
        if !counted {
            hello.as_object_mut().unwrap().remove("catchUp");
        }
        refreshed.send(hello).unwrap();
        refreshed.send(json!({"type": "icons"})).unwrap();
        refreshed.send(legacy_bot_event(1, Some(200), 100, "Alpha")).unwrap();
        for seq in 101..=300 {
            refreshed.send(entry(seq)).unwrap();
        }
        let buffered = usize::from(reaction.is_some());
        if let Some(event) = reaction {
            refreshed.send(event).unwrap();
        }
        refreshed.send(entry_at_rev(entry_event(301), 303)).unwrap();
        read_events(&mut app, &mut messages, 204 + buffered).await;
        drop(refreshed);
        let _live = expect_stream(&mut requests, 303).await;

        app.load_history();
        let (request, send_page) = tokio::time::timeout(Duration::from_secs(2), pages.recv()).await.unwrap().unwrap();
        assert_eq!(request, json!({"botId": "a", "beforeSeq": 101, "limit": 200}));
        assert_eq!(seqs(&app), (101..=301).collect::<Vec<_>>());
        let rows: Vec<_> = (1..=100).map(|seq| entry(seq)["entry"].clone()).collect();
        send_page.send(history_page(&rows, 101, 200)).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while app.pending_history.contains_key("a") {
                app.on_msg(messages.recv().await.unwrap());
            }
        })
        .await
        .unwrap();
        assert!(app.lane("a").iter().any(|entry| entry.seq == 100));
    }

    #[tokio::test]
    async fn zero_replay_uncounted_pages_from_the_tail_above_a_buffered_reaction() {
        let reaction = react_to_reply(final_reply_event(50, 100, "Old reply"), 302);
        page_after_zero_replay(false, Some(reaction)).await;
    }

    #[tokio::test]
    async fn zero_replay_counted_pages_from_the_tail_above_a_buffered_reaction() {
        let reaction = react_to_reply(final_reply_event(50, 100, "Old reply"), 302);
        page_after_zero_replay(true, Some(reaction)).await;
    }

    #[tokio::test]
    async fn zero_replay_uncounted_pages_from_the_tail_without_a_buffered_reaction() {
        page_after_zero_replay(false, None).await;
    }

    fn stage_uncounted_tail(app: &mut App) {
        app.on_msg(Msg::ReplayStarted);
        app.on_event(&json!({"type": "hello", "rev": 302}));
        for seq in 101..=300 {
            app.on_event(&entry_at_rev(entry_event(seq), seq + 1));
        }
    }

    #[test]
    fn uncounted_tail_keeps_reply_evidence_and_withdrawals_below_the_window() {
        for withdrawn in [false, true] {
            let mut app = test_app();
            app.on_event(&legacy_bot_event(1, Some(200), 100, "Alpha"));
            let saved = entry_at_rev(final_reply_event(50, 210, "Saved reply"), 51);
            app.on_event(&saved);
            if withdrawn {
                let mut removal = entry_at_rev(saved.clone(), 302);
                removal["entry"]["data"]["removed"] = true.into();
                app.on_event(&removal);
            }

            stage_uncounted_tail(&mut app);
            app.on_event(&react_to_reply(saved, if withdrawn { 301 } else { 302 }));
            app.on_msg(Msg::ReplayComplete);

            assert_eq!(app.oldest("a"), Some(101));
            let observed = &app.legacy_replies["a"][&50];
            assert_eq!(observed.latest_rev, Some(302));
            assert_eq!(observed.reply.map(|reply| reply.rev), (!withdrawn).then_some(51));
            app.on_event(&legacy_bot_event(303, None, 210, "Saved reply"));
            assert_eq!(app.toasts.len(), usize::from(!withdrawn));
        }
    }

    #[tokio::test]
    async fn uncounted_tail_confirms_a_send_receipt_below_the_window() {
        let mut app = test_app();
        let send = pending_file_send(&mut app);
        stage_uncounted_tail(&mut app);
        let mut receipt = entry_at_rev(entry_event(50), 302);
        receipt["entry"]["data"]["clientNonce"] = "pending".into();
        app.on_event(&receipt);
        assert!(app.sends.contains_key("a") && app.files.contains_key("a"));

        app.on_msg(Msg::ReplayComplete);
        assert_eq!(app.oldest("a"), Some(101));
        assert!(!app.sends.contains_key("a") && !app.files.contains_key("a"));
        assert!(send.await.unwrap_err().is_cancelled());
    }

    fn pending_file_send(app: &mut App) -> tokio::task::JoinHandle<()> {
        let task = tokio::spawn(std::future::pending());
        let files = vec![std::path::PathBuf::from("report.txt")];
        app.drafts.insert("a".into(), Editor::with("With attachment"));
        app.files.insert("a".into(), files.clone());
        app.sends.insert(
            "a".into(),
            PendingSend {
                text: "With attachment".into(),
                files,
                nonce: "pending".into(),
                task: Some(task.abort_handle()),
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        task
    }

    #[tokio::test]
    async fn confirmed_replay_keeps_the_view_and_new_send_until_a_live_cursor() {
        let RestoredStream { mut app, mut requests, mut messages, events } = confirmed_restore().await;
        let send = pending_file_send(&mut app);
        let history = prepare_history(&mut app, "a");
        let epoch = app.mirror_epoch;
        drop(events);

        let empty = expect_stream(&mut requests, 0).await;
        empty.send(restore_hello(27, 0)).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(app.mirror_epoch, epoch, "the confirmed view survives a zero-cursor reconnect");
        drop(empty);

        let resumed = expect_stream(&mut requests, 0).await;
        resumed.send(restore_hello(28, 0)).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(app.bots["a"].name, "Alpha");
        assert!(app.sends["a"].task.is_some());
        assert!(!send.is_finished());
        assert_eq!(app.drafts["a"].text, "With attachment");
        assert_eq!(app.files["a"], app.sends["a"].files);
        app.on_msg(Msg::Reply(history, Ok(json!({"entries": [entry_event(1)["entry"]]}))));
        assert_eq!(seqs(&app), [1, 2]);

        resumed.send(restored_bot("a", "Live Alpha", 23)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.bots["a"].name, "Live Alpha");
        drop(resumed);
        let _live = expect_stream(&mut requests, 23).await;
        send.abort();
    }

    #[tokio::test]
    async fn an_interrupted_replay_over_a_confirmed_view_discards_unconfirmed_deletions_and_receipts() {
        let RestoredStream { mut app, mut requests, mut messages, events } = confirmed_restore().await;
        let send = pending_file_send(&mut app);
        let history = prepare_history(&mut app, "a");
        app.drafts.insert("b".into(), Editor::with("Keep until confirmed"));
        let epoch = app.mirror_epoch;
        let mut receipt = entry_at_rev(entry_event(3), 31);
        receipt["entry"]["data"] = json!({"clientNonce": "pending", "text": "With attachment"});
        let deletion = json!({"type": "bot", "rev": 32, "bot": {"id": "b", "rev": 32, "deleted": true}});
        let mut updated = entry_at_rev(entry_event(2), 34);
        updated["entry"]["data"] = json!({"text": "Newer transcript"});
        drop(events);

        let partial = expect_stream(&mut requests, 0).await;
        partial.send(restore_hello(40, 5)).unwrap();
        partial.send(receipt.clone()).unwrap();
        partial.send(deletion.clone()).unwrap();
        partial.send(updated).unwrap();
        partial.send(restored_bot("a", "Newer Alpha", 35)).unwrap();
        read_events(&mut app, &mut messages, 5).await;
        assert!(app.bots.contains_key("b") && app.drafts.contains_key("b"));
        assert!(app.sends["a"].task.is_some());
        assert_eq!(seqs(&app), [2]);
        assert_eq!(app.entries["a"][&2].rev, Some(22));
        assert!(app.pending_history.contains_key("a"));
        drop(partial);

        let retry = expect_stream(&mut requests, 0).await;
        retry.send(restore_hello(36, 2)).unwrap();
        retry.send(restored_bot("a", "Stale Alpha", 20)).unwrap();
        retry.send(entry_at_rev(entry_event(2), 21)).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(app.mirror_epoch, epoch);
        assert_eq!(app.bots["a"].name, "Newer Alpha");
        assert_eq!(app.entries["a"][&2].rev, Some(22));
        app.on_msg(Msg::Reply(history, Ok(json!({"entries": [entry_event(1)["entry"]]}))));
        assert_eq!(seqs(&app), [2], "a reply to the old window cannot alter the replacement");
        assert_eq!(app.drafts["b"].text, "Keep until confirmed");
        assert!(app.sends["a"].task.is_some() && app.files.contains_key("a"));
        drop(retry);

        let confirmed = expect_stream(&mut requests, 0).await;
        confirmed.send(restore_hello(40, 2)).unwrap();
        confirmed.send(deletion).unwrap();
        confirmed.send(receipt).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert!(!app.bots.contains_key("b") && !app.drafts.contains_key("b"));
        assert!(!app.drafts.contains_key("a") && !app.files.contains_key("a") && !app.sends.contains_key("a"));
        assert!(send.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn an_uncounted_window_refresh_applies_live_entries_after_replacing_the_tail() {
        let (RestoredStream { mut app, mut requests, mut messages, events }, mut pages) =
            confirmed_restore_with(26, vec![restored_bot("a", "Alpha", 20), entry_at_rev(entry_event(2), 22)]).await;
        let send = pending_file_send(&mut app);
        drop(events);

        let replay = expect_stream(&mut requests, 0).await;
        replay.send(json!({"type": "hello", "hostId": "host", "rev": 28})).unwrap();
        replay.send(entry_at_rev(entry_event(2), 25)).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!(app.entries["a"][&2].rev, Some(22));

        let mut delivered = entry_at_rev(entry_event(3), 27);
        delivered["entry"]["data"]["clientNonce"] = "pending".into();
        let after = After::Sent { key: "a".into(), nonce: "pending".into(), epoch: app.mirror_epoch };
        app.on_msg(Msg::Reply(after, Ok(json!({"entry": delivered["entry"]}))));
        let (request, history_response) =
            tokio::time::timeout(Duration::from_secs(2), pages.recv()).await.unwrap().unwrap();
        assert_eq!(request, json!({"botId": "a", "beforeSeq": 4, "limit": 1}));
        history_response.send((StatusCode::OK, json!({"entries": [delivered["entry"]]}))).unwrap();
        read_receipt_check(&mut app, &mut messages).await;
        assert!(!app.sends.contains_key("a"));
        send.abort();

        replay.send(entry_at_rev(entry_event(4), 29)).unwrap();
        finish_recovery(&mut app, &mut messages).await;
        assert_eq!(seqs(&app), [2, 3]);
        assert_eq!(app.entries["a"][&2].rev, Some(25));
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(seqs(&app), [2, 3, 4]);
        drop(replay);
        let _live = expect_stream(&mut requests, 29).await;
    }

    #[tokio::test]
    async fn a_second_restore_rewinds_from_zero_if_hello_is_behind_the_mirror() {
        for seen_newer_row in [false, true] {
            let RestoredStream { mut app, mut requests, mut messages, events } = confirmed_restore().await;
            let send = pending_file_send(&mut app);
            let epoch = app.mirror_epoch;
            drop(events);

            if seen_newer_row {
                let partial = expect_stream(&mut requests, 0).await;
                partial.send(restore_hello(40, 2)).unwrap();
                partial.send(restored_bot("a", "Newer Alpha", 35)).unwrap();
                read_events(&mut app, &mut messages, 2).await;
                assert_eq!(app.bots["a"].name, "Newer Alpha");
                drop(partial);
            }

            let restored = expect_stream(&mut requests, 0).await;
            restored.send(restore_hello(if seen_newer_row { 32 } else { 25 }, 1)).unwrap();
            restored.send(restored_bot("a", "Restored again", 19)).unwrap();
            finish_recovery(&mut app, &mut messages).await;
            assert_eq!(app.mirror_epoch, epoch + 1);
            assert_eq!(app.bots["a"].name, "Restored again");
            assert!(!app.bots.contains_key("b"));
            assert_eq!(seqs(&app), [] as [i64; 0]);
            assert!(app.sends["a"].task.is_none());
            assert_eq!(app.drafts["a"].text, "With attachment");
            assert_eq!(app.files["a"], app.sends["a"].files);
            assert!(send.await.unwrap_err().is_cancelled());
        }
    }

    async fn recovery_deletion_after_interrupted_replay(counted: bool) {
        let (client, mut requests) = stream_host().await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let hello = |rev, count| json!({"type": "hello", "hostId": "host", "rev": rev, "catchUp": count});
        let bot =
            |id, rev, deleted| json!({"type": "bot", "rev": rev, "bot": {"id": id, "rev": rev, "deleted": deleted}});
        let original = expect_stream(&mut requests, 0).await;
        original.send(hello(130, Some(1))).unwrap();
        original.send(bot("a", 128, false)).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        app.selected = Some("a".into());
        let files = vec![std::path::PathBuf::from("report.txt")];
        for id in ["a", "b"] {
            app.drafts.insert(id.into(), Editor::with("Unsent draft"));
            app.files.insert(id.into(), files.clone());
            app.pasted.insert(id.into(), vec!["Unsent".into()]);
            app.sends.insert(
                id.into(),
                PendingSend {
                    text: "Unsent draft".into(),
                    files: files.clone(),
                    nonce: "pending".into(),
                    task: None,
                    receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
                },
            );
        }
        drop(original);
        let rewind = expect_stream(&mut requests, 130).await;
        rewind.send(hello(26, counted.then_some(0))).unwrap();
        drop(rewind);
        let partial = expect_stream(&mut requests, 0).await;
        partial.send(hello(26, counted.then_some(2))).unwrap();
        partial.send(bot("a", 25, true)).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert!(!app.bots.contains_key("a"));
        drop(partial);

        let replacement = expect_stream(&mut requests, 0).await;
        replacement.send(hello(24, counted.then_some(3))).unwrap();
        replacement.send(bot("b", 21, true)).unwrap();
        replacement.send(bot("b", 20, false)).unwrap();
        read_events(&mut app, &mut messages, 3).await;
        assert_eq!(
            (app.drafts.get("b").map(|d| d.text.as_str()), app.files.get("b")),
            (Some("Unsent draft"), Some(&files)),
            "the replacement replay is still incomplete"
        );
        replacement.send(bot("a", 22, false)).unwrap();
        if !counted {
            replacement.send(bot("a", 27, false)).unwrap();
        }
        finish_recovery(&mut app, &mut messages).await;
        assert!(app.bots.contains_key("a"));
        assert_eq!(app.drafts.get("a").map(|d| d.text.as_str()), Some("Unsent draft"));
        assert_eq!(app.files["a"], files);
        assert_eq!(app.sends["a"].nonce, "pending");
        assert_eq!(app.selected.as_deref(), Some("a"));
        assert!(!app.bots.contains_key("b"), "a stale row cannot supersede the deletion");
        assert!(!app.drafts.contains_key("b") && !app.files.contains_key("b"));
        assert!(!app.sends.contains_key("b") && !app.pasted.contains_key("b"));

        replacement.send(bot("a", 28, true)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert!(!app.drafts.contains_key("a") && !app.files.contains_key("a"));
        assert!(!app.sends.contains_key("a") && !app.pasted.contains_key("a"));
        assert!(app.selected.is_none());
    }

    #[tokio::test]
    async fn recovery_deletion_preserves_drafts_after_an_interrupted_counted_replay() {
        recovery_deletion_after_interrupted_replay(true).await;
    }

    #[tokio::test]
    async fn recovery_deletion_preserves_drafts_after_an_interrupted_legacy_replay() {
        recovery_deletion_after_interrupted_replay(false).await;
    }

    async fn restored_stream_after_drop(catch_up: Option<u64>) {
        let (client, mut requests) = stream_host().await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let hello = |rev, count| json!({"type": "hello", "hostId": "host", "rev": rev, "catchUp": count});

        let original = expect_stream(&mut requests, 0).await;
        original.send(hello(130, Some(0))).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        drop(original);
        let abandoned = expect_stream(&mut requests, 130).await;
        abandoned.send(hello(26, catch_up)).unwrap();
        drop(abandoned);

        let first = expect_stream(&mut requests, 0).await;
        first.send(hello(26, catch_up)).unwrap();
        if catch_up.is_some() {
            first.send(json!({"type": "bot", "rev": 25, "bot": {"id": "a", "rev": 25, "name": "Alpha"}})).unwrap();
        }
        read_events(&mut app, &mut messages, if catch_up.is_some() { 2 } else { 1 }).await;
        drop(first);

        let second = expect_stream(&mut requests, 0).await;
        second.send(hello(26, catch_up)).unwrap();
        let mut replayed = entry_event(2);
        replayed["rev"] = json!(23);
        second.send(replayed).unwrap();
        let live = if catch_up.is_some() { 24 } else { 27 };
        second.send(json!({"type": "bot", "rev": live, "bot": {"id": "a", "rev": live, "name": "Restored"}})).unwrap();
        read_events(&mut app, &mut messages, 3).await;
        assert_eq!(app.bots["a"].name, if catch_up.is_some() { "Alpha" } else { "Restored" });
        assert_eq!(seqs(&app), [2]);
        drop(second);
        let _normal = expect_stream(&mut requests, live).await;
    }

    #[tokio::test]
    async fn restored_stream_without_a_live_revision_retries_after_counted_replay() {
        restored_stream_after_drop(Some(1)).await;
    }

    #[tokio::test]
    async fn restored_stream_without_a_live_revision_retries_after_legacy_hello() {
        restored_stream_after_drop(None).await;
    }

    async fn restored_send_after_partial_replay(catch_up: Option<u64>) {
        let (client, mut requests) = stream_host().await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        client.spawn_stream(tx);
        let hello = |rev, count| json!({"type": "hello", "hostId": "host", "rev": rev, "catchUp": count});

        let original = expect_stream(&mut requests, 0).await;
        original.send(hello(130, Some(0))).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        let files = vec![std::path::PathBuf::from("report.txt")];
        app.drafts.insert("a".into(), Editor::with("Retry me"));
        app.files.insert("a".into(), files.clone());
        app.sends.insert(
            "a".into(),
            PendingSend {
                text: "Retry me".into(),
                files: files.clone(),
                nonce: "pending".into(),
                task: None,
                receipt: SendReceiptCheck::Direct(app.send_receipt_connection),
            },
        );
        drop(original);

        let abandoned = expect_stream(&mut requests, 130).await;
        abandoned.send(hello(26, catch_up)).unwrap();
        drop(abandoned);
        let first = expect_stream(&mut requests, 0).await;
        first.send(hello(26, catch_up)).unwrap();
        let mut receipt = entry_event(2);
        receipt["rev"] = json!(24);
        receipt["entry"]["data"] = json!({"clientNonce": "pending", "text": "Retry me"});
        first.send(receipt.clone()).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        drop(first);

        let second = expect_stream(&mut requests, 0).await;
        second.send(hello(25, catch_up.map(|_| 1))).unwrap();
        second.send(json!({"type": "bot", "rev": 25, "bot": {"id": "a", "rev": 25, "name": "Alpha"}})).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!(app.drafts.get("a").map(|d| d.text.as_str()), Some("Retry me"));
        assert_eq!(app.files["a"], files);
        assert_eq!(app.sends["a"].nonce, "pending");
        assert_eq!(seqs(&app), [] as [i64; 0]);

        drop(second);
        let third = expect_stream(&mut requests, 0).await;
        third.send(hello(26, catch_up)).unwrap();
        third.send(receipt.clone()).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert!(app.sends.contains_key("a"));
        let rev = if catch_up.is_some() { 25 } else { 27 };
        third.send(json!({"type": "bot", "rev": rev, "bot": {"id": "a", "rev": rev}})).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        finish_recovery(&mut app, &mut messages).await;
        if catch_up.is_some() {
            assert_eq!(client.rev.load(std::sync::atomic::Ordering::Relaxed), 0);
        }
        assert!(!app.sends.contains_key("a"));
        assert!(!app.drafts.contains_key("a"));
        assert!(!app.files.contains_key("a"));
        app.drafts.insert("a".into(), Editor::with("Next draft"));
        third.send(receipt).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.drafts["a"].text, "Next draft");
    }

    #[tokio::test]
    async fn restored_send_survives_an_interrupted_counted_replay() {
        restored_send_after_partial_replay(Some(2)).await;
    }

    #[tokio::test]
    async fn restored_send_survives_an_interrupted_legacy_replay() {
        restored_send_after_partial_replay(None).await;
    }

    #[tokio::test]
    async fn a_restored_host_rewinds_the_stream_and_replaces_the_bot_and_chat_mirror() {
        let (client, mut requests) = stream_host().await;
        let (tx, mut messages) = unbounded_channel();
        let mut app = App::new(client.clone(), tx.clone(), String::new());
        app.term_focused = true;
        client.spawn_stream(tx);

        let hello = |rev, catch_up| json!({"type": "hello", "hostId": "host", "rev": rev, "catchUp": catch_up});
        let bot = |id, name, rev, status, reply: Option<&str>| {
            json!({"type": "bot", "rev": rev, "bot": {
                "id": id, "name": name, "rev": rev, "status": status, "completionReplyId": reply
            }})
        };
        let original = expect_stream(&mut requests, 0).await;
        original.send(hello(130, 4)).unwrap();
        original.send(bot("a", "Renamed Alpha", 121, "idle", Some("abandoned-reply"))).unwrap();
        let mut entry = entry_event(2);
        entry["entry"]["data"] = json!({"text": "Edited later"});
        original.send(entry.clone()).unwrap();
        original.send(entry_event(9)).unwrap();
        original.send(bot("later", "Created later", 128, "idle", None)).unwrap();
        read_events(&mut app, &mut messages, 5).await;
        app.drafts.insert("a".into(), Editor::with("Draft to keep"));
        let old_history = prepare_history(&mut app, "a");
        drop(original);

        let abandoned = expect_stream(&mut requests, 130).await;
        abandoned.send(hello(24, 1)).unwrap();
        abandoned.send(entry_event(10)).unwrap();
        drop(abandoned);

        let interrupted = expect_stream(&mut requests, 0).await;
        interrupted.send(hello(24, 2)).unwrap();
        interrupted.send(bot("partial", "Partial snapshot", 20, "idle", None)).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        drop(interrupted);

        let restored = expect_stream(&mut requests, 0).await;
        restored.send(hello(24, 4)).unwrap();
        restored.send(json!({"type": "icons"})).unwrap();
        restored.send(bot("b", "Beta", 19, "idle", Some("historical-reply"))).unwrap();
        restored.send(bot("a", "Alpha", 21, "working", None)).unwrap();
        entry["rev"] = json!(22);
        entry["entry"]["data"] = json!({"text": "Before the edit"});
        restored.send(entry).unwrap();
        read_events(&mut app, &mut messages, 5).await;
        app.on_msg(Msg::Reply(old_history, Ok(json!({"entries": [entry_event(9)["entry"]]}))));
        assert_eq!(app.bots.len(), 2);
        assert_eq!(app.bots["a"].name, "Alpha");
        assert_eq!(app.bots["a"].status, Status::Working);
        assert_eq!(seqs(&app), [2]);
        assert_eq!(app.entries["a"][&2].text(), "Before the edit");
        assert_eq!(app.drafts["a"].text, "Draft to keep");
        assert!(app.toasts.is_empty());

        restored.send(bot("b", "Beta", 22, "idle", Some("historical-reply"))).unwrap();
        restored.send(bot("a", "Alpha", 23, "idle", Some("fresh-reply"))).unwrap();
        read_events(&mut app, &mut messages, 2).await;
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Alpha is done");
        app.toasts.clear();
        for rev in [23, 25] {
            restored.send(bot("a", "Alpha", rev, "idle", Some("fresh-reply"))).unwrap();
        }
        read_events(&mut app, &mut messages, 2).await;
        assert!(app.toasts.is_empty());
        drop(restored);

        let normal = expect_stream(&mut requests, 25).await;
        normal.send(hello(25, 0)).unwrap();
        read_events(&mut app, &mut messages, 1).await;
        assert_eq!(app.bots["a"].name, "Alpha");
    }

    #[test]
    fn a_live_completion_can_precede_hello_rev_after_the_counted_catch_up() {
        let mut app = test_app();
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": 120, "catchUp": 3})));
        app.on_msg(Msg::Event(json!({"type": "icons"})));
        app.on_msg(completion_event("Alpha", 117, "working", None));
        app.on_msg(Msg::Event(json!({"type": "futureEvent"})));
        assert!(app.toasts.is_empty());

        app.on_msg(completion_event("Alpha", 119, "idle", Some("during-connect")));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Alpha is done");
        app.toasts.clear();
        app.on_msg(completion_event("Alpha", 119, "idle", Some("during-connect")));
        assert!(app.toasts.is_empty());
        app.on_msg(completion_event("Alpha", 122, "working", None));
        app.on_msg(completion_event("Alpha", 124, "idle", Some("next-reply")));
        assert_eq!(app.toasts.len(), 1);
    }

    #[test]
    fn older_buffered_bot_states_and_duplicate_replies_stay_quiet_after_catch_up() {
        let mut app = test_app();
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": 110, "catchUp": 0})));
        app.on_msg(completion_event("kept", 111, "idle", Some("already-held")));
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": 120, "catchUp": 4})));
        app.on_msg(completion_event("replayed", 116, "idle", Some("in-snapshot")));
        app.on_msg(Msg::Event(
            json!({"type": "bot", "rev": 117, "bot": {"id": "deleted", "rev": 117, "deleted": true}}),
        ));
        app.on_msg(Msg::Event(json!({"type": "futureEvent"})));
        app.on_msg(Msg::Event(json!({"type": "usage", "usage": {"providers": []}})));

        app.on_msg(completion_event("kept", 109, "idle", Some("older-reply")));
        app.on_msg(completion_event("kept", 110, "working", None));
        app.on_msg(completion_event("kept", 111, "idle", Some("already-held")));
        app.on_msg(completion_event("kept", 112, "working", None));
        app.on_msg(completion_event("replayed", 113, "working", None));
        app.on_msg(completion_event("deleted", 114, "working", None));
        assert!(!app.bots.contains_key("deleted"));
        app.on_msg(completion_event("kept", 115, "idle", Some("already-held")));
        app.on_msg(completion_event("replayed", 118, "working", None));
        app.on_msg(completion_event("replayed", 119, "idle", Some("in-snapshot")));
        app.on_msg(completion_event("replayed", 121, "idle", Some("in-snapshot")));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn done_toast_skips_turns_that_left_no_new_reply_in_this_chat() {
        let mut app = test_app();
        app.upsert_bot(&json!({"id":"a", "rev":1, "name":"Alpha", "status":"working", "startedAt":200, "lastAt":100, "lastMessage":"old X", "unread":0}), true);
        app.upsert_bot(&json!({"id":"a", "rev":2, "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":100, "lastMessage":"old X", "unread":0}), true);
        assert!(app.toasts.is_empty(), "bot-to-bot or silent turn must not toast with stale chat preview");

        // User queued a message while Alpha worked on another bot's request.
        app.upsert_bot(&json!({"id":"a", "rev":3, "name":"Alpha", "status":"working", "startedAt":300, "lastAt":310, "lastMessage":"You: hey", "unread":0}), true);
        app.upsert_bot(&json!({"id":"a", "rev":4, "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":310, "lastMessage":"You: hey", "unread":0}), true);
        assert!(app.toasts.is_empty(), "a queued user message is not a bot reply");

        // A real reply in Alpha's own chat (including mid-turn send_message) toasts as done.
        app.upsert_bot(&json!({"id":"a", "rev":5, "name":"Alpha", "status":"working", "startedAt":400, "lastAt":310, "lastMessage":"You: hey", "unread":0}), true);
        app.on_event(&entry_at_rev(final_reply_event(1, 410, "Fresh reply"), 6));
        app.upsert_bot(&json!({"id":"a", "rev":7, "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":410, "lastMessage":"Fresh reply", "unread":1}), true);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Alpha is done");
    }
}
