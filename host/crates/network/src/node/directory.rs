//! Who offers what: the last word from every other computer, and finding an agent by name.

use super::{Inner, locked};
use crate::roster::Role;
use crate::types::{Address, Peer};
use crate::wire::Agent;
use anyhow::{Result, anyhow, bail};
use iroh::EndpointId;

/// What a computer offered when it last answered, and whether the last try got through.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Seen {
    agents: Vec<Agent>,
    answered: bool,
}

/// The computer name an address uses for this one before it is in a network.
const LOCAL: &str = "local";
/// What another computer's offer may hold before the rest is left out: these show in every app
/// and in agents' prompts.
const MAX_AGENTS: usize = 200;
const MAX_AGENT_NAME: usize = 80;
const MAX_ABOUT: usize = 1024;

/// Another computer's agents within bounds: the first [`MAX_AGENTS`] with an id a letter can
/// carry ([`wire::MAX_ADDRESS`](crate::wire::MAX_ADDRESS)), their names and descriptions cut to length.
fn bounded(agents: Vec<Agent>) -> Vec<Agent> {
    let cut = |text: &mut String, max: usize| {
        if text.len() > max {
            let end = (0..=max).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
            text.truncate(end);
        }
    };
    agents
        .into_iter()
        .filter(|a| !a.id.is_empty() && a.id.len() <= crate::wire::MAX_ADDRESS)
        .take(MAX_AGENTS)
        .map(|mut a| {
            cut(&mut a.name, MAX_AGENT_NAME);
            cut(&mut a.about, MAX_ABOUT);
            a
        })
        .collect()
}

impl Inner {
    /// Notes that `id` answered with `agents`, or (`None`) that it didn't.
    pub(super) fn saw(&self, id: EndpointId, agents: Option<Vec<Agent>>) {
        let mut seen = locked(&self.seen);
        let changed = if let Some(agents) = agents {
            let now = Seen { agents: bounded(agents), answered: true };
            seen.insert(id, now.clone()).is_none_or(|before| before != now)
        } else {
            let heard = seen.entry(id).or_insert(Seen { agents: Vec::new(), answered: false });
            std::mem::replace(&mut heard.answered, false)
        };
        drop(seen);
        if changed {
            self.changed.send_replace(());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::AgentKind;

    #[test]
    fn another_computers_offer_is_cut_to_bounds() {
        let agent = |id: String, name: String, about: String| Agent { id, name, about, kind: AgentKind::Codync };
        let mut offer = vec![agent("x".repeat(257), "an id no letter carries".into(), String::new())];
        // Forty characters of any script is an address name a letter carries.
        offer.push(agent("審".repeat(40), "審計".into(), String::new()));
        offer.extend((0..300).map(|n| agent(format!("bot-{n}"), "é".repeat(100), "a".repeat(5000))));
        let kept = bounded(offer);
        assert_eq!(kept.len(), MAX_AGENTS);
        assert_eq!(kept[0].id, "審".repeat(40));
        assert_eq!(kept[1].id, "bot-0");
        assert!(kept[1].name.len() <= MAX_AGENT_NAME && kept[1].name.chars().all(|c| c == 'é'));
        assert_eq!(kept[1].about.len(), MAX_ABOUT);
    }
}
