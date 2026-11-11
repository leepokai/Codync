//! The event triggers an editor may offer, and the filters each source takes.
//!
//! The host keeps the catalog so desktop, Android and the bot describe the same events. Stored
//! triggers stay valid when a source later loses an event: [`validate`] is used by the editor's
//! save, not by [`super::Trigger::validate`], which must keep older triggers savable.

use anyhow::{Result, bail};
use serde_json::{Value, json};

pub struct Filter {
    pub key: &'static str,
    /// The field's label, translated by the clients.
    pub label: &'static str,
    /// What to type, shown under the field.
    pub hint: &'static str,
}

pub struct Event {
    /// The `event` a payload carries: `<name>.<action>`, or `*` for every one.
    pub id: &'static str,
    pub label: &'static str,
}

pub struct Source {
    pub id: &'static str,
    pub label: &'static str,
    pub events: &'static [Event],
    pub filters: &'static [Filter],
}

const ANY: Event = Event { id: "*", label: "Any event" };
const MESSAGE_CONTAINS: Filter =
    Filter { key: "messageContains", label: "Text contains", hint: "Only when the text has this" };

pub const SOURCES: &[Source] = &[
    Source {
        id: "github",
        label: "GitHub",
        events: &[
            ANY,
            Event { id: "issues.opened", label: "Issue opened" },
            Event { id: "issues.closed", label: "Issue closed" },
            Event { id: "issue_comment.created", label: "Comment on an issue" },
            Event { id: "pull_request.opened", label: "Pull request opened" },
            Event { id: "pull_request.closed", label: "Pull request closed" },
            Event { id: "pull_request_review.submitted", label: "Pull request review" },
            Event { id: "pull_request_review_comment.created", label: "Comment on a review" },
            Event { id: "check_run.completed", label: "Checks finished" },
            Event { id: "push", label: "Push" },
            Event { id: "release.published", label: "Release published" },
            Event { id: "discussion.created", label: "Discussion opened" },
        ],
        filters: &[
            Filter { key: "repo", label: "Repository", hint: "owner/name" },
            Filter { key: "sender", label: "Sender", hint: "A GitHub login" },
            Filter { key: "conclusion", label: "Conclusion", hint: "failure for failed checks" },
            MESSAGE_CONTAINS,
        ],
    },
    Source {
        id: "slack",
        label: "Slack",
        events: &[ANY, Event { id: "app_mention", label: "Mention" }],
        filters: &[
            Filter { key: "channel", label: "Channel", hint: "A channel id, such as C0123ABCD" },
            Filter { key: "user", label: "Sender", hint: "A Slack user id" },
            MESSAGE_CONTAINS,
        ],
    },
    Source {
        id: "telegram",
        label: "Telegram",
        events: &[ANY, Event { id: "mention", label: "Mention" }],
        filters: &[
            Filter { key: "chat", label: "Chat", hint: "A chat id" },
            Filter { key: "user", label: "Sender", hint: "A Telegram user id" },
            MESSAGE_CONTAINS,
        ],
    },
    Source {
        id: "discord",
        label: "Discord",
        events: &[
            ANY,
            Event { id: "message.created", label: "Message posted" },
            Event { id: "message.mention", label: "Mention" },
        ],
        filters: &[
            Filter { key: "channel", label: "Channel", hint: "A channel id" },
            Filter { key: "user", label: "Sender", hint: "A Discord user id" },
            MESSAGE_CONTAINS,
        ],
    },
    Source {
        id: "whatsapp",
        label: "WhatsApp",
        events: &[
            ANY,
            Event { id: "message", label: "Message" },
            Event { id: "groupMention", label: "Mention in a group" },
        ],
        filters: &[
            Filter { key: "chat", label: "Chat", hint: "A chat id, such as 5215555555555@s.whatsapp.net" },
            Filter { key: "from", label: "Sender", hint: "A WhatsApp id" },
            MESSAGE_CONTAINS,
        ],
    },
];

pub fn source(id: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|source| source.id == id)
}

