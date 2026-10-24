//! The Effort level of each step of a bot's chain (its own agent, then each fallback): the agent setting of ACP
//! category `thought_level`, and one line saying what that step runs with, from its own agent's session only.

use super::bot::{Bot, ModelOption, Session};
use super::{App, Form};

/// How a step shows its Effort level.
#[derive(Debug, PartialEq, Eq)]
pub enum Effort<'a> {
    /// The agent has no effort level for this model (as far as anyone knows), and nothing is saved.
    None,
    /// Unknown until the agent runs this model.
    Later,
    /// The setting to pick from (when known) and the line under it.
    Row { setting: Option<&'a ModelOption>, caption: Option<String> },
}

/// The agent's effort level among `settings`.
pub fn setting(settings: &[ModelOption]) -> Option<&ModelOption> {
    settings.iter().find(|s| s.category.as_deref() == Some("thought_level"))
}

impl Bot {
    /// What agent `backend` says, when the bot runs on it now: its own agent unless it's on a fallback.
    pub fn session_of(&self, backend: &str) -> Option<&Session> {
        (self.agent_step.as_deref().unwrap_or(&self.backend) == backend).then_some(&self.session)
    }
}

/// The settings in the order the form shows them: the effort level first, then the agent's others.
pub fn in_order(settings: &[ModelOption]) -> Vec<&ModelOption> {
    let effort = setting(settings);
    effort.into_iter().chain(settings.iter().filter(|s| Some(*s) != effort)).collect()
}

/// What the form calls setting `o`: the effort level by that name, any other by the agent's.
pub fn label(o: &ModelOption) -> &str {
    if o.category.as_deref() == Some("thought_level") { "Effort level" } else { &o.name }
}

impl App {
    /// The settings a step offers: what the agent's catalog lists for `model`, each replaced by the session's own entry
    /// while the bot runs its agent on the model the host saved for the step with nothing pending (its list is for the
    /// model it runs, and may hold only some, as Cursor's does), then the session's others.
    pub(in crate::tui) fn settings_for(
        &self,
        bot: Option<&Bot>,
        backend: &str,
        model: &str,
        saved: bool,
    ) -> Vec<ModelOption> {
        let session = bot.filter(|_| saved).and_then(|b| b.session_of(backend)).filter(|s| !s.pending);
        let session = session.and_then(|s| s.settings.as_deref()).unwrap_or_default();
        let catalog = self.model_settings(backend, model);
        let own = |c: &ModelOption| session.iter().find(|s| s.id == c.id).unwrap_or(c).clone();
        let others = session.iter().filter(|s| !catalog.iter().any(|c| c.id == s.id)).cloned();
        catalog.iter().map(own).chain(others).collect()
    }

    /// What the form shows for its own agent (`step` `None`) or its fallback `step`: `step_settings` in order, then any
    /// setting picked for it that none lists, so it stays visible and can be cleared.
    pub(in crate::tui) fn form_settings(&self, f: &Form, step: Option<usize>) -> Vec<ModelOption> {
        let listed = &self.step_settings(f, step);
        let picked = match step {
            Some(i) => f.fallbacks.get(i).map(|x| &x.model_options),
            None => f.model_options(),
        };
        let unlisted = picked.into_iter().flat_map(|p| p.keys()).filter(|id| !listed.iter().any(|o| &o.id == *id));
        let unlisted =
            unlisted.map(|id| ModelOption { id: id.clone(), name: id.clone(), values: vec![], category: None });
        in_order(listed).into_iter().cloned().chain(unlisted).collect()
    }

    /// `settings_for` the form's own agent (`step` `None`) or its fallback `step`, as the form shows them now.
    pub(in crate::tui) fn step_settings(&self, f: &Form, step: Option<usize>) -> Vec<ModelOption> {
        let entry = step.and_then(|i| f.fallbacks.get(i));
        let (backend, model) = match entry {
            Some(x) => (x.backend.clone(), x.model.clone().unwrap_or_default()),
            None => (f.backend.clone(), f.model.text.trim().to_owned()),
        };
        let bot = self.bots.get(&f.bot_id);
        let saved = bot.is_some_and(|b| match entry {
            Some(_) => b.fallbacks.iter().any(|x| x.backend == backend && x.model.clone().unwrap_or_default() == model),
            None => b.backend == backend && b.model.clone().unwrap_or_default() == model,
        });
        self.settings_for(bot, &backend, &model, saved)
    }

