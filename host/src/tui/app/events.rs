//! Host messages: the event stream and command replies.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{After, App, Bot, Dir, Entry, Focus, Msg, Overlay, PairCode, Status, merge_toggles};

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
            }
            Msg::Event(v) => self.on_event(&v),
            Msg::Reply(after, r) => self.on_reply(after, r),
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
        match v["type"].as_str() {
            Some("hello") => {
                self.snapshot_floors();
                self.network = v["network"].clone();
                if !v["usage"].is_null() {
                    self.usage = v["usage"].clone();
                }
            }
            Some("usage") => self.usage = v["usage"].clone(),
            Some("screen") => self.screen = v["screen"].clone(),
            Some("network") => self.network_changed(&v["network"]),
            Some("bot") => {
                self.upsert_bot(&v["bot"]);
                self.memory_moved(&v["bot"]);
            }
            Some("artifact") => self.artifact_changed(&v["artifact"]),
            Some("entry") => {
                if let Some((bot, e)) = Entry::parse(&v["entry"])
                    && !self.outside_window(&bot, &e)
                {
                    if !self.entries.get(&bot).is_some_and(|m| m.contains_key(&e.seq)) {
                        self.entry_added(&bot, &e);
                    }
                    self.entries.entry(bot).or_default().insert(e.seq, e);
                }
            }
            _ => {}
        }
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

    fn upsert_bot(&mut self, v: &Value) {
        let Some(id) = v["id"].as_str() else { return };
        if v["deleted"].as_bool().unwrap_or(false) {
            self.bots.remove(id);
            self.entries.remove(id);
            self.floors.remove(id);
            self.drafts.remove(id);
            self.files.remove(id);
            if self.selected.as_deref() == Some(id) {
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
        let watching = self.selected.as_deref() == Some(new.id.as_str()) && self.chat_visible() && self.term_focused;
        // A member's turn in a group is the group's: it alone says done / needs you.
        if !new.notify || watching || old.away().is_some() || new.away().is_some() {
            return;
        }
        if new.status == Status::NeedsInput && old.status != Status::NeedsInput {
            self.toast(format!("{} needs you", new.name), true);
        } else if old.status == Status::Working && new.status == Status::Idle {
            self.toast(format!("{} is done", new.name), false);
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
                    After::Dirs => {
                        if let Some(Overlay::Folder(p)) = self.overlays.last_mut() {
                            p.error = Some(e);
                        }
                    }
                    // In place of the code, which may have expired.
                    After::Pairing => {
                        if let Some(Overlay::Pair(code)) = self.overlays.last_mut() {
                            *code = PairCode::Failed(e);
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
            | After::Sent(_)
            | After::Connection(_, _) => {}
            After::Installed => {
                self.error = None;
                self.flash("Host installed; connecting…");
                self.call("hello", json!({}), After::Hello);
            }
            After::Hello => {
                self.mismatch = crate::compat::check(env!("CARGO_PKG_VERSION"), crate::compat::MIN_HOST, &v);
                v["name"].as_str().unwrap_or("computer").clone_into(&mut self.host);
                v["home"].as_str().unwrap_or_default().clone_into(&mut self.home);
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
                self.analytics_hello(&v);
                self.screen = v["screen"].clone();
            }
            After::Analytics => self.analytics_set(&v),
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
            After::Dirs => {
                if let Some(Overlay::Folder(p)) = self.overlays.last_mut() {
                    v["path"].as_str().unwrap_or_default().clone_into(&mut p.path);
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
            After::Pairing => {
                if let Some(Overlay::Pair(code)) = self.overlays.last_mut() {
                    *code = match v["pairingUrl"].as_str() {
                        Some(url) => PairCode::Ready(url.to_owned()),
                        None => PairCode::Failed("The host sent no pairing code.".into()),
                    };
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
                    self.upsert_bot(&v["bot"]);
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
        app.upsert_bot(&json!({"id":"b", "computer":true}));
        assert!(app.bots["b"].computer);
        assert!(FIELDS.contains(&Field::Computer));
    }

    #[test]
    fn fast_mode_survives_loading_the_editor() {
        let mut app = test_app();
        app.upsert_bot(&json!({"id":"b", "fastMode":true}));
        assert!(app.bots["b"].fast_mode);
        assert!(FIELDS.contains(&Field::Fast));
    }

    #[test]
    fn a_refused_fast_mode_reads_its_reason() {
        let mut app = test_app();
        app.upsert_bot(
            &json!({"id":"b", "fastMode":true, "fastModeSupport":"refused", "fastModeIssue":"requires extra usage"}),
        );
        assert!(app.bots["b"].fast_shown());
        assert!(app.bots["b"].fast_refusal().is_some_and(|why| why.contains("requires extra usage")));
        app.upsert_bot(&json!({"id":"g", "kind":"group", "fastModeSupport":"offered"}));
        assert!(!app.bots["g"].fast_shown());
    }
}
