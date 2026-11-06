//! The network sheet's form for creating a network or joining one with an invite.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Editor, cycle};
use super::super::l10n::tr;
use super::network::{NetReply, NetworkSheet, Step};

/// How computers reach each other without a direct path: what's shown, what the host takes
/// (empty: the relay address typed under it) and what it means. The desktop app's words.
pub const CONNECTORS: [(&str, &str, &str); 3] = [
    ("Public relays", "public", "Direct when it can, through a shared relay otherwise."),
    ("Private: direct only", "private", "Same network, Tailscale or a VPN."),
    ("Your own relay", "", "An iroh-relay you run."),
];

/// Creating a network, or joining one with an invite.
#[derive(Default)]
pub struct Setup {
    pub invite: Editor,
    /// The network's name: the new one's, or (joining) instead of the invite's.
    pub network: Editor,
    pub name: Editor,
    pub connector: usize,
    pub relay: Editor,
    pub field: SetupField,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum SetupField {
    Invite,
    #[default]
    Network,
    Name,
    Connector,
    Relay,
}

impl Setup {
    pub(super) fn join() -> Self {
        Self { field: SetupField::Invite, ..Self::default() }
    }

    /// The fields in order: joining starts with the invite, a relay URL only for your own relay.
    pub fn fields(&self, join: bool) -> Vec<SetupField> {
        let mut fields = vec![SetupField::Network, SetupField::Name, SetupField::Connector];
        if join {
            fields.insert(0, SetupField::Invite);
        }
        if CONNECTORS[self.connector].1.is_empty() {
            fields.push(SetupField::Relay);
        }
        fields
    }

    fn step(&mut self, join: bool, by: isize) {
        let fields = self.fields(join);
        let i = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = fields[cycle(i, by, fields.len())];
    }

    fn connector(&self) -> String {
        match CONNECTORS[self.connector].1 {
            "" => self.relay.text.trim().to_owned(),
            name => name.to_owned(),
        }
    }

    fn body(&self) -> Value {
        let given = |e: &Editor| Some(e.text.trim()).filter(|t| !t.is_empty()).map(str::to_owned);
        json!({
            "invite": self.invite.text.trim(), "network": given(&self.network), "name": given(&self.name),
            "connector": self.connector(),
        })
    }
}

impl App {
    /// Keys in the create or join form.
    pub(super) fn setup_key(&mut self, n: &mut NetworkSheet, mut s: Setup, join: bool, k: KeyEvent) -> Step {
        match k.code {
            KeyCode::Esc => return Step::List,
            // Already sent: a second one would only be refused.
            KeyCode::Enter if n.busy => {}
            KeyCode::Tab | KeyCode::Down => s.step(join, 1),
            KeyCode::BackTab | KeyCode::Up => s.step(join, -1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if s.field == SetupField::Connector => {
                let by = if k.code == KeyCode::Left { -1 } else { 1 };
                s.connector = cycle(s.connector, by, CONNECTORS.len());
            }
            KeyCode::Enter => {
                if join && s.invite.text.trim().is_empty() {
                    n.error = Some(tr("Paste the invite from the other computer.").into_owned());
                } else if s.connector().is_empty() {
                    n.error = Some(tr("Enter your relay's address.").into_owned());
                } else {
                    n.busy = true;
                    n.error = None;
                    if join {
                        self.net("networkJoin", s.body(), NetReply::View);
                    } else {
                        self.net("networkCreate", s.body(), NetReply::Created);
                    }
                }
            }
            _ => {
                let editor = match s.field {
                    SetupField::Invite => Some(&mut s.invite),
                    SetupField::Network => Some(&mut s.network),
                    SetupField::Name => Some(&mut s.name),
                    SetupField::Relay => Some(&mut s.relay),
                    SetupField::Connector => None,
                };
                if let Some(e) = editor {
                    e.key(k);
                }
            }
        }
        if join { Step::Join(s) } else { Step::Create(s) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_url_field_only_for_your_own_relay() {
        let mut s = Setup::join();
        assert_eq!(s.fields(true), [SetupField::Invite, SetupField::Network, SetupField::Name, SetupField::Connector]);
        s.connector = 2;
        assert_eq!(s.fields(false), [SetupField::Network, SetupField::Name, SetupField::Connector, SetupField::Relay]);
        s.relay = Editor::with(" https://relay.example ");
        assert_eq!(s.body()["connector"], "https://relay.example");
        assert_eq!(
            (&s.body()["name"], &s.body()["network"]),
            (&Value::Null, &Value::Null),
            "the defaults when left empty"
        );
    }
}
