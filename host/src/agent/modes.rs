//! The agent's session modes: how it asks before it acts (Claude's Manual / Accept edits / Plan / Auto, Codex's
//! Read-only / Auto review / Full access…), as its `mode` config option or the older `modes` field lists them.
//! A bot names the one it wants (`BotConfig::mode`); what still reaches Codync follows the bot's Approvals.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The mode where the agent itself decides what is safe and asks only about the rest. Codync's name for it, and
/// Claude's id: an agent that calls it something else (Codex's `agent`, "Auto review") is listed under it.
pub const AUTO: &str = "auto";

/// One mode, with the agent's own id and words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mode {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// A session's modes as the agent last listed them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modes {
    /// The config option that switches them; none for an agent that lists only the older `modes` field.
    pub config_id: Option<String>,
    pub current: Option<String>,
    /// The mode the session opened in, which a bot without a mode of its own works in.
    pub opening: Option<String>,
    /// In the agent's order.
    pub list: Vec<Mode>,
    /// The id of its Auto mode, if it has one.
    auto: Option<String>,
}

/// What a session does for the mode a bot wants (see [`switch`]).
#[derive(Debug, PartialEq, Eq)]
pub enum Switch<'a> {
    /// It's in that mode already, or the bot wants [`AUTO`] from an agent without one, which keeps its own.
    Stay,
    To(&'a Mode),
    /// The agent doesn't have that mode: the turn doesn't start, rather than run in one the user didn't pick.
    Missing,
}

impl Modes {
    /// From a `session/new` / `session/load` answer or a `config_option_update` of agent `backend`; none when the
    /// agent lists no modes.
    pub fn from_session(session: &Value, backend: &str) -> Option<Self> {
        let (config_id, current, list) = if let Some(option) =
            session["configOptions"].as_array().into_iter().flatten().find(|o| o["category"] == "mode")
        {
            let mut list = Vec::new();
            flatten(&option["options"], &mut list);
            (text(&option["id"]), text(&option["currentValue"]), list)
        } else {
            let modes = &session["modes"];
            let list = modes["availableModes"].as_array().into_iter().flatten().filter_map(|m| entry(&m["id"], m));
            (None, text(&modes["currentModeId"]), list.collect())
        };
        let auto = auto_mode(backend, &list).map(|m| m.id.clone());
        (!list.is_empty()).then(|| Self { config_id, opening: current.clone(), current, list, auto })
    }

    fn by_id(&self, id: &str) -> Option<&Mode> {
        self.list.iter().find(|m| m.id == id)
    }

    /// What clients offer: `[{id, name, description}]`, the Auto mode first and listed as [`AUTO`], so a bot's choice
    /// carries over to another agent that has one.
    pub fn catalog(&self) -> Value {
        let auto = self.auto.as_deref();
        let mut modes: Vec<Mode> = self.list.clone();
        // Another agent's mode under Codync's id for Auto (Goose's approves every tool call) can't be told apart from
        // it, so it isn't offered.
        modes.retain(|m| m.id != AUTO || auto == Some(AUTO));
        modes.sort_by_key(|m| Some(m.id.as_str()) != auto);
        for mode in &mut modes {
            if Some(mode.id.as_str()) == auto {
                AUTO.clone_into(&mut mode.id);
            }
        }
        json!(modes)
    }
}

/// What a session in `modes` (none: the agent lists none) does for a bot that wants `wanted`: the agent's own id, or
/// [`AUTO`] for its Auto mode under any id; `None` for the mode the session opened in.
pub fn switch<'a>(modes: Option<&'a Modes>, wanted: Option<&str>) -> Switch<'a> {
    let Some(modes) = modes else {
        return if wanted.is_some_and(|id| id != AUTO) { Switch::Missing } else { Switch::Stay };
    };
    let mode = match wanted {
        None => modes.opening.as_deref().and_then(|id| modes.by_id(id)),
        Some(AUTO) => modes.auto.as_deref().and_then(|id| modes.by_id(id)),
        Some(id) => match modes.by_id(id) {
            Some(mode) => Some(mode),
            None => return Switch::Missing,
        },
    };
    match mode {
        Some(mode) if modes.current.as_deref() != Some(&mode.id) => Switch::To(mode),
        _ => Switch::Stay,
    }
}

/// The agent's Auto mode: Claude's `auto`, or one named "Auto review" (Codex's `agent`). Only these: another agent's
/// `auto` may approve everything without asking (Goose's).
fn auto_mode<'a>(backend: &str, list: &'a [Mode]) -> Option<&'a Mode> {
    list.iter()
        .find(|m| backend == "claude" && m.id == AUTO)
        .or_else(|| list.iter().find(|m| m.name.eq_ignore_ascii_case("auto review")))
}

fn flatten(options: &Value, out: &mut Vec<Mode>) {
    for option in options.as_array().into_iter().flatten() {
        if option["options"].is_array() {
            flatten(&option["options"], out);
        } else if let Some(mode) = entry(&option["value"], option)
            && !out.iter().any(|m| m.id == mode.id)
        {
            out.push(mode);
        }
    }
}

fn entry(id: &Value, mode: &Value) -> Option<Mode> {
    let id = text(id)?;
    let description = text(&mode["description"]).map(sentence);
    Some(Mode { name: text(&mode["name"]).unwrap_or_else(|| id.clone()), description, id })
}

