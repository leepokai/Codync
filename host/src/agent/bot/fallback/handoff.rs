//! A switch the user made to another login (another agent, or another login profile of it: `Cmd::Reconfigure`). The
//! main session is left as on a move down the chain, so the next message carries a recap of the chat; until then a
//! line in the chat says a handoff is pending (`agentSwitch.manual`, `state`), and once it goes, it unfolds to what
//! was carried. A move to another folder owes the same recap, without a line. What is owed is kept (`kv`
//! `fallback.handoff.<bot>`), so a host restart in between still carries it.

use super::super::Actor;
use crate::agent::fallback;
use crate::store::{BotConfig, Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn key(bot: &str) -> String {
    format!("fallback.handoff.{bot}")
}

/// The handoff the next main-chat message carries: its line (none for a move to another folder), `relay.recap` as it
/// was owed, and the login it left.
#[derive(Serialize, Deserialize)]
pub(super) struct Owed {
    pub(super) line: Option<String>,
    pub(super) from: String,
    pub(super) session: Option<String>,
    login: Option<String>,
}

impl Owed {
    /// A move to another folder, told to the same login it left (`fallback::login::of`), not one down the chain.
    pub(super) fn moved(&self, login: &str) -> bool {
        self.line.is_none() && self.login.as_deref() == Some(login)
    }
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

/// Bot `cfg` moved to its own folder while no actor ran (`hub::projects::migrate`): its next message carries a recap
/// of `session`, the main session it left, as after a move `Cmd::Reconfigure` makes. A handoff already owed carries it
/// as it is.
pub(crate) fn owe_move(store: &Store, cfg: &BotConfig, session: Option<String>) -> anyhow::Result<()> {
    if store.kv_get(&key(&cfg.id)).is_some() {
        return Ok(());
    }
    let owed = Owed { line: None, from: cfg.backend.clone(), session, login: Some(fallback::login::of(cfg)) };
    store.kv_set(&key(&cfg.id), &serde_json::to_string(&owed)?)
}

impl Actor {
    pub(super) fn owed(&self) -> Option<Owed> {
        self.hub.store.kv_get(&key(&self.base.id)).and_then(|v| serde_json::from_str(&v).ok())
    }

    /// `Cmd::Reconfigure`, before the bot takes `to`: from a login that has a conversation to another one, the next
    /// message carries it. A second switch before that message moves the same line on.
    pub(in crate::agent::bot) fn switched_by_user(&mut self, to: &BotConfig) {
        if fallback::login::of(to) == fallback::login::of(&self.cfg) {
            return;
        }
        let to_name = self.called(to);
        let owed = self.owed();
        if let Some(id) = owed.as_ref().and_then(|o| o.line.clone())
            && let Some(mut line) = self.hub.store.entry(&id)
        {
            // Back where it came from: the line names the login it just left instead.
            if line.data["agentSwitch"]["fromName"] == to_name.as_str() {
                line.data["agentSwitch"]["from"] = self.cfg.backend.clone().into();
                line.data["agentSwitch"]["fromName"] = self.called(&self.cfg).into();
            }
            line.data["agentSwitch"]["to"] = to.backend.clone().into();
            line.data["agentSwitch"]["toName"] = to_name.into();
            set_state(&mut line.data, State::Pending);
            self.hub.set_entry(&id, &line.data);
            return;
        }
        if self.session_id.is_none() && owed.is_none() {
            return;
        }
        // A move to another folder still owed: the recap is of the session that move left.
        if self.relay.recap.is_none() {
            self.relay.recap = owed.map(|o| (o.from, o.session));
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
        self.keep(&Owed { line: Some(line), from, session, login: None });
        crate::analytics::agent_switched_by_user(&self.hub, &to.backend, to.login_profile.is_some());
    }

    /// `Cmd::Reconfigure` to another folder: the agent starts again there in a new session (`restart_agent`), so the
    /// next message carries a recap of the chat, as after a switch of login. No line: the chat shows the folder in
    /// the bot's settings, not as a step of its conversation. A handoff already owed carries it as it is.
    pub(in crate::agent::bot) fn moved_folder(&mut self) {
        if self.owed().is_some() {
            return;
        }
        self.owe_recap();
        let Some((from, session)) = self.relay.recap.clone() else { return };
        let login = Some(fallback::login::of(&self.cfg));
        self.keep(&Owed { line: None, from, session, login });
    }

    fn keep(&self, owed: &Owed) {
        let saved = serde_json::to_string(owed).map_err(anyhow::Error::from);
        super::park::logged(
            &self.base.id,
            "keep the owed handoff",
            saved.and_then(|v| self.hub.store.kv_set(&key(&self.base.id), &v)),
        );
    }

    /// The user's switch stops being pending: handed off with what `carry_in` put on its line once the prompt went
    /// (`told`), or switched when that was nothing, or the user started a new session instead (`told`: false).
    pub(in crate::agent::bot) fn handed_off(&self, told: bool) {
        let Some(owed) = self.owed() else { return };
        if let Some(id) = &owed.line
            && let Some(mut line) = self.hub.store.entry(id)
        {
            if !told {
                line.data["agentSwitch"]["carried"] = Value::Null;
            }
            let carried = line.data["agentSwitch"]["carried"].is_string();
            set_state(&mut line.data, if carried { State::HandedOff } else { State::Switched });
            self.hub.set_entry(id, &line.data);
        }
        super::park::logged(&self.base.id, "forget the owed handoff", self.hub.store.kv_delete(&key(&self.base.id)));
    }
}
