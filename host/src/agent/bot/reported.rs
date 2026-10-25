//! What the bot's main agent session itself says its settings are (the bot payload's `sessionOptions` and
//! `sessionSettings`): only that session's own word, never another session's, a saved choice or a setter that
//! succeeded without saying what it set.

use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Reported {
    /// The session the report is about; `None`: no live session, nothing is known.
    session: Option<String>,
    /// The agent process that opened it: another process loading the same session id hasn't said anything yet.
    process: u64,
    /// Setting id -> value, as the session last listed it. Absent ids are unknown.
    values: BTreeMap<String, String>,
    /// The settings the session offers for the model it runs; `None` until it lists them.
    settings: Option<Vec<Value>>,
    /// Saved settings that didn't take, by id: `not_offered` (left out, never sent) or `rejected` (the agent refused
    /// it). They outlive the session they happened in, until the next one opens or the bot's settings change.
    issues: BTreeMap<String, (&'static str, String)>,
    /// The agent `issues` happened on: another agent's editor never shows them.
    issues_agent: Option<String>,
}

impl Reported {
    /// The main session `sid` was opened or loaded by agent process `process` (`Conn::process`) and answered
    /// `answer`. What an earlier session said doesn't carry over: a session that lists nothing is live, but nothing
    /// about it is known.
    pub(super) fn opened(&mut self, sid: &str, process: u64, answer: &Value) {
        *self = Self { session: Some(sid.to_owned()), process, ..Self::default() };
        self.listed(sid, answer);
    }

    /// A later answer or `config_option_update` from session `sid`.
    pub(super) fn listed(&mut self, sid: &str, answer: &Value) {
        if self.session.as_deref() != Some(sid) || !answer["configOptions"].is_array() {
            return;
        }
        self.values = crate::agent::config::current_values(answer);
        // What each setting is set to is `values`; the list says only what can be picked.
        let mut offered = crate::agent::auth::session_options(answer);
        for setting in &mut offered {
            setting.as_object_mut().map(|s| s.remove("default"));
        }
        self.settings = Some(offered);
    }

    /// Session `sid` took these settings without listing what it set: they are no longer known. A model change
    /// without a list makes everything unknown.
    pub(super) fn unconfirmed(&mut self, sid: &str, ids: &[String], model_changed: bool) {
        if self.session.as_deref() != Some(sid) {
            return;
        }
        if model_changed {
            self.values.clear();
            self.settings = None;
        }
        for id in ids {
            self.values.remove(id);
        }
    }

    /// Settings left out of the session just opened (never sent).
    pub(super) fn left_out(&mut self, agent: &str, settings: &[(String, String)]) {
        self.bind(agent);
        self.issues.extend(settings.iter().map(|(id, value)| (id.clone(), ("not_offered", value.clone()))));
    }

    /// The agent refused setting `id`: the session didn't start.
    pub(super) fn rejected(&mut self, agent: &str, id: &str, value: &str) {
        self.bind(agent);
        self.issues.insert(id.to_owned(), ("rejected", value.to_owned()));
    }

    /// Issues are about `agent` from now on: any about another agent no longer apply.
    fn bind(&mut self, agent: &str) {
        if self.issues_agent.as_deref() != Some(agent) {
            self.issues.clear();
            self.issues_agent = Some(agent.to_owned());
        }
    }

    /// The bot runs on `agent` now (its own, or a fallback): issues another agent had are dropped, as after Switch
    /// back, where its own never skipped them.
    pub(super) fn on_agent(&mut self, agent: &str) {
        if self.issues_agent.as_deref().is_some_and(|a| a != agent) {
            self.issues.clear();
            self.issues_agent = None;
        }
    }

    /// The bot's saved settings changed: what went wrong with the old ones no longer applies.
    pub(super) fn settings_changed(&mut self) {
        self.issues.clear();
    }

    /// The payload's `settingIssues`: setting id -> `{issue, value}`, the value it was about.
    pub(super) fn issues(&self) -> Value {
        let issues =
            self.issues.iter().map(|(id, (issue, value))| (id.clone(), json!({"issue": issue, "value": value})));
        Value::Object(issues.collect())
    }

    /// Whether the main session is running (`sessionLive`), whatever it listed.
    pub(super) fn live(&self) -> bool {
        self.session.is_some()
    }

    /// Keeps the report only while its session is live in the process that opened it (`live(sid, process)`).
    pub(super) fn settle(&mut self, live: impl Fn(&str, u64) -> bool) {
        if self.session.as_deref().is_some_and(|sid| !live(sid, self.process)) {
            let (issues, issues_agent) = (std::mem::take(&mut self.issues), self.issues_agent.take());
            *self = Self { issues, issues_agent, ..Self::default() };
        }
    }

    /// The payload's `sessionOptions` (`None`: no live session) and `sessionSettings` (`None`: not listed).
    pub(super) fn wire(&self) -> (Option<BTreeMap<String, String>>, Option<Vec<Value>>) {
        match self.session {
            Some(_) => (Some(self.values.clone()), self.settings.clone()),
            None => (None, None),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn what_went_wrong_outlives_the_session_until_the_settings_or_session_change() {
        let mut r = Reported::default();
        r.rejected("claude", "effort", "max");
        r.settle(|_, _| false);
        assert_eq!(
            r.issues(),
            serde_json::json!({"effort": {"issue": "rejected", "value": "max"}}),
            "the session never started; the issue names the value it was about"
        );
        r.settings_changed();
        assert_eq!(r.issues(), serde_json::json!({}), "a new choice isn't the refused one");
        r.opened("main", 1, &serde_json::json!({}));
        r.left_out("claude", &[("effort".to_owned(), "high".to_owned())]);
        assert_eq!(r.issues(), serde_json::json!({"effort": {"issue": "not_offered", "value": "high"}}));
        r.opened("next", 1, &serde_json::json!({}));
        assert_eq!(r.issues(), serde_json::json!({}), "a new session starts over");
    }

    #[test]
    fn what_went_wrong_stays_with_the_agent_it_happened_on() {
        // Greptile on #204 (1644a28f): a fallback's skipped effort showed in the main agent's editor after Switch back.
        let mut r = Reported::default();
        r.opened("fallback", 1, &serde_json::json!({}));
        r.left_out("codex", &[("effort".to_owned(), "high".to_owned())]);
        r.on_agent("codex");
        assert_eq!(r.issues(), serde_json::json!({"effort": {"issue": "not_offered", "value": "high"}}));
        r.on_agent("claude");
        assert_eq!(r.issues(), serde_json::json!({}), "the main agent never skipped it");
        r.rejected("claude", "effort", "max");
        r.settle(|_, _| false);
        r.on_agent("claude");
        assert_eq!(r.issues(), serde_json::json!({"effort": {"issue": "rejected", "value": "max"}}), "its own stays");
    }

    #[test]
    fn live_comes_from_the_session_not_from_what_it_listed() {
        let mut r = Reported::default();
        assert!(!r.live());
        r.opened("main", 1, &serde_json::json!({"sessionId": "main"}));
        assert!(r.live(), "a session that lists nothing is still running");
        r.settle(|_, _| false);
        assert!(!r.live());
    }

    use super::*;
    use serde_json::json;

    fn listing(effort: &str) -> Value {
        json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "currentValue": "sol", "options": [{"value": "sol"}]},
            {"id": "effort", "category": "thought_level", "type": "select", "currentValue": effort,
             "options": [{"value": "low"}, {"value": "high"}]},
        ]})
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn only_the_main_sessions_own_word_counts() {
        let mut r = Reported::default();
        assert_eq!(r.wire(), (None, None), "nothing before a session");
        r.opened("main", 1, &listing("high"));
        assert_eq!(r.wire().0, Some(values(&[("effort", "high"), ("model", "sol")])));
        assert_eq!(r.wire().1.unwrap()[0]["id"], "effort");
        // Another session (a routine's, or a late update for it) says nothing about this one.
        r.listed("routine", &listing("low"));
        assert_eq!(r.wire().0, Some(values(&[("effort", "high"), ("model", "sol")])));
        r.listed("main", &listing("low"));
        assert_eq!(r.wire().0, Some(values(&[("effort", "low"), ("model", "sol")])));
        // An answer without a list leaves the report as it is.
        r.listed("main", &json!({}));
        assert_eq!(r.wire().0, Some(values(&[("effort", "low"), ("model", "sol")])));
    }

    #[test]
    fn a_session_that_lists_nothing_is_live_but_unknown() {
        let mut r = Reported::default();
        r.opened("old", 1, &listing("high"));
        r.opened("new", 1, &json!({"sessionId": "new"}));
        assert_eq!(r.wire(), (Some(values(&[])), None), "not the old session's values");
    }

    #[test]
    fn a_setter_without_a_list_makes_its_setting_unknown() {
        let mut r = Reported::default();
        r.opened("main", 1, &listing("low"));
        r.unconfirmed("main", &["effort".to_owned()], false);
        assert_eq!(r.wire().0, Some(values(&[("model", "sol")])), "low was before the change");
        r.unconfirmed("elsewhere", &["model".to_owned()], false);
        assert_eq!(r.wire().0, Some(values(&[("model", "sol")])));
        r.unconfirmed("main", &[], true);
        assert_eq!(r.wire(), (Some(values(&[])), None), "another model: nothing listed is known");
    }

    #[test]
    fn a_new_process_reusing_the_session_id_starts_unknown() {
        // Reviewer: a replacement agent process that loads the same session id isn't the process that reported.
        let mut r = Reported::default();
        r.opened("main", 1, &listing("high"));
        r.settle(|sid, process| sid == "main" && process == 1);
        assert_eq!(r.wire().0, Some(values(&[("effort", "high"), ("model", "sol")])));
        r.settle(|sid, process| sid == "main" && process == 2);
        assert_eq!(r.wire(), (None, None), "process 2 hasn't said anything yet");
    }

    #[test]
    fn a_report_ends_with_its_session() {
        let mut r = Reported::default();
        r.opened("main", 1, &listing("high"));
        r.settle(|sid, _| sid == "main");
        assert_eq!(r.wire().0, Some(values(&[("effort", "high"), ("model", "sol")])));
        // Closed, released, or the agent exited: no live session.
        r.settle(|_, _| false);
        assert_eq!(r.wire(), (None, None));
    }
}
