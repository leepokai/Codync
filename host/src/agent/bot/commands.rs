//! The agent's own `/` commands (ACP `available_commands_update`): the latest list per bot, shown in the apps'
//! `/` list while it belongs to the agent the bot runs on. A message in the bot's own chat that starts with one
//! goes to the agent as typed, so the agent runs it (docs/features/marketplace.md, "Commands").

use super::Actor;
use crate::store::Store;
use codync_agent::events::Command;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Names Codync's own `/` list takes first: Fast mode, and the terminal's dev servers.
const BUILT_IN: [&str; 2] = ["fast", "servers"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommand {
    /// What follows the `/`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// What to type after the name, for a command that takes input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// The list as an agent last sent it.
#[derive(Serialize, Deserialize)]
struct Listed {
    agent: String,
    commands: Vec<AgentCommand>,
}

fn key(bot: &str) -> String {
    format!("agent.commands.{bot}")
}

/// The commands the agent listed, each name once; a name with a space can't be typed, so it's left out.
fn parse(commands: &[Command]) -> Vec<AgentCommand> {
    let mut seen = HashSet::new();
    commands
        .iter()
        .filter_map(|c| {
            let name = c.name.trim().trim_start_matches('/');
            let typable = !name.is_empty() && !name.contains(char::is_whitespace);
            (typable && seen.insert(name.to_owned())).then(|| AgentCommand {
                name: name.to_owned(),
                description: c.description.as_deref().unwrap_or_default().trim().to_owned(),
                hint: c.hint.as_deref().map(str::trim).filter(|h| !h.is_empty()).map(str::to_owned),
            })
        })
        .collect()
}

/// What `agent` last listed for `bot`, without the names Codync's own commands and the bot's skills take:
/// typing those runs Codync's. Empty when another agent listed it.
pub fn listed(store: &Store, bot: &str, agent: &str, enabled_skills: &[String]) -> Vec<AgentCommand> {
    let Some(saved) = store.kv_get(&key(bot)).and_then(|v| serde_json::from_str::<Listed>(&v).ok()) else {
        return Vec::new();
    };
    if saved.agent != agent {
        return Vec::new();
    }
    saved
        .commands
        .into_iter()
        .filter(|c| !BUILT_IN.contains(&c.name.as_str()) && !enabled_skills.contains(&c.name))
        .collect()
}

impl Actor {
    /// Keeps the commands the running agent just listed, and tells the apps when they changed.
    pub(super) fn note_commands(&self, commands: &[Command]) {
        let listed = Listed { agent: self.cfg.backend.clone(), commands: parse(commands) };
        let text = serde_json::to_string(&listed).expect("a command list is plain data and always serializes");
        let key = key(&self.cfg.id);
        if self.hub.store.kv_get(&key).as_deref() == Some(text.as_str()) {
            return;
        }
        if let Err(error) = self.hub.store.kv_set(&key, &text) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the agent's commands");
            return;
        }
        self.hub.emit_bot(&self.cfg.id);
    }

    /// `text` starts with a command the running agent lists: the message goes to it as typed.
    pub(super) fn is_agent_command(&self, text: &str) -> bool {
        let Some((name, _)) = crate::market::slash_command(text) else { return false };
        let skills = crate::chat::context::enabled_skills(&self.hub.store, &self.cfg);
        listed(&self.hub.store, &self.cfg.id, &self.cfg.backend, &skills).iter().any(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_list_keeps_each_typable_name_once_with_its_description_and_hint() {
        let update = json!({"sessionUpdate": "available_commands_update", "availableCommands": [
            {"name": "review", "description": " Review the changes ", "input": {"hint": "what to look at"}},
            {"name": "/compact", "description": "Summarize the conversation"},
            {"name": "review", "description": "again"},
            {"name": "two words"},
            {"description": "no name"},
        ]});
        let codync_agent::events::Event::Commands(commands) = codync_agent::events::Event::parse(&update) else {
            panic!("a command list")
        };
        assert_eq!(
            parse(&commands),
            vec![
                AgentCommand {
                    name: "review".into(),
                    description: "Review the changes".into(),
                    hint: Some("what to look at".into())
                },
                AgentCommand { name: "compact".into(), description: "Summarize the conversation".into(), hint: None },
            ]
        );
    }

    #[test]
    fn the_list_follows_the_agent_and_leaves_codyncs_own_names_out() {
        let path = std::env::temp_dir().join(format!("codync-agent-commands-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).expect("temp store");
        let skills = ["release".to_owned()];
        let commands = ["review", "fast", "release", "servers"].map(|name| AgentCommand {
            name: name.into(),
            description: String::new(),
            hint: None,
        });
        let saved = Listed { agent: "claude".into(), commands: commands.to_vec() };
        store.kv_set(&key("b"), &serde_json::to_string(&saved).unwrap()).unwrap();
        let names = |agent| listed(&store, "b", agent, &skills).into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(names("claude"), ["review"]);
        assert!(names("codex").is_empty(), "another agent's list isn't this one's");
        let _ = std::fs::remove_file(&path);
    }
}