    /// The Effort level of the form's own agent (`step` `None`) or of its fallback `step`. The line under it speaks
    /// only for what the host saved: none while the form shows another agent, model or value.
    pub(in crate::tui) fn form_effort(&self, f: &Form, step: Option<usize>) -> Effort<'_> {
        let entry = step.and_then(|i| f.fallbacks.get(i));
        let (backend, model) = match entry {
            Some(x) => (x.backend.as_str(), x.model.clone().unwrap_or_default()),
            None => (f.backend.as_str(), f.model.text.trim().to_owned()),
        };
        let picked = match entry {
            Some(x) => Some(&x.model_options),
            None => f.model_options(),
        };
        // What the host has for this agent and model.
        let bot = self.bots.get(&f.bot_id);
        let host = bot.and_then(|b| match entry {
            Some(_) => b
                .fallbacks
                .iter()
                .find(|x| x.backend == backend && x.model.clone().unwrap_or_default() == model)
                .map(|x| &x.model_options),
            None => (b.backend == backend && b.model.clone().unwrap_or_default() == model).then_some(&b.model_options),
        });
        let catalog = self.model_settings(backend, &model);
        let loading = matches!(self.models.get(backend), Some(None));
        let session = bot.filter(|_| host.is_some()).and_then(|b| b.session_of(backend));
        let id = match effort(catalog, None, session, loading, !model.is_empty()) {
            Effort::Row { setting: Some(s), .. } => Some(s.id.clone()),
            _ => None,
        };
        let shown = id.as_ref().and_then(|id| picked.and_then(|p| p.get(id)));
        let saved = id.as_ref().and_then(|id| host.and_then(|h| h.get(id)));
        match effort(catalog, saved.map(String::as_str), session, loading, !model.is_empty()) {
            Effort::Row { setting, .. } if host.is_none() || shown != saved => Effort::Row { setting, caption: None },
            e => e,
        }
    }
}

