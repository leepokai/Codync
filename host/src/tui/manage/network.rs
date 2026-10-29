//! The networks of the user's computers (codync-network) this computer is in: who is in each,
//! inviting, creating or joining another (`network_setup.rs`), leaving, roles, removal,
//! recovery, each network's rules (`network_rules.rs`), and the cloud assistants switch. The TUI
//! runs on the computer, so it gets everything the desktop app does.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};
use std::borrow::Cow;

use super::super::app::{After, App, Editor, Overlay, copy};
use super::super::l10n::tr;
use super::network_rules::RuleForm;
use super::network_setup::Setup;

#[derive(Default)]
pub struct NetworkSheet {
    /// `networkView`'s answer; `None` until it comes.
    pub view: Option<Value>,
    /// The network shown, in `view.networks`.
    pub net: usize,
    /// The computer picked in it.
    pub cursor: usize,
    pub step: Step,
    pub error: Option<String>,
    pub busy: bool,
}

#[derive(Default)]
pub enum Step {
    #[default]
    List,
    Create(Setup),
    Join(Setup),
    /// The recovery code, shown once: only "I saved it" closes it.
    Recovery(Recovery),
    /// A one-time invite (`None` while it comes) and when it stops working.
    Invite(Option<(String, i64)>),
    Recover(Editor),
    Remove {
        computer: String,
        cascade: bool,
    },
    Leave,
    /// The network's rules, and the one picked.
    Rules(usize),
    NewRule(RuleForm),
}

/// A new network's recovery code, and whose it is (named from the host's answer: the sheet may
/// not show the new network yet).
pub struct Recovery {
    pub code: String,
    pub network: String,
    pub computer: String,
}

/// What a network call's answer is for.
#[derive(Clone, Copy)]
pub enum NetReply {
    /// A view or status to show (`networkView`, `networkPeers`, `networkJoin`, …).
    View,
    Created,
    Invite,
    /// Done: flash this and show the list again.
    Done(&'static str),
    Assistants,
}

impl NetworkSheet {
    pub fn networks(&self) -> &[Value] {
        self.view.as_ref().and_then(|v| v["networks"].as_array()).map_or(&[], Vec::as_slice)
    }

    /// The network shown.
    pub fn current(&self) -> Option<&Value> {
        self.networks().get(self.net)
    }

    /// The shown network's id, for the host's `network`.
    pub fn id(&self) -> Value {
        self.current().map_or(Value::Null, |n| n["id"].clone())
    }

    pub fn joined(&self) -> bool {
        !self.networks().is_empty()
    }

    pub fn role(&self) -> &str {
        self.current().and_then(|n| n["role"].as_str()).unwrap_or_default()
    }

    /// This computer's name in the shown network.
    pub fn computer(&self) -> Cow<'_, str> {
        self.current().and_then(|n| n["computer"].as_str()).map_or_else(|| tr("this computer"), Cow::Borrowed)
    }

    /// What the host says this computer may do in the shown network (`canInvite`, `canRecover`).
    pub fn may(&self, flag: &str) -> bool {
        self.current().is_some_and(|n| n[flag] == true)
    }

    /// What the host says of this computer as a whole (`assistantsAvailable`, `assistants`).
    pub fn says(&self, flag: &str) -> bool {
        self.view.as_ref().is_some_and(|v| v[flag] == true)
    }

    /// The shown network's name.
    pub fn name(&self) -> &str {
        self.current().and_then(|n| n["name"].as_str()).unwrap_or_default()
    }

    /// This computer's row in the shown network.
    pub fn me(&self) -> Option<&Value> {
        self.computers().iter().find(|c| c["me"] == true)
    }

    pub fn computers(&self) -> &[Value] {
        self.current().and_then(|n| n["computers"].as_array()).map_or(&[], Vec::as_slice)
    }

    pub fn rules(&self) -> &[Value] {
        self.current().and_then(|n| n["rules"].as_array()).map_or(&[], Vec::as_slice)
    }

    fn selected(&self) -> Option<&Value> {
        self.computers().get(self.cursor)
    }

