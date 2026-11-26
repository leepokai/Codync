//! Host messages: the event stream and command replies.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{After, App, Bot, Dir, Entry, Focus, Kind, Msg, Overlay, Status, merge_toggles};

/// Whether a finished turn left a new bot reply in this chat (not a bot-to-bot turn, pass, or queued user send).
fn turn_replied(old: &Bot, new: &Bot) -> bool {
    let from_bot = !new.last_message.is_empty() && !new.last_message.starts_with("You: ");
    if let Some(started_at) = old.started_at {
        return new.unread > old.unread || (new.last_at >= started_at && from_bot);
    }
    new.unread > old.unread || ((new.last_at != old.last_at || new.last_message != old.last_message) && from_bot)
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
            Msg::Online(on) => {
                self.online = on;
                if !on {
                    // The next connection may reach another host: nothing goes by this one's version.
                    self.host_version.clear();
                }
                if !on && self.error.is_none() && self.bots.is_empty() {
                    // Say why instead of spinning on "Connecting…".
                    self.call("hello", json!({}), After::Hello);
                }
                if on {
                    self.error = None;
                    // Every reconnect: the host may have been updated (or replaced) meanwhile.
                    self.call("hello", json!({}), After::Hello);
                }
            }
            Msg::Rewind => {
                self.bots.clear();
                self.entries.clear();
                self.history_done.clear();
                self.floors.clear();
            }
            Msg::Connected { since } => {
                self.stream_since = since;
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
                self.replayed_deletions.clear();
                self.completions_before_live.clear();
                if self.catch_up_left.is_some() {
                    self.completions_before_live
                        .extend(self.bots.values().filter_map(|bot| bot.completion_reply_id.clone()));
                }
                self.snapshot_floors();
                self.network = v["network"].clone();
                self.workflows = v["workflows"].as_bool().unwrap_or(false);
                if !v["usage"].is_null() {
                    self.usage = v["usage"].clone();
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
            Some("threadKeepDays") => self.thread_keep_days = super::thread_keep::days(&v["days"]),
            Some("pairingRequests") => self.pairing_requests_changed(v),
            Some("bot") => self.on_bot_event(v, catch_up_left),
            Some("artifact") => self.artifact_changed(&v["artifact"]),
            Some("threadEntry") => self.thread_entries.keep(v["thread"].as_str(), &v["entry"]),
            Some("entry") => {
                if let Some((bot, e)) = Entry::parse(&v["entry"])
                    && !self.outside_window(&bot, &e)
                {
                    if !self.entries.get(&bot).is_some_and(|m| m.contains_key(&e.seq)) {
                        self.entry_added(&bot, &e);
                        // A routine's run and a follow-up say in the chat that they began or were dropped.
                        if e.kind == Kind::Notice {
                            self.reload_details(&bot);
                        }
                    }
                    self.entries.entry(bot).or_default().insert(e.seq, e);
                }
            }
            _ => {}
        }
    }

    fn on_bot_event(&mut self, event: &Value, catch_up_left: Option<u64>) {
        let bot = &event["bot"];
        let rev = event["rev"].as_i64();
        if catch_up_left.is_some_and(|left| left > 0)
            && let Some(reply) = bot["completionReplyId"].as_str()
        {
            self.completions_before_live.insert(reply.to_owned());
        }
        if let (Some(left), Some(id), Some(rev)) = (catch_up_left, bot["id"].as_str(), rev) {
            if left == 0 {
                let previous_rev =
                    self.bots.get(id).and_then(|bot| bot.rev).or_else(|| self.replayed_deletions.get(id).copied());
                if previous_rev.is_some_and(|previous| rev <= previous) {
                    return;
                }
            } else if bot["deleted"] == true {
                self.replayed_deletions.insert(id.to_owned(), rev);
            }
        }
        let live = catch_up_left.map_or_else(|| rev.is_none_or(|rev| rev > self.replay_rev), |left| left == 0);
        self.upsert_bot(bot, live);
        self.close_idle_work_page();
        self.memory_moved(bot);
    }

    /// A catch-up from `since > 0` starts: remember where each bot's loaded chat begins.
    fn snapshot_floors(&mut self) {
        self.floors = if self.stream_since > 0 {
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

    fn outside_window(&self, bot: &str, e: &Entry) -> bool {
        let held = self.entries.get(bot).is_some_and(|m| m.contains_key(&e.seq));
        outside_window(self.floors.get(bot).copied(), held, e)
    }

    fn upsert_bot(&mut self, v: &Value, live: bool) {
        let Some(id) = v["id"].as_str() else { return };
        if v["deleted"].as_bool().unwrap_or(false) {
            if let Some(gone) = self.bots.remove(id) {
                self.threads_ended(&gone.work, &[]);
            }
            self.entries.remove(id);
            self.floors.remove(id);
            self.drafts.remove(id);
            self.files.remove(id);
            if self.selected.as_deref() == Some(id) {
                self.close_find();
                self.selected = None;
                self.reply = None;
                self.pick = None;
                self.chat_page = false;
                self.typing = false;
            }
            return;
        }
        let Some(new) = Bot::parse(v) else { return };
        // Marked read once per change.
        if self.bots.get(&new.id).is_none_or(|old| old.unread != new.unread) {
            self.reading.remove(&new.id);
        }
        let old = self.bots.insert(new.id.clone(), new.clone());
        let Some(old) = old else { return };
        self.threads_ended(&old.work, &new.work);
        self.follow_threads(&new.id);
        self.keep_details_cursor(&new.id, (&old.work, old.max_threads));
        let watching = self.selected.as_deref() == Some(new.id.as_str()) && self.chat_visible() && self.term_focused;
        // A member's turn in a group is the group's: it alone says done / needs you.
        if !new.notify || watching {
            return;
        }
        let away = old.away().is_some() || new.away().is_some();
        if live && new.status == Status::Idle {
            let replied = if v.get("completionReplyId").is_some() {
                new.completion_reply_id.as_deref().is_some_and(|reply| {
                    old.completion_reply_id.as_deref() != Some(reply) && !self.completions_before_live.contains(reply)
                })
            } else {
                !away && old.status == Status::Working && turn_replied(&old, &new)
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

    fn on_reply(&mut self, after: After, r: Result<Value, String>) {
        if let After::Connection(id, step) = after {
            self.connection_reply(&id, step, r);
            return;
        }
        if let After::Sent(key) = after {
            self.sent(&key, r);
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
        let v = match r {
            Ok(v) => v,
            Err(e) => {
                match after {
                    After::Hello => self.error = Some(e),
                    After::Tracked => {}
                    After::History(id) => {
                        self.history_busy.remove(&id);
                    }
                    After::Saved => match self.overlays.last_mut() {
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
            | After::Tracked
            | After::Answered(_)
            | After::Sheet(_)
            | After::Network(_)
            | After::UsageOrder(_)
            | After::Sent(_)
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
            After::History(id) => {
                self.history_busy.remove(&id);
                let list = v["entries"].as_array().cloned().unwrap_or_default();
                if list.is_empty() {
                    self.history_done.insert(id);
                }
                for e in &list {
                    if let Some((bot, e)) = Entry::parse(e) {
                        self.entries.entry(bot).or_default().entry(e.seq).or_insert(e);
                    }
                }
            }
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
            After::Saved => {
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

    fn sent(&mut self, key: &str, result: Result<Value, String>) {
        let Some(sent) = self.sends.get_mut(key) else { return };
        sent.busy = false;
        match result {
            Ok(value) => {
                if self.drafts.get(key).is_some_and(|d| d.text.trim() == sent.text)
                    && self.files.get(key).map_or(&[][..], Vec::as_slice) == sent.files
                {
                    self.drafts.remove(key);
                    self.files.remove(key);
                    self.pasted.remove(key);
                }
                self.sends.remove(key);
                if self.selected.as_deref() == Some(key) {
                    self.reply = None;
                }
                // The stream may already hold a newer copy (its delivery status).
                if let Some((bot, e)) = Entry::parse(&value["entry"]) {
                    super::kofi::sent();
                    self.entries.entry(bot).or_default().entry(e.seq).or_insert(e);
                }
                self.flash("Sent");
            }
            Err(error) => self.flash(&format!("{error} Draft kept; Enter retries, Ctrl+C clears text.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Editor, FIELDS, Field, PendingSend, test_app};
    use super::*;

    fn entry_event(seq: i64) -> Value {
        json!({"type": "entry", "entry": {"botId": "a", "id": format!("e{seq}"), "seq": seq, "turn": seq,
            "kind": "user"}})
    }

    fn seqs(app: &App) -> Vec<i64> {
        app.entries.get("a").map(|m| m.keys().copied().collect()).unwrap_or_default()
    }

    fn connect(app: &mut App, since: i64) {
        app.on_msg(Msg::Connected { since });
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
        app.on_reply(After::History("a".into()), Ok(json!({"entries": [entry_event(3)["entry"]]})));
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

    #[test]
    fn send_failure_keeps_text_files_and_nonce_for_retry() {
        let mut app = test_app();
        let key = "bot";
        let files = vec![std::path::PathBuf::from("/tmp/report.txt")];
        app.drafts.insert(key.into(), Editor::with("draft"));
        app.files.insert(key.into(), files.clone());
        app.sends.insert(
            key.into(),
            PendingSend { text: "draft".into(), files: files.clone(), nonce: "same-nonce".into(), busy: true },
        );
        app.on_reply(After::Sent(key.into()), Err("Disconnected".into()));
        assert_eq!(app.drafts[key].text, "draft");
        assert_eq!(app.files[key], files);
        assert_eq!(app.sends[key].nonce, "same-nonce");
        assert!(!app.sends[key].busy);
        app.on_reply(After::Sent(key.into()), Ok(json!({})));
        assert!(!app.drafts.contains_key(key));
        assert!(!app.files.contains_key(key));
    }

    #[test]
    fn late_send_success_does_not_erase_a_new_draft() {
        let mut app = test_app();
        app.drafts.insert("bot".into(), Editor::with("new draft"));
        app.sends.insert(
            "bot".into(),
            PendingSend { text: "old draft".into(), files: vec![], nonce: "n".into(), busy: true },
        );
        app.on_reply(After::Sent("bot".into()), Ok(json!({})));
        assert_eq!(app.drafts["bot"].text, "new draft");
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
        app.on_msg(Msg::Online(false));
        assert!(app.host_version.is_empty(), "the host that answers next may be another build");
    }

    #[test]
    fn a_reaction_on_an_old_reply_does_not_toast_as_done() {
        let mut app = test_app();
        app.on_event(&json!({"type":"bot", "bot": {
            "id":"a", "name":"Alpha", "status":"working", "startedAt":200,
            "lastAt":100, "lastMessage":"Old reply", "completionReplyId":null
        }}));
        app.on_event(&json!({"type":"bot", "bot": {
            "id":"a", "name":"Alpha", "status":"idle",
            "lastAt":210, "lastMessage":"Old reply", "completionReplyId":null
        }}));
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
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"working", "startedAt":200, "lastAt":100, "lastMessage":"old X", "unread":0}), true);
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":100, "lastMessage":"old X", "unread":0}), true);
        assert!(app.toasts.is_empty(), "bot-to-bot or silent turn must not toast with stale chat preview");

        // User queued a message while Alpha worked on another bot's request.
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"working", "startedAt":300, "lastAt":310, "lastMessage":"You: hey", "unread":0}), true);
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":310, "lastMessage":"You: hey", "unread":0}), true);
        assert!(app.toasts.is_empty(), "a queued user message is not a bot reply");

        // A real reply in Alpha's own chat (including mid-turn send_message) toasts as done.
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"working", "startedAt":400, "lastAt":410, "lastMessage":"Fresh reply", "unread":1}), true);
        app.upsert_bot(&json!({"id":"a", "name":"Alpha", "status":"idle", "startedAt":null, "lastAt":410, "lastMessage":"Fresh reply", "unread":1}), true);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Alpha is done");
    }
}
