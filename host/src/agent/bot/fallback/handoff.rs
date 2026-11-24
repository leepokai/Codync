//! A switch the user made to another login (another agent, or another login profile of it: `Cmd::Reconfigure`). The
//! main session is left as on a move down the chain, so the next message carries a recap of the chat; until then a
//! line in the chat says a handoff is pending (`agentSwitch.manual`, `state`), and once it goes, it unfolds to what
//! was carried. What is owed is kept (`kv` `fallback.handoff.<bot>`), so a host restart in between still carries it.

use super::super::Actor;
use crate::agent::fallback;
use crate::store::BotConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn key(bot: &str) -> String {
    format!("fallback.handoff.{bot}")
}

/// The handoff the next main-chat message carries: its line, and `relay.recap` as it was owed.
#[derive(Serialize, Deserialize)]
struct Owed {
    line: String,
    from: String,
    session: Option<String>,
}

/// The line's state (`agentSwitch.state`).
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum State {
    Pending,
    HandedOff,
    /// Nothing was carried: the chat had nothing to tell, or the user started a new session.
    Switched,
}

impl State {
    fn text(self, from: &str, to: &str) -> String {
        let what = match self {
            Self::Pending => "Context handoff pending",
            Self::HandedOff => "Context handed off",
            Self::Switched => "Switched",
        };
        format!("{what}: {from} → {to}")
    }
}

/// `line`'s `agentSwitch` set to `state`, with its text.
fn set_state(line: &mut Value, state: State) {
    let change = &line["agentSwitch"];
    let text =
        state.text(change["fromName"].as_str().unwrap_or_default(), change["toName"].as_str().unwrap_or_default());
    line["agentSwitch"]["state"] = json!(state);
    line["text"] = text.into();
}

impl Actor {
    fn owed(&self) -> Option<Owed> {
        self.hub.store.kv_get(&key(&self.base.id)).and_then(|v| serde_json::from_str(&v).ok())
    }

    /// `Cmd::Reconfigure`, before the bot takes `to`: from a login that has a conversation to another one, the next
    /// message carries it. A second switch before that message moves the same line on.
    pub(in crate::agent::bot) fn switched_by_user(&mut self, to: &BotConfig) {
        if fallback::login::of(to) == fallback::login::of(&self.cfg) {
            return;
        }
        let to_name = self.called(to);
        if let Some(owed) = self.owed()
            && let Some(mut line) = self.hub.store.entry(&owed.line)
        {
            // Back where it came from: the line names the login it just left instead.
            if line.data["agentSwitch"]["fromName"] == to_name.as_str() {
                line.data["agentSwitch"]["from"] = self.cfg.backend.clone().into();
                line.data["agentSwitch"]["fromName"] = self.called(&self.cfg).into();
            }
            line.data["agentSwitch"]["to"] = to.backend.clone().into();
            line.data["agentSwitch"]["toName"] = to_name.into();
            set_state(&mut line.data, State::Pending);
            self.hub.set_entry(&owed.line, &line.data);
            return;
        }
        if self.session_id.is_none() {
            return;
        }
        self.owe_recap();
        let Some((from, session)) = self.relay.recap.clone() else { return };
        let mut data = json!({
            "style": "info",
            "agentSwitch": {
                "from": self.cfg.backend, "to": to.backend, "back": false, "reason": null, "resetsAt": null,
                "carried": null, "manual": true, "fromName": self.called(&self.cfg), "toName": to_name,
            },
        });
        set_state(&mut data, State::Pending);
        // In the bot's own chat, whichever lane a running turn is in: the recap is of that chat.
        let Some(line) = self.main_line(&data) else { return };
        let saved = serde_json::to_string(&Owed { line, from, session }).map_err(anyhow::Error::from);
        super::park::logged(
            &self.base.id,
            "keep the owed handoff",
            saved.and_then(|v| self.hub.store.kv_set(&key(&self.base.id), &v)),
        );
        crate::analytics::agent_switched_by_user(&self.hub, &to.backend, to.login_profile.is_some());
    }

    /// The recap a host restart dropped from `relay.recap`: the agent and session a user's switch left.
    pub(super) fn owed_recap(&self) -> Option<(String, Option<String>)> {
        self.owed().map(|o| (o.from, o.session))
    }

    /// The line of the user's switch the next message carries in.
    pub(super) fn owed_line(&self) -> Option<String> {
        self.owed().map(|o| o.line)
    }

    /// The user's switch stops being pending: handed off with what `carry_in` put on its line once the prompt went
    /// (`told`), or switched when that was nothing, or the user started a new session instead (`told`: false).
    pub(in crate::agent::bot) fn handed_off(&self, told: bool) {
        let Some(owed) = self.owed() else { return };
        if let Some(mut line) = self.hub.store.entry(&owed.line) {
            if !told {
                line.data["agentSwitch"]["carried"] = Value::Null;
            }
            let carried = line.data["agentSwitch"]["carried"].is_string();
            set_state(&mut line.data, if carried { State::HandedOff } else { State::Switched });
            self.hub.set_entry(&owed.line, &line.data);
        }
        super::park::logged(&self.base.id, "forget the owed handoff", self.hub.store.kv_delete(&key(&self.base.id)));
    }
}
