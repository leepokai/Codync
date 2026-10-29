//! Who is in the network: a log of signed changes that every member keeps a copy of.
//!
//! Copies merge (a union of entries), and every computer works out the same members from them,
//! whatever order the entries arrived in. Each entry is signed by the computer that made it, and
//! it counts only if that computer could make it:
//!
//! - the network's **root key** (the recovery code, kept on no computer) names the owner;
//! - the **owner** promotes admins, and admits and removes anyone;
//! - **admins** admit computers and remove plain members;
//! - every computer writes only where it is reachable.
//!
//! A removed computer stays removed: coming back means joining again with a new key.
//!
//! Every copy keeps only the entries that can count, so the whole log fits in the one frame a
//! hello carries it in, whatever a stolen computer signs.

use crate::wire;
use anyhow::{Context, Result, anyhow, bail};
use iroh::{EndpointAddr, EndpointId, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const FILE: &str = "network.json";
/// Entries one computer may have in the log: far more changes than a network of computers makes,
/// and few enough that the log stays well inside a frame.
const MAX_PER_SIGNER: usize = 256;
/// What one entry may hold, so no member can grow the log past what a hello carries.
const MAX_ENTRY: usize = 8 * 1024;
/// The addresses a location keeps: the relay first (`TransportAddr` orders relays before IP
/// addresses), then the direct ones.
const MAX_ADDRS: usize = 64;
/// A computer's name (`node::check_name`'s 40) with the number `free_name` adds.
const MAX_NAME: usize = 48;
/// What a computer's name never holds: they'd break `agent@computer.network`.
pub(crate) const NOT_IN_NAMES: [char; 4] = ['/', ' ', '@', '.'];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Member,
    Admin,
    Owner,
}

impl Role {
    /// Whether a computer with this role may invite computers: the owner and admins.
    pub fn invites(self) -> bool {
        self >= Role::Admin
    }

    /// Whether a computer with this role may remove one with `other`: the owner removes anyone,
    /// admins remove plain members.
    pub fn removes(self, other: Role) -> bool {
        self == Role::Owner || (self == Role::Admin && other == Role::Member)
    }

    /// Whether a computer with this role may make one with `other` an admin: only the owner, and
    /// only a plain member.
    pub fn promotes(self, other: Role) -> bool {
        self == Role::Owner && other == Role::Member
    }
}

/// One computer in the network, as the log has it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub role: Role,
    /// Where it last said it was reachable.
    pub addr: EndpointAddr,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    /// The root key names the owner (again, to move it to another computer).
    Owner {
        id: EndpointId,
        name: String,
    },
    Admit {
        id: EndpointId,
        name: String,
    },
    Promote {
        id: EndpointId,
    },
    /// `cascade` also voids everything the removed computer signed (a stolen one).
    Remove {
        id: EndpointId,
        cascade: bool,
    },
    /// Where the signing computer is reachable.
    Locate {
        addr: EndpointAddr,
    },
}

/// `addr` cut to the addresses a location may hold: what this computer signs, and compares with
/// the location it signed before.
pub fn bounded_addr(addr: EndpointAddr) -> EndpointAddr {
    EndpointAddr::from_parts(addr.id, addr.addrs.into_iter().take(MAX_ADDRS))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Signed {
    /// When it was signed (Unix ms), as its signer says.
    pub at: u64,
    pub by: EndpointId,
    pub change: Change,
    /// Hex ed25519 signature over the network, signer, time and change.
    pub sig: String,
}

impl Signed {
    pub fn new(network: &EndpointId, key: &SecretKey, at: u64, change: Change) -> Self {
        let by = key.public();
        let sig = wire::hex(&key.sign(&body(network, &by, at, &change)).to_bytes());
        Self { at, by, change, sig }
    }

    fn verifies(&self, network: &EndpointId) -> bool {
        let Some(Ok(bytes)) = wire::unhex(&self.sig).map(<[u8; Signature::LENGTH]>::try_from) else { return false };
        self.by.verify(&body(network, &self.by, self.at, &self.change), &Signature::from_bytes(&bytes)).is_ok()
    }
}

fn body(network: &EndpointId, by: &EndpointId, at: u64, change: &Change) -> Vec<u8> {
    serde_json::to_vec(&(network, by, at, change)).expect("a change always serializes")
}

/// The network as this computer knows it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Roster {
    /// The network's id: its root key.
    pub network: EndpointId,
    log: BTreeSet<Signed>,
    #[serde(skip)]
    members: BTreeMap<EndpointId, Member>,
    #[serde(skip)]
    removed: BTreeSet<EndpointId>,
}

impl PartialEq for Roster {
    fn eq(&self, other: &Self) -> bool {
        self.network == other.network && self.log == other.log
    }
}