/// Clients put Codync's own sentence after the agent's description, so it ends like one ("Claude handles
/// permission decisions" has no period).
fn sentence(mut s: String) -> String {
    if !s.ends_with(['.', '!', '?', '。', '！', '？']) {
        s.push('.');
    }
    s
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude() -> Value {
        json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "options": []},
            {"id": "mode", "category": "mode", "type": "select", "currentValue": "default", "options": [
                {"value": "default", "name": "Manual", "description": "Always ask before making changes"},
                {"value": "acceptEdits", "name": "Accept edits"},
                {"value": "plan", "name": "Plan"},
                {"value": "auto", "name": "Auto", "description": "Claude handles permission decisions"}]}]})
    }

    fn codex() -> Value {
        json!({"configOptions": [{"id": "mode", "category": "mode", "type": "select", "currentValue": "agent", "options": [
            {"value": "read-only", "name": "Read-only"},
            {"value": "agent", "name": "Auto review", "description": "Only ask for actions detected as potentially unsafe"},
            {"value": "agent-full-access", "name": "Full access"}]}]})
    }

    fn goose() -> Value {
        json!({"modes": {"currentModeId": "smart_approve", "availableModes": [
            {"id": "auto", "name": "auto", "description": "Automatically approve tool calls"},
            {"id": "approve", "name": "approve"}, {"id": "smart_approve", "name": "smart_approve"},
            {"id": "chat", "name": "chat"}]}})
    }

    fn to(modes: &Modes, wanted: Option<&str>) -> Option<String> {
        match switch(Some(modes), wanted) {
            Switch::To(mode) => Some(mode.id.clone()),
            Switch::Stay => None,
            Switch::Missing => Some("missing".into()),
        }
    }

    #[test]
    fn auto_is_the_agents_own_auto_mode_under_any_id() {
        let claude = Modes::from_session(&claude(), "claude").unwrap();
        assert_eq!(claude.config_id.as_deref(), Some("mode"));
        assert_eq!(to(&claude, Some(AUTO)).as_deref(), Some("auto"));
        assert_eq!(to(&claude, Some("plan")).as_deref(), Some("plan"));
        assert_eq!(to(&claude, Some("default")), None, "already in it");
        let codex = Modes::from_session(&codex(), "codex").unwrap();
        assert_eq!(to(&codex, Some(AUTO)), None, "its Auto review is where it starts");
        assert_eq!(to(&codex, Some("read-only")).as_deref(), Some("read-only"));
        let manual = json!({"modes": {"currentModeId": "ask", "availableModes": [{"id": "ask", "name": "Ask"}, {"id": "code", "name": "Code"}]}});
        let legacy = Modes::from_session(&manual, "kimi").unwrap();
        assert_eq!((legacy.config_id.as_deref(), legacy.current.as_deref()), (None, Some("ask")));
        assert_eq!(to(&legacy, Some(AUTO)), None, "no Auto mode: it keeps its own, and the bot asks the user");
        assert!(Modes::from_session(&json!({"configOptions": [{"id": "model", "category": "model"}]}), "x").is_none());
    }

    #[test]
    fn a_mode_the_agent_lacks_never_runs_in_another() {
        let codex = Modes::from_session(&codex(), "codex").unwrap();
        assert_eq!(to(&codex, Some("plan")).as_deref(), Some("missing"), "another agent's id");
        assert_eq!(switch(None, Some("plan")), Switch::Missing, "an agent without modes");
        assert_eq!(switch(None, Some(AUTO)), Switch::Stay);
        assert_eq!(switch(None, None), Switch::Stay);
        // Goose's `auto` approves every tool call: it isn't the Auto mode, nor offered under its id.
        let goose = Modes::from_session(&goose(), "goose").unwrap();
        assert_eq!(to(&goose, Some(AUTO)), None);
        let ids: Vec<Value> = goose.catalog().as_array().unwrap().iter().map(|m| m["id"].clone()).collect();
        assert_eq!(ids, [json!("approve"), json!("smart_approve"), json!("chat")]);
        // Nor is Claude's, run as a custom command: Codync can't tell which agent it is.
        let custom = Modes::from_session(&claude(), "custom").unwrap();
        assert_eq!(to(&custom, Some(AUTO)), None);
    }

    #[test]
    fn a_bot_without_a_mode_goes_back_to_the_one_its_session_opened_in() {
        let mut claude = Modes::from_session(&claude(), "claude").unwrap();
        assert_eq!(to(&claude, None), None);
        claude.current = Some("auto".into());
        assert_eq!(to(&claude, None).as_deref(), Some("default"));
    }

    #[test]
    fn clients_get_the_auto_mode_first_and_under_codyncs_name() {
        let codex = Modes::from_session(&codex(), "codex").unwrap().catalog();
        let ids: Vec<&str> = codex.as_array().unwrap().iter().filter_map(|m| m["id"].as_str()).collect();
        assert_eq!(ids, ["auto", "read-only", "agent-full-access"]);
        assert_eq!(codex[0]["name"], "Auto review");
        let claude = Modes::from_session(&claude(), "claude").unwrap().catalog();
        assert_eq!(claude[0]["id"], "auto");
        assert_eq!(
            claude[1],
            json!({"id": "default", "name": "Manual", "description": "Always ask before making changes."})
        );
        // A description that already ends a sentence keeps its own mark.
        assert_eq!(sentence("Plan first!".into()), "Plan first!");
    }
}
