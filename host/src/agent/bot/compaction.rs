//! The bot's conversations as they fill up ([design](../../../../docs/design/bot-memory.md#compaction)): what the
//! agent reports about its context and its own compactions, and the compactions and rollovers the host starts. The
//! rules are `codync_mem::compaction`; this records what it needs and acts on what it decides.

use super::{Actor, Adapter, Done, Instructions, Queued};
use crate::chat::{context, handoff, memory};
use crate::store::{BotConfig, EntryKind, Lane, Store, now_ms};
use codync_mem::compaction::{self as rules, CacheLife, Conversation, Decision, Threshold};
use serde_json::{Value, json};

/// Context use below which a fall isn't read as a compaction: a short session has little to summarize.
const COMPACTED_FROM: u64 = 20_000;

/// Whether Codync summarizes the bot's chat ahead of time, so the apps offer the user a [`Threshold`] for it: its
/// agent is Claude Code or Codex, or a custom command turned out to run one of their adapters.
pub fn offers_threshold(store: &Store, cfg: &BotConfig) -> bool {
    !cfg.is_group()
        && match harness_adapter(&cfg.backend) {
            Adapter::Other => cfg.backend == "custom" && context_window(store, &cfg.id).is_some(),
            _ => true,
        }
}

/// The adapter a backend runs, before its agent has answered.
fn harness_adapter(backend: &str) -> Adapter {
    match backend {
        "claude" => Adapter::Claude,
        "codex" => Adapter::Codex,
        _ => Adapter::Other,
    }
}

/// The user's threshold where Codync compacts the bot's chat: by the adapter running it, or with none running (a
/// custom command released or not started yet) by what its agent last reported. Elsewhere the default: the agent's
/// own compaction, and no rollovers.
fn threshold(store: &Store, cfg: &BotConfig, running: Option<Adapter>) -> Threshold {
    let compacts = running.map_or_else(|| offers_threshold(store, cfg), |adapter| adapter != Adapter::Other);
    if compacts { cfg.compaction } else { Threshold::default() }
}

fn window_key(bot: &str) -> String {
    format!("context.window.{bot}")
}

/// The context window of the bot's own chat, as its agent last reported it; kept only for an agent Codync compacts.
pub fn context_window(store: &Store, bot: &str) -> Option<u64> {
    store.kv_get(&window_key(bot)).and_then(|v| v.parse().ok())
}

impl Actor {
    fn conversation_key(&self, session: &str) -> String {
        format!("compaction.{}.{session}", self.cfg.id)
    }

    fn conversation(&self, session: &str) -> Conversation {
        self.hub
            .store
            .kv_get(&self.conversation_key(session))
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    }

    fn save_conversation(&self, session: &str, c: &Conversation) {
        let saved = serde_json::to_string(c)
            .map_err(anyhow::Error::from)
            .and_then(|v| self.hub.store.kv_set(&self.conversation_key(session), &v));
        if let Err(error) = saved {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the conversation");
        }
    }

    fn cache_life_key(&self) -> String {
        format!("cache.life.{}", self.cfg.id)
    }

    /// How long the provider keeps this bot's prompts cached: learned from its turns, or guessed from its sign-in
    /// (Claude keeps an hour on a subscription, five minutes on an API key).
    fn cache_life(&self) -> CacheLife {
        if let Some(life) = self.hub.store.kv_get(&self.cache_life_key()).and_then(|v| serde_json::from_str(&v).ok()) {
            return life;
        }
        let api_key = crate::agent::auth::saved_env(&self.hub.store, &self.cfg.backend)
            .unwrap_or_default()
            .iter()
            .any(|k| k == "ANTHROPIC_API_KEY")
            || std::env::var_os("ANTHROPIC_API_KEY").is_some();
        if self.adapter() == Adapter::Claude && !api_key { CacheLife::OneHour } else { CacheLife::FiveMinutes }
    }

    /// The bot's own chat, not a room, a routine or another bot's request: the conversation the user comes
    /// back to, and the only one compacted ahead of time.
    fn in_main_chat(&self) -> bool {
        self.lane.chat == self.cfg.id
            && self.active_group.is_none()
            && self.active_routine.is_none()
            && self.active_request.is_none()
            && self.turn_session.is_some()
            && self.turn_session == self.session_id
    }

    /// The adapter the bot's agent runs; before it runs, its harness's.
    fn adapter(&self) -> Adapter {
        self.conn.as_ref().map_or_else(|| harness_adapter(&self.cfg.backend), |c| c.adapter)
    }