impl Roster {
    /// A new network with `me` as its owner. Returns it with the root key, which the caller
    /// shows once as the recovery code and keeps nowhere.
    pub fn create(me: &SecretKey, name: String, addr: EndpointAddr, at: u64) -> (Self, SecretKey) {
        let root = SecretKey::generate();
        let network = root.public();
        let mut roster = Self { network, log: BTreeSet::new(), members: BTreeMap::new(), removed: BTreeSet::new() };
        roster.log.insert(Signed::new(&network, &root, at, Change::Owner { id: me.public(), name }));
        roster.log.insert(Signed::new(&network, me, at, Change::Locate { addr: bounded_addr(addr) }));
        roster.rebuild();
        (roster, root)
    }

    /// Signs `change` as `key` and adds it, if `key` may make it.
    pub fn sign(&mut self, key: &SecretKey, at: u64, change: Change) -> Result<()> {
        let entry = Signed::new(&self.network, key, at, change);
        let before = self.clone();
        self.log.insert(entry);
        self.rebuild();
        if self.members == before.members && self.removed == before.removed {
            *self = before;
            bail!("this computer isn't allowed to make that change");
        }
        Ok(())
    }

    pub fn members(&self) -> &BTreeMap<EndpointId, Member> {
        &self.members
    }

    pub fn is_member(&self, id: &EndpointId) -> bool {
        self.members.contains_key(id)
    }

    pub fn is_removed(&self, id: &EndpointId) -> bool {
        self.removed.contains(id)
    }

    pub fn role(&self, id: &EndpointId) -> Option<Role> {
        self.members.get(id).map(|m| m.role)
    }

    /// The member called `name`, or whose id starts with it.
    pub fn find(&self, name: &str) -> Option<EndpointId> {
        let name = name.trim();
        self.members
            .iter()
            .find(|(_, m)| m.name.eq_ignore_ascii_case(name))
            .or_else(|| self.members.iter().find(|(id, _)| name.len() >= 6 && id.to_string().starts_with(name)))
            .map(|(id, _)| *id)
    }

    pub fn name(&self, id: &EndpointId) -> String {
        self.members.get(id).map_or_else(|| id.fmt_short().to_string(), |m| m.name.clone())
    }

    /// `name`, or the first of `name-2`, `name-3`… no other computer has.
    pub fn free_name(&self, name: &str, for_id: &EndpointId) -> String {
        let taken = |n: &str| self.find(n).is_some_and(|id| id != *for_id);
        if !taken(name) {
            return name.into();
        }
        // One more number than there are members: one of them is free.
        (2..=self.members.len() + 2).map(|n| format!("{name}-{n}")).find(|n| !taken(n)).expect("a number is free")
    }

    /// Takes in another member's copy: the entries that verify. Returns whether anything changed.
    pub fn merge(&mut self, other: &Roster) -> bool {
        if other.network != self.network {
            return false;
        }
        let new: Vec<Signed> =
            other.log.iter().filter(|e| !self.log.contains(e) && e.verifies(&self.network)).cloned().collect();
        if new.is_empty() {
            return false;
        }
        let before = self.log.clone();
        self.log.extend(new);
        self.rebuild();
        self.log != before
    }

    /// Works out the members from the log, then drops the entries that can't count.
    fn rebuild(&mut self) {
        let known = self.count();
        if self.compact(&known) {
            self.count();
        }
    }

    /// Drops what can't count, so the log stays small enough to send whole: entries out of bounds,
    /// entries by computers the log never let in, changes to who is in by computers that may not
    /// make them, all but each computer's newest location (and none of a removed one's), and
    /// whatever a computer signed past its share. Returns whether anything went.
    fn compact(&mut self, known: &Known) -> bool {
        let before = self.log.len();
        self.log.retain(bounded);
        let newest = newest_locations(&self.log);
        let mut share: BTreeMap<EndpointId, usize> = BTreeMap::new();
        let kept: BTreeSet<Signed> = self
            .log
            .iter()
            .filter(|s| {
                let counts = known.signers.contains(&s.by)
                    && match &s.change {
                        Change::Owner { .. } => s.by == self.network,
                        Change::Locate { .. } => {
                            !self.removed.contains(&s.by) && newest.get(&s.by).is_some_and(|(n, _)| *n == *s)
                        }
                        _ => known.deciders.contains(&s.by),
                    };
                let n = share.entry(s.by).or_default();
                *n += usize::from(counts);
                counts && *n <= MAX_PER_SIGNER
            })
            .cloned()
            .collect();
        let changed = kept.len() != before;
        self.log = kept;
        changed
    }

