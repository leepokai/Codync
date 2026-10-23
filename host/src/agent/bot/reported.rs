//! What the bot's main agent session itself says its settings are (the bot payload's `sessionOptions` and
//! `sessionSettings`): only that session's own word, never another session's, a saved choice or a setter that
//! succeeded without saying what it set.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Reported {
    /// The session the report is about; `None`: no live session, nothing is known.
    session: Option<String>,
    /// Setting id -> value, as the session last listed it. Absent ids are unknown.
    values: BTreeMap<String, String>,
    /// The settings the session offers for the model it runs; `None` until it lists them.
    settings: Option<Vec<Value>>,
}

impl Reported {
    /// The main session `sid` was opened or loaded and answered `answer`.
    pub(super) fn opened(&mut self, _sid: &str, _answer: &Value) {}

    /// A later answer or `config_option_update` from session `sid`.
    pub(super) fn listed(&mut self, _sid: &str, _answer: &Value) {}

    /// Session `sid` took these settings without listing what it set: they are no longer known. A model change
    /// without a list makes everything unknown.
    pub(super) fn unconfirmed(&mut self, _sid: &str, _ids: &[String], _model_changed: bool) {}

    /// Keeps the report only while its session is live (`live` says whether a session is).
    pub(super) fn settle(&mut self, _live: impl Fn(&str) -> bool) {}

    /// The payload's `sessionOptions` (`None`: no live session) and `sessionSettings` (`None`: not listed).
    pub(super) fn wire(&self) -> (Option<BTreeMap<String, String>>, Option<Vec<Value>>) {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listing(effort: &str) -> Value {
        json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "currentValue": "sol", "options": [{"value": "sol"}]},
            {"id": "effort", "category": "thought_level", "type": "select", "currentValue": effort,
             "options": [{"value": "low"}, {"value": "high"}]},
        ]})
    }

    fn values(pairs: &[(&str, &str)]) -> Option<BTreeMap<String, String>> {
        Some(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }

    #[test]
    fn only_the_main_sessions_own_word_counts() {
        let mut r = Reported::default();
        assert_eq!(r.wire(), (None, None), "nothing before a session");
        r.opened("main", &listing("high"));
        assert_eq!(r.wire().0, values(&[("effort", "high"), ("model", "sol")]));
        assert_eq!(r.wire().1.unwrap()[0]["id"], "effort");
        // Another session (a routine's, or a late update for it) says nothing about this one.
        r.listed("routine", &listing("low"));
        assert_eq!(r.wire().0, values(&[("effort", "high"), ("model", "sol")]));
        r.listed("main", &listing("low"));
        assert_eq!(r.wire().0, values(&[("effort", "low"), ("model", "sol")]));
        // An answer without a list leaves the report as it is.
        r.listed("main", &json!({}));
        assert_eq!(r.wire().0, values(&[("effort", "low"), ("model", "sol")]));
    }

    #[test]
    fn a_session_that_lists_nothing_is_live_but_unknown() {
        let mut r = Reported::default();
        r.opened("old", &listing("high"));
        r.opened("new", &json!({"sessionId": "new"}));
        assert_eq!(r.wire(), (values(&[]), None), "not the old session's values");
    }

    #[test]
    fn a_setter_without_a_list_makes_its_setting_unknown() {
        let mut r = Reported::default();
        r.opened("main", &listing("low"));
        r.unconfirmed("main", &["effort".to_owned()], false);
        assert_eq!(r.wire().0, values(&[("model", "sol")]), "low was before the change");
        r.unconfirmed("elsewhere", &["model".to_owned()], false);
        assert_eq!(r.wire().0, values(&[("model", "sol")]));
        r.unconfirmed("main", &[], true);
        assert_eq!(r.wire(), (values(&[]), None), "another model: nothing listed is known");
    }

    #[test]
    fn a_report_ends_with_its_session() {
        let mut r = Reported::default();
        r.opened("main", &listing("high"));
        r.settle(|sid| sid == "main");
        assert_eq!(r.wire().0, values(&[("effort", "high"), ("model", "sol")]));
        // Closed, released, or the agent exited: no live session.
        r.settle(|_| false);
        assert_eq!(r.wire(), (None, None));
    }
}
