//! Who offers what: the last word from every other computer, and finding an agent by name.

use super::{Inner, locked};
use crate::roster::Role;
use crate::types::{Address, Peer};
use crate::wire::Agent;
use anyhow::{Result, anyhow, bail};
use iroh::EndpointId;

/// What a computer offered when it last answered, and whether the last try got through.
pub(super) struct Seen {
    agents: Vec<Agent>,
    answered: bool,
}

/// The computer name an address uses for this one before it is in a network.
const LOCAL: &str = "local";

impl Inner {
    /// Notes that `id` answered with `agents`, or (`None`) that it didn't.
    pub(super) fn saw(&self, id: EndpointId, agents: Option<Vec<Agent>>) {
        let mut seen = locked(&self.seen);
        match agents {
            Some(agents) => {
                seen.insert(id, Seen { agents, answered: true });
            }
            None => {
                seen.entry(id).or_insert(Seen { agents: Vec::new(), answered: false }).answered = false;
            }
        }
    }

    /// This computer, then every other member as last heard, by name.
    pub(super) fn directory(&self) -> Vec<Peer> {
        let me = self.me();
        let roster = locked(&self.roster).clone();
        let mine = Peer {
            id: me,
            name: roster.as_ref().map_or_else(|| LOCAL.into(), |r| r.name(&me)),
            role: roster.as_ref().and_then(|r| r.role(&me)).unwrap_or(Role::Owner),
            me: true,
            online: true,
            agents: self.offered(),
        };
        let seen = locked(&self.seen);
        let mut others: Vec<Peer> = roster
            .iter()
            .flat_map(|r| r.members().iter())
            .filter(|(id, _)| **id != me)
            .map(|(id, m)| {
                let heard = seen.get(id);
                Peer {
                    id: *id,
                    name: m.name.clone(),
                    role: m.role,
                    me: false,
                    online: heard.is_some_and(|s| s.answered),
                    agents: heard.map(|s| s.agents.clone()).unwrap_or_default(),
                }
            })
            .collect();
        others.sort_by(|a, b| a.name.cmp(&b.name));
        std::iter::once(mine).chain(others).collect()
    }

    pub(super) fn resolve(&self, name: &str) -> Result<Address> {
        let name = name.trim();
        let peers = self.directory();
        if name.contains('@') {
            let address: Address = name.parse()?;
            let peer = peers
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&address.computer))
                .ok_or_else(|| anyhow!("no computer called {}", address.computer))?;
            return Ok(Address { agent: address.agent, computer: peer.name.clone() });
        }
        // Nearest first: this computer's own agents win.
        let found: Vec<Address> = peers
            .iter()
            .filter(|p| p.agents.iter().any(|a| a.id == name))
            .map(|p| Address { agent: name.into(), computer: p.name.clone() })
            .collect();
        match &found[..] {
            [] => bail!("no agent called {name}; list the agents to see who is around"),
            [only] => Ok(only.clone()),
            [first, ..] if peers[0].agents.iter().any(|a| a.id == name) => Ok(first.clone()),
            many => {
                let names: Vec<String> = many.iter().map(ToString::to_string).collect();
                bail!("more than one agent is called {name}: {}; use the full address", names.join(", "))
            }
        }
    }
}