    /// Takes a new view, keeping the network shown when it's still there.
    fn show(&mut self, view: Value) {
        let shown = self.id();
        self.view = Some(view);
        self.net = self.networks().iter().position(|n| n["id"] == shown).unwrap_or(0);
        self.cursor = self.cursor.min(self.computers().len().saturating_sub(1));
    }
}

impl App {
    pub fn open_network(&mut self) {
        self.net("networkView", json!({}), NetReply::View);
        self.overlays.push(Overlay::Network(Box::default()));
    }

    pub(super) fn net(&self, method: &'static str, body: Value, r: NetReply) {
        self.call(method, body, After::Network(r));
    }

    /// The `network` event: what the open sheet shows.
    pub(in crate::tui) fn network_changed(&mut self, view: &Value) {
        self.network = view.clone();
        for o in &mut self.overlays {
            if let Overlay::Network(n) = o {
                n.show(view.clone());
            }
        }
    }

    pub(in crate::tui) fn network_reply(&mut self, r: NetReply, res: Result<Value, String>) {
        let open = self.overlays.iter().any(|o| matches!(o, Overlay::Network(_)));
        if matches!(r, NetReply::Created) && res.is_ok() && !open {
            // The recovery code is shown only once: bring back the sheet that was closed meanwhile.
            self.open_network();
        }
        let Some(Overlay::Network(n)) = self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Network(_)))
        else {
            return;
        };
        n.busy = false;
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                n.error = Some(e);
                return;
            }
        };
        n.error = None;
        let mut flash = None;
        match r {
            NetReply::View => {
                n.show(v);
                if matches!(n.step, Step::Join(_) | Step::Leave | Step::NewRule(_)) {
                    n.step = Step::List;
                }
            }
            NetReply::Created => {
                if let Some(at) = n.networks().iter().position(|x| x["id"] == v["network"]) {
                    n.net = at;
                }
                let text = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
                n.step = Step::Recovery(Recovery {
                    code: text("recoveryCode"),
                    network: text("name"),
                    computer: text("computer"),
                });
            }
            NetReply::Invite => {
                let code = v["invite"].as_str().unwrap_or_default().to_owned();
                n.step = Step::Invite(Some((code, v["expiresAt"].as_i64().unwrap_or_default())));
            }
            NetReply::Done(message) => {
                n.step = Step::List;
                flash = Some(message);
            }
            NetReply::Assistants => {
                if let Some(view) = &mut n.view {
                    view["assistants"] = v["assistants"].clone();
                }
            }
        }
        if let Some(message) = flash {
            self.flash(message);
            self.net("networkView", json!({}), NetReply::View);
        }
    }

    pub(in crate::tui) fn network_key(&mut self, mut n: Box<NetworkSheet>, k: KeyEvent) -> Option<Overlay> {
        let step = std::mem::take(&mut n.step);
        n.step = match step {
            Step::List => return self.network_list_key(n, k),
            Step::Create(s) => self.setup_key(&mut n, s, false, k),
            Step::Join(s) => self.setup_key(&mut n, s, true, k),
            Step::Recovery(r) => self.recovery_key(r, k),
            Step::Invite(invite) => self.invite_key(&n, invite, k),
            Step::Recover(code) => self.recover_key(&mut n, code, k),
            Step::Remove { computer, cascade } => self.remove_key(&mut n, computer, cascade, k),
            Step::Leave => self.leave_key(&mut n, k),
            Step::Rules(at) => self.rules_key(&mut n, at, k),
            Step::NewRule(form) => self.rule_form_key(&mut n, form, k),
        };
        Some(Overlay::Network(n))
    }

    fn recovery_key(&mut self, r: Recovery, k: KeyEvent) -> Step {
        match k.code {
            KeyCode::Char('y') => {
                copy(&r.code);
                self.flash("Recovery code copied");
                Step::Recovery(r)
            }
            // Closed only once the user says they kept it.
            KeyCode::Char('s') => Step::List,
            _ => Step::Recovery(r),
        }
    }

    fn invite_key(&mut self, n: &NetworkSheet, invite: Option<(String, i64)>, k: KeyEvent) -> Step {
        match (k.code, invite) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => Step::List,
            (KeyCode::Char('y'), Some((code, until))) => {
                copy(&code);
                self.flash("Invite copied");
                Step::Invite(Some((code, until)))
            }
            (KeyCode::Char('n'), _) => {
                self.net("networkInvite", json!({"network": n.id()}), NetReply::Invite);
                Step::Invite(None)
            }
            (_, invite) => Step::Invite(invite),
        }
    }

    fn recover_key(&mut self, n: &mut NetworkSheet, mut code: Editor, k: KeyEvent) -> Step {
        match k.code {
            KeyCode::Esc => Step::List,
            KeyCode::Enter if !code.text.trim().is_empty() => {
                n.busy = true;
                let done = NetReply::Done(
                    "This computer is the owner now. Remove the lost computer from the list; if it was stolen, also remove the computers it invited.",
                );
                self.net("networkRecover", json!({"network": n.id(), "code": code.text.trim()}), done);
                Step::Recover(code)
            }
            _ => {
                code.key(k);
                Step::Recover(code)
            }
        }
    }

    fn remove_key(&mut self, n: &mut NetworkSheet, computer: String, cascade: bool, k: KeyEvent) -> Step {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => Step::List,
            KeyCode::Char('c' | ' ') => Step::Remove { computer, cascade: !cascade },
            KeyCode::Enter => {
                n.busy = true;
                let body = json!({"network": n.id(), "computer": computer, "cascade": cascade});
                self.net("networkRemove", body, NetReply::Done("Removed from the network"));
                Step::Remove { computer, cascade }
            }
            _ => Step::Remove { computer, cascade },
        }
    }

    fn leave_key(&mut self, n: &mut NetworkSheet, k: KeyEvent) -> Step {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => Step::List,
            KeyCode::Enter if !n.busy => {
                n.busy = true;
                self.net("networkLeave", json!({"network": n.id()}), NetReply::View);
                Step::Leave
            }
            _ => Step::Leave,
        }
    }

    fn network_list_key(&mut self, mut n: Box<NetworkSheet>, k: KeyEvent) -> Option<Overlay> {
        let last = n.computers().len().saturating_sub(1);
        let selected = n.selected().cloned();
        let name = |c: &Value| c["name"].as_str().unwrap_or_default().to_owned();
        let count = n.networks().len();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'N') => return None,
            KeyCode::Down | KeyCode::Char('j') => n.cursor = (n.cursor + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => n.cursor = n.cursor.saturating_sub(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char('[' | ']') if count > 1 => {
                let by = if matches!(k.code, KeyCode::Left | KeyCode::Char('[')) { -1 } else { 1 };
                n.net = super::super::app::cycle(n.net, by, count);
                n.cursor = 0;
            }
            KeyCode::Char('a') if n.says("assistantsAvailable") => {
                self.net("setAssistants", json!({"enabled": !n.says("assistants")}), NetReply::Assistants);
            }
            // A computer can be in several networks: these work whether it's in one or not.
            KeyCode::Char('c') if n.view.is_some() => n.step = Step::Create(Setup::default()),
            KeyCode::Char('J') if n.view.is_some() => n.step = Step::Join(Setup::join()),
            KeyCode::Char('L') if n.joined() => n.step = Step::Leave,
            KeyCode::Char('w') if n.joined() => n.step = Step::Rules(0),
            KeyCode::Char('r') if n.joined() => {
                n.busy = true;
                self.net("networkPeers", json!({"network": n.id()}), NetReply::View);
            }
            KeyCode::Char('i') if n.may("canInvite") => {
                self.net("networkInvite", json!({"network": n.id()}), NetReply::Invite);
                n.step = Step::Invite(None);
            }
            KeyCode::Char('o') if n.may("canRecover") => n.step = Step::Recover(Editor::default()),
            KeyCode::Char('p') => match selected {
                Some(c) if c["canPromote"] == true => {
                    let body = json!({"network": n.id(), "computer": name(&c)});
                    self.net("networkPromote", body, NetReply::Done("Made an admin"));
                }
                Some(_) => self.flash("Only the owner makes plain members admins"),
                None => {}
            },
            KeyCode::Char('x') => match selected {
                Some(c) if c["canRemove"] == true => n.step = Step::Remove { computer: name(&c), cascade: false },
                Some(c) if c["me"] == true => self.flash("Remove this computer from another one"),
                Some(_) => self.flash("Only the owner removes admins; admins remove members"),
                None => {}
            },
            _ => {}
        }
        Some(Overlay::Network(n))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::super::app::test_app;
    use super::*;
    use crossterm::event::KeyModifiers;

    pub(in crate::tui::manage) fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn network(id: &str, role: &str, rules: &Value) -> Value {
        json!({"id": id, "name": id, "computer": "desk", "role": role, "canInvite": role != "member",
            "canRecover": role != "owner", "rules": rules, "computers": [
            {"name": "desk", "role": role, "me": true, "online": true, "agents": [{"id": "reviewer", "name": "Reviewer"}],
             "canRemove": false, "canPromote": false},
            {"name": "laptop", "role": "member", "me": false, "online": true, "agents": [{"id": "coder", "name": "Coder"}],
             "canRemove": role != "member", "canPromote": role == "owner"},
        ]})
    }

    pub(in crate::tui::manage) fn sheet(networks: &[Value]) -> Box<NetworkSheet> {
        let view = json!({"joined": !networks.is_empty(), "networks": networks, "assistants": false, "assistantsAvailable": true});
        Box::new(NetworkSheet { view: Some(view), ..NetworkSheet::default() })
    }

    pub(in crate::tui::manage) fn owner() -> Box<NetworkSheet> {
        sheet(&[network("home", "owner", &json!([]))])
    }

    #[tokio::test]
    async fn removing_asks_first_and_cascade_is_a_choice() {
        let mut app = test_app();
        let mut n = owner();
        n.cursor = 1;
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Char('x'))) else { panic!("kept open") };
        assert!(matches!(&n.step, Step::Remove { computer, cascade: false } if computer == "laptop"));
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Char('c'))) else { panic!("kept open") };
        assert!(matches!(n.step, Step::Remove { cascade: true, .. }));
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Esc)) else { panic!("kept open") };
        assert!(matches!(n.step, Step::List));
    }

    #[tokio::test]
    async fn a_member_cant_remove_and_the_recovery_code_needs_saving() {
        let mut app = test_app();
        let mut n = sheet(&[network("home", "member", &json!([]))]);
        n.cursor = 1;
        let Some(Overlay::Network(mut n)) = app.network_key(n, key(KeyCode::Char('x'))) else { panic!("kept open") };
        assert!(matches!(n.step, Step::List), "the host said it can't");
        n.step = Step::Recovery(Recovery { code: "1234".into(), network: "home".into(), computer: "desk".into() });
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Esc)) else { panic!("kept open") };
        assert!(matches!(n.step, Step::Recovery(_)), "esc doesn't skip saving it");
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Char('s'))) else { panic!("kept open") };
        assert!(matches!(n.step, Step::List));
    }

    #[tokio::test]
    async fn a_recovery_code_that_comes_after_the_sheet_closed_opens_it_again() {
        let mut app = test_app();
        // Before the `network` event brings the new network: the step names it from the answer.
        app.network_reply(NetReply::Created, Ok(json!({"recoveryCode": "1234", "name": "work", "computer": "desk"})));
        let Some(Overlay::Network(n)) = app.overlays.last() else { panic!("the sheet is back") };
        assert!(matches!(&n.step, Step::Recovery(r) if (r.code.as_str(), r.network.as_str()) == ("1234", "work")));
    }

    #[tokio::test]
    async fn several_networks_switch_and_a_new_view_keeps_the_one_shown() {
        let mut app = test_app();
        let n = sheet(&[network("home", "owner", &json!([])), network("work", "member", &json!([]))]);
        assert_eq!(n.id(), "home");
        let Some(Overlay::Network(mut n)) = app.network_key(n, key(KeyCode::Char(']'))) else { panic!("kept open") };
        assert_eq!((n.id(), n.role()), (json!("work"), "member"));
        // Work comes first in the next view: the sheet stays on work.
        let view = json!({"networks": [network("work", "member", &json!([])), network("home", "owner", &json!([]))]});
        n.show(view);
        assert_eq!((n.net, n.id()), (0, json!("work")));
        // Creating or joining another is always there.
        let Some(Overlay::Network(n)) = app.network_key(n, key(KeyCode::Char('c'))) else { panic!("kept open") };
        assert!(matches!(n.step, Step::Create(_)));
    }
}