    /// The command that makes the agent compact, with the keep-list where it takes one.
    fn compact_command(&self) -> Option<String> {
        match self.adapter() {
            Adapter::Claude => Some(format!("/compact {}", rules::KEEP)),
            Adapter::Codex => Some("/compact".into()),
            Adapter::Other => None,
        }
    }

    fn threshold(&self) -> Threshold {
        threshold(&self.hub.store, &self.cfg, self.conn.as_ref().map(|c| c.adapter))
    }

    /// Keeps the main chat's window for the apps, which offer sizes up to what it holds; none for an agent Codync
    /// doesn't compact. Only the bot's own agent tells it: the sizes are for that agent's window, not a fallback's.
    fn note_window(&self, size: u64) {
        if self.cfg.backend != self.base.backend {
            return;
        }
        let window = match (self.adapter(), size) {
            (Adapter::Other, _) => None,
            (_, 0) => return,
            (_, size) => Some(size),
        };
        if context_window(&self.hub.store, &self.cfg.id) == window {
            return;
        }
        let key = window_key(&self.cfg.id);
        let saved = match window {
            Some(size) => self.hub.store.kv_set(&key, &size.to_string()),
            None => self.hub.store.kv_delete(&key),
        };
        if let Err(error) = saved {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the window");
            return;
        }
        self.hub.emit_bot(&self.cfg.id);
    }

    /// The agent reported its context use.
    pub(super) fn track_usage(&mut self, used: u64, size: u64) {
        let Some(sid) = self.turn_session.clone() else { return };
        let Some(conn) = self.conn.as_mut() else { return };
        let before = conn.usage.insert(sid, (used, size)).map(|(used, _)| used);
        if !conn.reports_compaction && compacted_by_usage(before, used) {
            self.compacted(&format!("usage-{}", now_ms()), None);
        }
    }

