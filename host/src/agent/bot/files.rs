//! File copies run outside the actor; publication still belongs to its original turn. Artifact cards post the same way.
use super::Actor;
use crate::chat::files::{self, Snapshot};
use crate::chat::replies::with_quote;
use crate::store::EntryKind;
use anyhow::{Result, anyhow, ensure};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};

type Reply = oneshot::Sender<Result<()>>;
pub(super) struct Completed {
    epoch: u64,
    text: Option<String>,
    quote: Option<Value>,
    cancelled: Arc<AtomicBool>,
    reply: Reply,
    result: Result<Snapshot>,
}

pub(super) struct FileShares {
    epoch: u64,
    pub summary: Option<String>,
    pending: Vec<Arc<AtomicBool>>,
    pub tx: mpsc::UnboundedSender<Completed>,
    pub rx: mpsc::UnboundedReceiver<Completed>,
}
impl Default for FileShares {
    fn default() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { epoch: 0, summary: None, pending: Vec::new(), tx, rx }
    }
}
impl FileShares {
    pub fn cancel(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        for flag in self.pending.drain(..) {
            flag.store(true, Ordering::Release);
        }
    }
}
impl Drop for FileShares {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Actor {
    pub(super) fn begin_file(
        &mut self,
        source: String,
        name: Option<String>,
        text: Option<String>,
        quote: Option<Value>,
        cancelled: Arc<AtomicBool>,
        reply: Reply,
    ) {
        if let Err(error) = self.can_send_file() {
            let _ = reply.send(Err(error));
            return;
        }
        self.files.pending.retain(|flag| !flag.load(Ordering::Acquire));
        if self.files.pending.len() >= 4 {
            let _ = reply.send(Err(anyhow!("too many pending file shares")));
            return;
        }
        self.files.pending.push(cancelled.clone());
        let epoch = self.files.epoch;
        // A thread's file is its bot's: it shows in that chat, and outlives the thread.
        let root = files::root(self.cfg.owner());
        let cwd = PathBuf::from(&self.cfg.cwd);
        let tx = self.files.tx.clone();
        tokio::spawn(async move {
            let flag = cancelled.clone();
            let result =
                tokio::task::spawn_blocking(move || files::snapshot(&root, &cwd, &source, name.as_deref(), &flag))
                    .await;
            let result = result.map_err(anyhow::Error::from).and_then(std::convert::identity);
            let _ = tx.send(Completed { epoch, text, quote, cancelled, reply, result });
        });
    }

    fn can_send_file(&self) -> Result<()> {
        self.turn_writes()?;
        ensure!(self.in_own_chat(), "send_file is only available in the bot's own chat");
        Ok(())
    }

    /// A turn that may still write into the chat: one is running, and the user hasn't stopped it.
    fn turn_writes(&self) -> Result<()> {
        ensure!(!self.stop_requested, "turn is stopping");
        ensure!(self.active_workflow.is_none(), "send_file and artifact cards aren't available in a workflow step");
        ensure!(self.turn.is_some(), "no turn is running: between turns, tell the user with send_message");
        Ok(())
    }

    /// A published artifact's card (`chat::artifacts`, its entry's `data`): a message of the turn, like a shared file.
    /// A routine's run posts it too: its lane is the bot's own chat, and that is where the user reads what the run
    /// published (`#380`). A room's or another bot's request leaves the artifact in the Library with no card.
    pub(super) fn post_artifact(&mut self, data: Value) -> Result<String> {
        self.turn_writes()?;
        ensure!(self.in_own_chat() || self.in_routine(), "artifact cards go only to the bot's own chat");
        let text = data["text"].as_str().unwrap_or_default().to_owned();
        self.close_seg();
        let entry =
            self.add_for_user(EntryKind::Agent, data).ok_or_else(|| anyhow!("couldn't save the artifact card"))?;
        self.record_completion_reply(&entry);
        self.files.summary = Some(text);
        self.set_activity("Working…");
        Ok(entry.id)
    }

    pub(super) fn finish_file(&mut self, completed: Completed) {
        self.files.pending.retain(|flag| !Arc::ptr_eq(flag, &completed.cancelled));
        let result = (|| {
            ensure!(
                completed.epoch == self.files.epoch
                    && !completed.cancelled.load(Ordering::Acquire)
                    && !completed.reply.is_closed(),
                "file sharing cancelled"
            );
            self.can_send_file()?;
            ensure!(self.hub.store.bot(&self.cfg.id)?.is_some_and(|bot| !bot.deleted), "bot was deleted");
            let mut snapshot = completed.result?;
            ensure!(
                completed.text.is_none() || snapshot.meta.plays_in_chat,
                "text goes only with a voice note: an mp3, m4a, wav or aac file up to 20 MiB"
            );
            self.close_seg();
            let text = match completed.text {
                Some(text) => self.post_voice_note(text, &snapshot, completed.quote)?,
                None => self.post_file(&snapshot, completed.quote)?,
            };
            snapshot.commit();
            self.files.summary = Some(text);
            self.set_activity("Working…");
            Ok(())
        })();
        let _ = completed.reply.send(result);
    }

    /// A file message: the card, with its name and size for a reader without one.
    fn post_file(&mut self, snapshot: &Snapshot, quote: Option<Value>) -> Result<String> {
        let text = format!("{} ({} bytes)", snapshot.meta.name, snapshot.meta.size);
        let data = with_quote(json!({"text": text, "final": true, "files": [snapshot.meta]}), quote);
        let saved =
            self.add_for_user(EntryKind::Agent, data).ok_or_else(|| anyhow!("couldn't save the file message"))?;
        self.record_completion_reply(&saved);
        Ok(text)
    }

    /// A message carrying a voice note: one bubble, the player over the text, sent like `send_message`.
    fn post_voice_note(&mut self, text: String, snapshot: &Snapshot, quote: Option<Value>) -> Result<String> {
        let data = with_quote(json!({"text": text, "final": true, "files": [snapshot.meta], "voiceNote": true}), quote);
        let saved = self.add_for_user(EntryKind::Agent, data);
        self.record_sent(saved, text.clone())?;
        Ok(text)
    }
}