    /// Works out the members from the log. Who may sign what depends only on entries from above
    /// (the root, then the owners, latest first, then admins), so the result doesn't depend on
    /// arrival order. Returns who may have entries in the log.
    fn count(&mut self) -> Known {
        let root = self.network;
        let log: Vec<&Signed> = self.log.iter().collect();
        // Owners in turn, each from the time the root named it.
        let mut owners: Vec<(u64, EndpointId, String)> = log
            .iter()
            .filter(|s| s.by == root)
            .filter_map(|s| match &s.change {
                Change::Owner { id, name } => Some((s.at, *id, name.clone())),
                _ => None,
            })
            .collect();
        owners.sort();
        let owner_at = |at: u64| owners.iter().rev().find(|(from, ..)| *from <= at).map(|(_, id, _)| *id);
        let top = |by: &EndpointId, at: u64| *by == root || owner_at(at) == Some(*by);
        let owner = owners.last().map(|(_, id, _)| *id);

        // Removals by the top: the root's, then each owner's from the latest named back. An owner
        // that one above it removed with `cascade` (a stolen one) removes nobody, whatever time it
        // signs with, and only the root removes the current owner.
        let mut above = vec![root];
        for (_, id, _) in owners.iter().rev() {
            if !above.contains(id) {
                above.push(*id);
            }
        }
        let mut top_removed: BTreeMap<EndpointId, (u64, bool)> = BTreeMap::new();
        for by in &above {
            if top_removed.get(by).is_some_and(|&(_, cascade)| cascade) {
                continue;
            }
            for s in log.iter().filter(|s| s.by == *by && top(&s.by, s.at)) {
                if let Change::Remove { id, cascade } = &s.change
                    && (s.by == root || (Some(*id) != owner_at(s.at) && Some(*id) != owner))
                {
                    note_removal(&mut top_removed, *id, s.at, *cascade);
                }
            }
        }
        let top_voided = |by: &EndpointId| top_removed.get(by).is_some_and(|&(_, cascade)| cascade);
        let mut promoted: BTreeMap<EndpointId, u64> = BTreeMap::new();
        for s in &log {
            if let Change::Promote { id } = &s.change
                && top(&s.by, s.at)
                && !top_voided(&s.by)
            {
                let at = promoted.entry(*id).or_insert(s.at);
                *at = (*at).min(s.at);
            }
        }
        // Admins, within the time span they could sign in.
        let admin_at = |id: &EndpointId, at: u64| {
            promoted.get(id).is_some_and(|&from| {
                from <= at && top_removed.get(id).is_none_or(|&(until, cascade)| !cascade && at < until)
            })
        };
        let mut removed = top_removed.clone();
        for s in &log {
            if let Change::Remove { id, cascade } = &s.change
                && admin_at(&s.by, s.at)
                && Some(*id) != owner_at(s.at)
                && !promoted.contains_key(id)
                && !top_removed.contains_key(id)
            {
                note_removal(&mut removed, *id, s.at, *cascade);
            }
        }
        // A cascade-removed computer voids what it admitted, too.
        let voided = |by: &EndpointId| removed.get(by).is_some_and(|&(_, cascade)| cascade);
        let may_admit = |s: &Signed| !voided(&s.by) && (top(&s.by, s.at) || admin_at(&s.by, s.at));

        // Each computer with the time it got in and the name it got in with.
        let mut admitted: BTreeMap<EndpointId, (u64, String)> = BTreeMap::new();
        for (at, id, name) in &owners {
            admitted.entry(*id).or_insert_with(|| (*at, name.clone()));
        }
        for s in &log {
            if let Change::Admit { id, name } = &s.change
                && may_admit(s)
            {
                let first = admitted.entry(*id).or_insert_with(|| (s.at, name.clone()));
                if s.at < first.0 {
                    *first = (s.at, name.clone());
                }
            }
        }
        let addrs = newest_locations(&self.log);
        let tops = std::iter::once(root).chain(owners.iter().map(|(_, id, _)| *id));
        let known = Known {
            signers: tops.clone().chain(admitted.keys().copied()).collect(),
            deciders: tops.chain(promoted.keys().copied()).collect(),
        };
        self.removed = removed.keys().copied().collect();
        let removed_at: BTreeMap<EndpointId, u64> = removed.iter().map(|(id, &(at, _))| (*id, at)).collect();
        self.members = distinct_names(admitted, &removed_at)
            .into_iter()
            .filter(|(id, _)| !self.removed.contains(id))
            .map(|(id, name)| {
                let role = if Some(id) == owner {
                    Role::Owner
                } else if admin_at(&id, u64::MAX) {
                    Role::Admin
                } else {
                    Role::Member
                };
                let addr = addrs.get(&id).map_or_else(|| EndpointAddr::new(id), |(_, a)| (*a).clone());
                (id, Member { name, role, addr })
            })
            .collect();
        known
    }