    /// The agent summarized its context: the next turn gets freshly rendered instructions and memory (see
    /// `context`), a harness that got them in its first message gets them again, and the chat shows the summary.
    pub(super) fn compacted(&mut self, id: &str, summary: Option<String>) {
        let Some(sid) = self.turn_session.clone() else { return };
        match context::bump_epoch(&self.hub.store, &self.cfg.id, &sid, id) {
            Ok(true) => {}
            // The same compaction reported again; Claude's adapter carries the summary on one of its reports.
            Ok(false) => {
                if let Some(summary) = summary {
                    self.keep_summary(&sid, &summary);
                }
                return;
            }
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the compaction");
                return;
            }
        }
        if let Err(error) = memory::compacted(&self.hub.store, &self.cfg.id, &sid) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the compaction");
        }
        if self.conn.as_ref().is_some_and(|c| c.instructions() == Instructions::FirstMessage) {
            self.profile_resend.insert(sid.clone());
        }
        let mut c = self.conversation(&sid);
        c.compacted(self.compacting.is_some(), self.compacting == Some(true), now_ms());
        self.save_conversation(&sid, &c);
        let _ = self.dreamer.send(memory::DreamEvent::Flush);
        let data = json!({
            "text": "Earlier context was summarized to make room.",
            "style": "info",
            "compaction": {"summary": Value::Null, "byHost": self.compacting.is_some()},
        });
        self.compaction_notice = self.add(EntryKind::Notice, self.turn.unwrap_or(0), data).map(|e| e.id);
        if let Some(summary) = summary {
            self.keep_summary(&sid, &summary);
        }
    }

    /// The harness's summary: shown on the compaction's line in the chat, and kept for a later handoff.
    fn keep_summary(&mut self, session: &str, summary: &str) {
        if let Err(error) = self.hub.store.kv_set(&handoff::summary_key(&self.cfg.id, session), summary) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't keep the summary");
        }
        if let Some(id) = &self.compaction_notice
            && let Some(mut entry) = self.hub.store.entry(id)
        {
            entry.data["compaction"]["summary"] = summary.into();
            self.hub.set_entry(id, &entry.data);
        }
    }

    /// After a turn in the main chat: record it, learn the cache's life from it, and compact or prepare a rollover
    /// when that's due.
    pub(super) fn after_turn(&mut self, done: &Done) {
        let compacting = self.compacting.take();
        if let Ok(answer) = done
            && answer["usage"].is_object()
        {
            // How well the prompt cache held: the first thing to look at when a turn costs more than it should.
            let usage = &answer["usage"];
            tracing::info!(
                bot = %self.cfg.id,
                session = self.turn_session.as_deref().unwrap_or_default(),
                cached_read = usage["cachedReadTokens"].as_u64().unwrap_or(0),
                cached_write = usage["cachedWriteTokens"].as_u64().unwrap_or(0),
                input = usage["inputTokens"].as_u64().unwrap_or(0),
                "turn usage"
            );
        }
        if !self.in_main_chat() {
            return;
        }
        let Some(sid) = self.turn_session.clone() else { return };
        let now = now_ms();
        let mut c = self.conversation(&sid);
        if let Some(&(used, size)) = self.conn.as_ref().and_then(|conn| conn.usage.get(&sid)) {
            c.usage(used, size);
            self.note_window(size);
        }
        // The compaction turn itself is no turn of the conversation: it neither counts nor resets the idle clock.
        if compacting.is_some() {
            self.save_conversation(&sid, &c);
            return;
        }
        if let Ok(answer) = done
            && let Some(life) = learned_life(answer, now - c.last_turn)
            && let Err(error) = self.hub.store.kv_set(&self.cache_life_key(), &json!(life).to_string())
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the cache's life");
        }
        c.turn_ended(now);
        self.save_conversation(&sid, &c);
        match rules::at_turn_end(&c, self.threshold(), now) {
            Decision::Compact if self.compact_command().is_some() => {
                self.queue.push_back(Queued::Compact { idle: false });
            }
            Decision::Rollover => self.draft_handoff(&sid, handoff::Reason::Full),
            _ => {}
        }
    }

    /// While the bot is at rest: compact before an hour-long cache expires, and once the next message should start
    /// a new conversation, put the handoff in the chat for the user to review.
    pub(super) fn tend_idle(&mut self) {
        let Some(sid) = self.session_id.clone() else { return };
        let c = self.settle_kept(&sid);
        if c.last_turn == 0 {
            return;
        }
        let now = now_ms();
        let life = self.cache_life();
        let at = self.threshold();
        if rules::while_idle(&c, at, life, now) == Decision::Compact && self.compact_command().is_some() {
            self.queue.push_back(Queued::Compact { idle: true });
        } else if rules::rollover_due(&c, at, life, now) && handoff::pending(&self.hub.store, &self.cfg.id).is_none() {
            let reason = if now - c.last_turn >= life.ms() { handoff::Reason::Cold } else { handoff::Reason::Away };
            self.draft_handoff(&sid, reason);
        }
    }

    /// The conversation, with the user's choice to carry on over its last handoff card recorded.
    fn settle_kept(&self, session: &str) -> Conversation {
        let mut c = self.conversation(session);
        if handoff::kept(&self.hub.store, &self.cfg.id) {
            c.rollover_declined = true;
            self.save_conversation(session, &c);
        }
        c
    }

    /// The user changed when the chat is summarized: a fresh conversation the new setting doesn't call for is
    /// withdrawn, and the old one goes on.
    pub(super) fn threshold_changed(&self) {
        let Some(sid) = &self.session_id else { return };
        let Some((card, _)) = handoff::pending(&self.hub.store, &self.cfg.id) else { return };
        let (c, at, now) = (self.conversation(sid), self.threshold(), now_ms());
        if !rules::rollover_due(&c, at, self.cache_life(), now) && rules::at_turn_end(&c, at, now) != Decision::Rollover
        {
            handoff::withdraw(&self.hub, &self.cfg.id, &card);
        }
    }

    fn draft_handoff(&mut self, session: &str, reason: handoff::Reason) {
        if handoff::pending(&self.hub.store, &self.cfg.id).is_some() {
            return;
        }
        if let Err(error) = handoff::draft(&self.hub, &self.cfg, session, reason) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't prepare the handoff");
        }
    }

    /// Runs a compaction the host decided on, as a hidden turn in the main chat. Dreaming goes first, and Claude's
    /// opening is rendered again, so the compaction keeps current memory (render, load, compact).
    pub(super) async fn start_compaction(&mut self, idle: bool, done_tx: &tokio::sync::mpsc::UnboundedSender<Done>) {
        let (Some(sid), Some(command)) = (self.session_id.clone(), self.compact_command()) else { return };
        // Decided when it was queued; another turn or a change of the setting may have come first.
        let (c, at, now) = (self.conversation(&sid), self.threshold(), now_ms());
        let due =
            if idle { rules::while_idle(&c, at, self.cache_life(), now) } else { rules::at_turn_end(&c, at, now) };
        if due != Decision::Compact {
            return;
        }
        crate::analytics::context_compacted(&self.hub, &self.cfg.backend, idle, at);
        let _ = self.dreamer.send(memory::DreamEvent::Flush);
        if let Err(error) = context::refresh(&self.hub.store, &self.cfg.id, &sid) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't refresh the opening");
        }
        self.compacting = Some(idle);
        self.raw_prompt = true;
        if let Err(e) = self.start_turn(Lane::main(&self.cfg.id), &[], &command, true, done_tx).await {
            self.compacting = None;
            self.start_failed(&e);
        }
    }

    /// Before the user's message in the main chat: a handoff waiting for it starts a new conversation. Returns the
    /// handoff to put before the message.
    pub(super) fn take_handoff(&mut self) -> Option<String> {
        // A rollover that came due since the last tick gets its card now, without the chance to edit it.
        if let Some(sid) = self.session_id.clone()
            && handoff::pending(&self.hub.store, &self.cfg.id).is_none()
        {
            let c = self.settle_kept(&sid);
            let life = self.cache_life();
            if rules::rollover_due(&c, self.threshold(), life, now_ms()) {
                let reason =
                    if now_ms() - c.last_turn >= life.ms() { handoff::Reason::Cold } else { handoff::Reason::Away };
                self.draft_handoff(&sid, reason);
            }
        }
        let (id, text) = handoff::pending(&self.hub.store, &self.cfg.id)?;
        handoff::sent(&self.hub, &self.cfg.id, &id);
        self.forget_session();
        Some(text)
    }
}