/// Why `source`, `event` and `filters` can't be saved, or `None` when they can.
pub fn problem(source_id: &str, event: &str, filters: &std::collections::BTreeMap<String, String>) -> Option<String> {
    let Some(source) = source(source_id) else {
        // A source the catalog doesn't know (an older or bot-set trigger) still saves: its event and
        // filters can't be checked here, so they pass through.
        if source_id.is_empty() || event.is_empty() {
            return Some("Choose a service for this trigger.".into());
        }
        return filters
            .iter()
            .find(|(key, value)| key.trim().is_empty() || value.trim().is_empty())
            .map(|(key, _)| format!("The {key} filter is empty."));
    };
    if !source.events.iter().any(|known| known.id == event) {
        return Some(format!("Choose an event {} can send.", source.label));
    }
    if filters.len() > 20 {
        return Some("Use at most 20 filters.".into());
    }
    for (key, value) in filters {
        if !source.filters.iter().any(|filter| filter.key == key) {
            return Some(format!("{} has no filter named {key}.", source.label));
        }
        if value.trim().is_empty() || value.len() > 4000 {
            return Some(format!("The {key} filter is empty or too long."));
        }
    }
    None
}

/// Checks [`problem`] for a save or a preview.
pub fn validate(source_id: &str, event: &str, filters: &std::collections::BTreeMap<String, String>) -> Result<()> {
    match problem(source_id, event, filters) {
        Some(why) => bail!("{why}"),
        None => Ok(()),
    }
}

/// Whether a filter matches `payload`; the special keys read where the services put them.
pub fn matches(payload: &Value, key: &str, value: &str) -> bool {
    match key {
        "messageContains" => payload["text"].as_str().is_some_and(|text| text.contains(value)),
        "conclusion" => {
            ["check_run", "check_suite"].iter().any(|kind| payload[*kind]["conclusion"].as_str() == Some(value))
        }
        _ => payload[key].as_str() == Some(value),
    }
}

/// The catalog the editors offer.
pub fn catalog() -> Value {
    json!({
        "sources": SOURCES.iter().map(|source| json!({
            "id": source.id,
            "label": source.label,
            "events": source.events.iter().map(|event| json!({"id": event.id, "label": event.label})).collect::<Vec<_>>(),
            "filters": source.filters.iter().map(|filter| json!({"key": filter.key, "label": filter.label, "hint": filter.hint})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// A readable trigger line for `source` and `event`, when the catalog knows them.
pub fn describe(source_id: &str, event: &str) -> Option<String> {
    let source = source(source_id)?;
    let event = source.events.iter().find(|known| known.id == event)?;
    Some(format!("When {}: {}", source.label, event.label.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn every_source_offers_any_event_and_known_filters() {
        for source in SOURCES {
            assert!(source.events.iter().any(|event| event.id == "*"), "{}", source.id);
            assert!(!source.filters.is_empty(), "{}", source.id);
        }
        assert!(source("github").is_some());
        assert!(source("nope").is_none());
    }

    #[test]
    fn validate_names_the_broken_field() {
        let empty = BTreeMap::new();
        assert!(validate("github", "issues.opened", &empty).is_ok());
        assert!(validate("github", "issues.assigned", &empty).is_err());
        assert!(validate("github", "issues.opened", &BTreeMap::from([("channel".into(), "C1".into())])).is_err());
        assert!(validate("github", "issues.opened", &BTreeMap::from([("repo".into(), " ".into())])).is_err());
        assert!(validate("slack", "*", &BTreeMap::from([("channel".into(), "C1".into())])).is_ok());
        assert!(
            validate("whatsapp", "message", &BTreeMap::from([("chat".into(), "5215555555555@s.whatsapp.net".into())]))
                .is_ok()
        );
        // A source outside the catalog keeps its event and filters as they are.
        assert!(validate("origin", "pull_request.opened", &BTreeMap::from([("repo".into(), "o/r".into())])).is_ok());
        assert!(validate("origin", "", &empty).is_err());
        assert!(validate("pagerduty", "incident.triggered", &empty).is_ok());
    }

    #[test]
    fn filters_read_the_payload_where_the_service_puts_them() {
        let failure = json!({"text": "Build failed", "repo": "o/r", "check_run": {"conclusion": "failure"}});
        assert!(matches(&failure, "repo", "o/r"));
        assert!(matches(&failure, "messageContains", "failed"));
        assert!(matches(&failure, "conclusion", "failure"));
        assert!(!matches(&failure, "conclusion", "success"));
        assert!(!matches(&failure, "repo", "o/other"));
    }

    #[test]
    fn description_uses_the_catalog() {
        assert_eq!(describe("github", "pull_request.opened").as_deref(), Some("When GitHub: pull request opened"));
        assert!(describe("github", "issues.assigned").is_none());
    }
}