/// A step's Effort level: `catalog` is what its agent lists for its model, `saved` what the host saved for it, `session`
/// its agent's own report while it runs, `chosen_model` whether it names a model of its own. First match wins; nothing
/// is inferred from a saved value.
pub fn effort<'a>(
    catalog: &'a [ModelOption],
    saved: Option<&str>,
    session: Option<&'a Session>,
    loading: bool,
    chosen_model: bool,
) -> Effort<'a> {
    // While nothing is pending, the running session's own list is for the model it runs.
    let listed = session.filter(|s| !s.pending).and_then(|s| s.settings.as_deref()).and_then(setting);
    let Some(found) = listed.or_else(|| setting(catalog)) else {
        let line = |why: &str| Effort::Row { setting: None, caption: saved.map(|v| format!("Saved: {v} · {why}")) };
        return match saved {
            Some(_) if loading => line("checking this agent’s options"),
            None if chosen_model && catalog.is_empty() && listed.is_none() && !loading => Effort::Later,
            Some(_) => line("not offered for this model"),
            None => Effort::None,
        };
    };
    let name = |v: &str| found.values.iter().find(|(id, _)| id == v).map_or(v, |(_, n)| n.as_str()).to_owned();
    let reported = session.and_then(|s| s.options.as_ref()).and_then(|o| o.get(&found.id)).map(String::as_str);
    let issue = session.and_then(|s| s.issues.get(&found.id)).filter(|(_, v)| Some(v.as_str()) == saved);
    let caption = match (saved, reported) {
        (Some(s), _) if issue.is_some_and(|(i, _)| i == "rejected") => {
            format!("Saved: {} · not applied (the agent refused it)", name(s))
        }
        (Some(s), Some(r)) if issue.is_some() => format!("In use: {} · Saved: {} not offered", name(r), name(s)),
        (Some(s), None) if issue.is_some() => format!("Saved: {} · not offered for this model", name(s)),
        (s, r) if session.is_some_and(|x| x.pending) && s != r => {
            let next = s.map_or_else(|| "Default".to_owned(), name);
            r.map_or_else(
                || format!("Current: unknown · {next} pending"),
                |r| format!("In use: {} · {next} pending", name(r)),
            )
        }
        (s, _) if !session.is_some_and(|x| x.live) => {
            s.map_or_else(|| "Not running".to_owned(), |s| format!("Not running · Saved: {}", name(s)))
        }
        (_, None) => "Current: unknown".to_owned(),
        (None, Some(r)) => format!("In use: {} (agent default)", name(r)),
        (Some(s), Some(r)) if s == r => format!("In use: {}", name(r)),
        (Some(s), Some(r)) => format!("Saved: {} · not confirmed (in use: {})", name(s), name(r)),
    };
    Effort::Row { setting: Some(found), caption: Some(caption) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn level() -> ModelOption {
        let values = [("low", "Low"), ("high", "High")].map(|(i, n)| (i.to_owned(), n.to_owned())).to_vec();
        ModelOption { id: "effort".into(), name: "Thinking".into(), values, category: Some("thought_level".into()) }
    }

    fn session(options: Option<&[(&str, &str)]>) -> Session {
        let options = options.map(|o| o.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect());
        Session { live: options.is_some(), options, settings: Some(vec![level()]), ..Session::default() }
    }

    fn caption(e: Effort<'_>) -> Option<String> {
        match e {
            Effort::Row { caption, .. } => caption,
            other => panic!("no row: {other:?}"),
        }
    }

    #[test]
    fn the_effort_level_is_the_thought_level_setting() {
        let other = ModelOption { category: Some("_custom".into()), ..level() };
        assert_eq!(setting(&[other.clone(), level()]), Some(&level()));
        assert_eq!(setting(&[other]), None);
    }

    #[test]
    fn the_line_says_what_the_running_session_confirms() {
        let c = [level()];
        let low = session(Some(&[("effort", "low")]));
        assert_eq!(caption(effort(&c, Some("low"), Some(&low), false, false)).as_deref(), Some("In use: Low"));
        assert_eq!(caption(effort(&c, None, Some(&low), false, false)).as_deref(), Some("In use: Low (agent default)"));
        let mismatch = caption(effort(&c, Some("high"), Some(&low), false, false));
        assert_eq!(mismatch.as_deref(), Some("Saved: High · not confirmed (in use: Low)"));
        let unknown = session(Some(&[]));
        assert_eq!(
            caption(effort(&c, Some("high"), Some(&unknown), false, false)).as_deref(),
            Some("Current: unknown")
        );
        assert_eq!(caption(effort(&c, Some("high"), None, false, false)).as_deref(), Some("Not running · Saved: High"));
        assert_eq!(caption(effort(&c, None, None, false, false)).as_deref(), Some("Not running"));
    }

    #[test]
    fn a_pending_change_and_an_issue_say_so() {
        let c = [level()];
        let pending = Session { pending: true, ..session(Some(&[("effort", "low")])) };
        assert_eq!(
            caption(effort(&c, Some("high"), Some(&pending), false, false)).as_deref(),
            Some("In use: Low · High pending")
        );
        let issues = BTreeMap::from([("effort".to_owned(), ("rejected".to_owned(), "high".to_owned()))]);
        let refused = Session { issues, ..session(None) };
        let line = caption(effort(&c, Some("high"), Some(&refused), false, false));
        assert_eq!(line.as_deref(), Some("Saved: High · not applied (the agent refused it)"));
        assert_eq!(
            caption(effort(&c, Some("low"), Some(&refused), false, false)).as_deref(),
            Some("Not running · Saved: Low")
        );
    }

    #[test]
    fn unknown_and_absent_effort_levels() {
        assert_eq!(effort(&[], None, None, false, true), Effort::Later, "a chosen model the agent hasn't run yet");
        assert_eq!(effort(&[], None, None, false, false), Effort::None);
        let loading = caption(effort(&[], Some("high"), None, true, false));
        assert_eq!(loading.as_deref(), Some("Saved: high · checking this agent’s options"));
        let gone = caption(effort(&[], Some("high"), None, false, false));
        assert_eq!(gone.as_deref(), Some("Saved: high · not offered for this model"));
    }
}