    /// A copy that came from disk or another computer: only the entries that verify count.
    #[must_use]
    pub fn verified(mut self) -> Self {
        let network = self.network;
        self.log.retain(|s| s.verifies(&network));
        self.rebuild();
        self
    }

    pub fn load(dir: &Path) -> Result<Option<Self>> {
        match std::fs::read(dir.join(FILE)) {
            Ok(bytes) => {
                let roster: Self = serde_json::from_slice(&bytes).context("network.json is damaged")?;
                Ok(Some(roster.verified()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("couldn't read network.json"),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, dir.join(FILE)).context("couldn't save network.json")
    }
}

/// Who may have entries in the log.
struct Known {
    /// The root, the owners and every computer the log let in (removed ones too).
    signers: BTreeSet<EndpointId>,
    /// Those that may change who is in: the root, the owners and the admins.
    deciders: BTreeSet<EndpointId>,
}

/// Whether an entry is within bounds: a name a computer may have, a location's few addresses, and
/// a few KiB in all. Anything past them can't count, whoever signed it.
fn bounded(s: &Signed) -> bool {
    let fits = match &s.change {
        Change::Owner { name, .. } | Change::Admit { name, .. } => {
            !name.is_empty() && name.len() <= MAX_NAME && !name.contains(NOT_IN_NAMES)
        }
        Change::Locate { addr } => addr.addrs.len() <= MAX_ADDRS,
        Change::Promote { .. } | Change::Remove { .. } => true,
    };
    fits && serde_json::to_vec(s).is_ok_and(|bytes| bytes.len() <= MAX_ENTRY)
}

/// Where each computer last said it is reachable: the entry and the address. Only that one counts.
fn newest_locations(log: &BTreeSet<Signed>) -> BTreeMap<EndpointId, (&Signed, &EndpointAddr)> {
    let mut newest: BTreeMap<EndpointId, (&Signed, &EndpointAddr)> = BTreeMap::new();
    for s in log {
        if let Change::Locate { addr } = &s.change
            && addr.id == s.by
            && newest.get(&s.by).is_none_or(|(n, _)| s.at > n.at)
        {
            newest.insert(s.by, (s, addr));
        }
    }
    newest
}

/// Names every computer uniquely. Two admins can admit a `laptop` each before their copies meet:
/// the one admitted later (by time, then id) gets the first free number, the same on every copy. A
/// removed computer keeps its name from the computers admitted while it was in, so no one's name
/// moves when it goes (letters on their way still name them); one admitted after can take it.
fn distinct_names(
    admitted: BTreeMap<EndpointId, (u64, String)>,
    removed_at: &BTreeMap<EndpointId, u64>,
) -> BTreeMap<EndpointId, String> {
    let mut order: Vec<(u64, EndpointId, String)> =
        admitted.into_iter().map(|(id, (at, name))| (at, id, name)).collect();
    order.sort();
    // Each name given, and until when it is held: for good, or until its computer was removed.
    let mut held: BTreeMap<String, Option<u64>> = BTreeMap::new();
    let mut names = BTreeMap::new();
    for (at, id, name) in order {
        let free = |n: &str| held.get(&n.to_lowercase()).is_none_or(|until| until.is_some_and(|t| t < at));
        let unique = std::iter::once(name.clone())
            // One more number than there are names: one of them is free.
            .chain((2..=held.len() + 2).map(|n| format!("{name}-{n}")))
            .find(|n| free(n))
            .expect("a number is free");
        held.insert(unique.to_lowercase(), removed_at.get(&id).copied());
        names.insert(id, unique);
    }
    names
}

/// Notes that `id` was removed at `at`: the earliest removal counts, and a cascade from any.
fn note_removal(removed: &mut BTreeMap<EndpointId, (u64, bool)>, id: EndpointId, at: u64, cascade: bool) {
    let e = removed.entry(id).or_insert((at, cascade));
    *e = (e.0.min(at), e.1 || cascade);
}

/// The recovery code for a root key, and back.
pub fn recovery_code(root: &SecretKey) -> String {
    let hex = wire::hex(&root.to_bytes());
    hex.as_bytes().chunks(8).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
}

pub fn root_from_code(code: &str) -> Result<SecretKey> {
    let hex: String = code.chars().filter(char::is_ascii_hexdigit).collect();
    let bytes: [u8; 32] = wire::unhex(&hex)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| anyhow!("a recovery code is 64 hex characters"))?;
    Ok(SecretKey::from_bytes(&bytes))
}

#[cfg(test)]
mod tests;