/// What a turn's prompt usage says about the cache's life, `gap_ms` after the previous turn.
fn learned_life(answer: &Value, gap_ms: i64) -> Option<CacheLife> {
    let usage = &answer["usage"];
    let read = usage["cachedReadTokens"].as_u64()?;
    let input = read + usage["cachedWriteTokens"].as_u64().unwrap_or(0) + usage["inputTokens"].as_u64().unwrap_or(0);
    CacheLife::observed(gap_ms, read, input)
}

/// Whether a session's context use fell the way it does when the agent summarizes it: by more than half.
fn compacted_by_usage(before: Option<u64>, used: u64) -> bool {
    before.is_some_and(|before| before >= COMPACTED_FROM && used < before / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_halved_context_reads_as_a_compaction() {
        assert!(compacted_by_usage(Some(90_000), 12_000));
        assert!(!compacted_by_usage(Some(90_000), 60_000), "ordinary churn");
        assert!(!compacted_by_usage(Some(8_000), 1_000), "too little to summarize");
        assert!(!compacted_by_usage(None, 1_000), "the session's first report");
    }

    #[test]
    fn a_custom_command_keeps_its_threshold_while_no_agent_runs() {
        let path = std::env::temp_dir().join(format!("codync-threshold-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        let cfg: BotConfig =
            serde_json::from_value(json!({"id": "b", "name": "B", "backend": "custom", "compaction": "nearLimit"}))
                .unwrap();
        assert_eq!(threshold(&store, &cfg, None), Threshold::Off, "not known to run Claude's or Codex's adapter yet");
        store.kv_set(&window_key("b"), "1000000").unwrap();
        assert_eq!(threshold(&store, &cfg, None), Threshold::NearLimit, "released after it reported its window");
        assert_eq!(threshold(&store, &cfg, Some(Adapter::Claude)), Threshold::NearLimit);
        assert_eq!(threshold(&store, &cfg, Some(Adapter::Other)), Threshold::Off, "now running another adapter");
        let cold = Conversation { used: 150_000, size: 1_000_000, last_turn: 1, ..Conversation::default() };
        assert!(!rules::rollover_due(&cold, threshold(&store, &cfg, None), CacheLife::FiveMinutes, 2 * 3_600_000));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_turn_after_a_pause_tells_the_cache_life() {
        let warm = json!({"usage": {"inputTokens": 2, "cachedReadTokens": 40_000, "cachedWriteTokens": 300}});
        assert_eq!(learned_life(&warm, 20 * 60_000), Some(CacheLife::OneHour));
        let cold = json!({"usage": {"inputTokens": 2, "cachedReadTokens": 0, "cachedWriteTokens": 40_000}});
        assert_eq!(learned_life(&cold, 20 * 60_000), Some(CacheLife::FiveMinutes));
        assert_eq!(learned_life(&json!({"stopReason": "end_turn"}), 20 * 60_000), None, "no usage reported");
    }
}
