//! A scheduled check-in is private until it has a useful result for the user.

use super::Actor;
use crate::store::EntryKind;
use serde_json::Value;

pub(super) fn is_silent(text: &str) -> bool {
    text.trim().is_empty() || text.trim() == "NO_UPDATE" || crate::chat::group::is_pass(text)
}

pub(super) fn is_proactive(data: &Value) -> bool {
    data["from"] == "proactive"
}

pub(super) fn silent_update(scheduled: bool, text: Option<&str>) -> bool {
    scheduled && text.is_none_or(is_silent)
}

pub(super) fn internal_step(kind: EntryKind) -> bool {
    matches!(kind, EntryKind::Agent | EntryKind::Thought | EntryKind::Tool | EntryKind::Plan)
}

fn useful_output(data: &Value) -> bool {
    data["files"].as_array().is_some_and(|files| !files.is_empty())
        || data["artifact"].is_object()
        || data["text"].as_str().is_some_and(|text| !is_silent(text))
}

impl Actor {
    pub(super) fn proactive_input(&self, id: &str) -> bool {
        self.hub.store.entry(id).is_some_and(|entry| is_proactive(&entry.data))
    }

    pub(super) fn proactive_batch(&self, ids: &[String]) -> bool {
        !ids.is_empty() && ids.iter().all(|id| self.proactive_input(id))
    }

    /// Explicit sends and final replies are buffered equally, so even streamed `NO_UPDATE` is never visible.
    pub(super) fn publish_proactive_output(&self, succeeded: bool) {
        if !self.proactive_turn || !succeeded {
            return;
        }
        let Some(turn) = self.turn else { return };
        let entries = match self.hub.store.proactive_outputs(&self.lane.chat, turn) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't read scheduled output");
                return;
            }
        };
        let entries: Vec<_> = entries.into_iter().filter(|entry| useful_output(&entry.data)).collect();
        // Announce all useful artifact metadata first, even when the same artifact has several cards this turn.
        for entry in &entries {
            if let Some(id) = entry.data["artifact"]["id"].as_str()
                && let Err(error) = crate::chat::artifacts::promote_card(&self.hub, &self.cfg.id, id, &entry.id)
            {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't publish scheduled artifact");
                return;
            }
        }
        for entry in entries {
            if let Err(error) = self.hub.edit_entry(&entry.id, |current| {
                current.data["internal"] = false.into();
                // A useful file or artifact can accompany a silent sentinel; show the asset, never the sentinel.
                if current.data["text"].as_str().is_some_and(is_silent) {
                    current.data["text"] = "".into();
                }
                Ok(true)
            }) {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't publish scheduled output");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_update_is_exact_and_only_the_scheduled_turn_treats_it_as_silence() {
        assert!(is_silent(" NO_UPDATE\n"));
        assert!(is_silent(""));
        assert!(is_silent("(pass)"));
        assert!(!is_silent("NO_UPDATE: waiting for approval"));
        assert!(!is_silent("Your build passed."));
        assert!(!is_proactive(&json!({"text":"NO_UPDATE"})));
        assert!(silent_update(true, Some("NO_UPDATE")));
        assert!(silent_update(true, None));
        assert!(!silent_update(true, Some("Build passed")));
        assert!(!silent_update(false, Some("NO_UPDATE")), "an ordinary turn keeps its own notification rules");
    }

    #[test]
    fn pending_approvals_and_errors_stay_actionable_while_steps_are_internal() {
        assert!(internal_step(EntryKind::Agent));
        assert!(internal_step(EntryKind::Thought));
        assert!(internal_step(EntryKind::Tool));
        assert!(internal_step(EntryKind::Plan));
        assert!(!internal_step(EntryKind::Permission));
        assert!(!internal_step(EntryKind::Notice));
        assert!(!useful_output(&json!({"text":"NO_UPDATE"})));
        assert!(useful_output(&json!({"text":"NO_UPDATE", "files":[{"id":"file"}]})));
    }
}
