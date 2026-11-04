//! The rules of one routine's flow without the database: which bots it reaches, the step each
//! runs in and which new link it accepts. A flow starts at the routine's own bot and follows the
//! links of that routine only.

use crate::store::FlowLink;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// How many hops from the routine's own bot a flow may reach, unless configured otherwise.
pub const DEFAULT_MAX_DEPTH: usize = 5;
/// The most a configured depth may be.
pub const MAX_DEPTH_LIMIT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    SameBot,
    IntoOwner,
    NotOnPath,
    Duplicate,
    Cycle,
    TooDeep(usize),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SameBot => f.write_str("a bot can't hand work to itself"),
            Self::IntoOwner => {
                f.write_str("the routine's own bot starts the flow, so nothing can hand work back to it")
            }
            Self::NotOnPath => {
                f.write_str("the flow doesn't reach that bot yet: connect it from the routine's bot first")
            }
            Self::Duplicate => f.write_str("these two bots are already connected in this routine"),
            Self::Cycle => f.write_str("that link would make the flow loop"),
            Self::TooDeep(max) => write!(f, "a flow reaches at most {max} bots deep"),
        }
    }
}

impl std::error::Error for Problem {}

/// The links form a cycle, so no order of steps exists.
#[derive(Debug, PartialEq, Eq)]
pub struct Cyclic;

/// The step each bot of the flow runs in: 0 for the routine's own bot and, for the rest, one more
/// than the deepest bot handing work to it. A bot the flow doesn't reach is absent.
pub fn depths(owner: &str, links: &[FlowLink]) -> Result<BTreeMap<String, usize>, Cyclic> {
    let reached = reachable(owner, links);
    let edges: Vec<&FlowLink> = links.iter().filter(|l| reached.contains(l.from_bot_id.as_str())).collect();
    let mut waiting: BTreeMap<&str, usize> = BTreeMap::new();
    for link in &edges {
        *waiting.entry(link.to_bot_id.as_str()).or_default() += 1;
    }
    if waiting.contains_key(owner) {
        return Err(Cyclic);
    }
    let mut depth = BTreeMap::from([(owner.to_owned(), 0)]);
    let mut ready = VecDeque::from([owner]);
    let mut settled = 1;
    while let Some(bot) = ready.pop_front() {
        let here = depth[bot];
        for link in edges.iter().filter(|l| l.from_bot_id == bot) {
            let to = link.to_bot_id.as_str();
            let best = depth.entry(to.to_owned()).or_insert(0);
            *best = (*best).max(here + 1);
            let left = waiting.get_mut(to).expect("every target was counted");
            *left -= 1;
            if *left == 0 {
                settled += 1;
                ready.push_back(to);
            }
        }
    }
    if settled == reached.len() { Ok(depth) } else { Err(Cyclic) }
}

fn reachable<'a>(start: &'a str, links: &'a [FlowLink]) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start];
    while let Some(bot) = queue.pop() {
        if seen.insert(bot) {
            queue.extend(links.iter().filter(|l| l.from_bot_id == bot).map(|l| l.to_bot_id.as_str()));
        }
    }
    seen
}

/// Every bot the flow reaches from the routine's own, the owner left out, even through a cycle.
pub fn reached(owner: &str, links: &[FlowLink]) -> Vec<String> {
    reachable(owner, links).into_iter().filter(|b| *b != owner).map(str::to_owned).collect()
}

/// Why `from` can't hand work to `to` in this routine, or `None` when it can.
pub fn connect_problem(owner: &str, links: &[FlowLink], from: &str, to: &str, max_depth: usize) -> Option<Problem> {
    if from == to {
        return Some(Problem::SameBot);
    }
    if to == owner {
        return Some(Problem::IntoOwner);
    }
    let Ok(current) = depths(owner, links) else { return Some(Problem::Cycle) };
    if !current.contains_key(from) {
        return Some(Problem::NotOnPath);
    }
    if links.iter().any(|l| l.from_bot_id == from && l.to_bot_id == to) {
        return Some(Problem::Duplicate);
    }
    if reachable(to, links).contains(from) {
        return Some(Problem::Cycle);
    }
    // The new hop puts `to` one step after `from`, and everything `to` hands work on moves with it.
    let deepest_after = longest_from(to, links);
    (current[from] + 1 + deepest_after > max_depth).then_some(Problem::TooDeep(max_depth))
}

/// The longest chain of hops that starts at `bot`.
fn longest_from(bot: &str, links: &[FlowLink]) -> usize {
    walk(bot, links, &mut BTreeMap::new())
}

fn walk<'a>(bot: &'a str, links: &'a [FlowLink], best: &mut BTreeMap<&'a str, usize>) -> usize {
    if let Some(known) = best.get(bot) {
        return *known;
    }
    // The links are acyclic here (the caller checked), so the walk ends.
    let longest =
        links.iter().filter(|l| l.from_bot_id == bot).map(|l| 1 + walk(&l.to_bot_id, links, best)).max().unwrap_or(0);
    best.insert(bot, longest);
    longest
}

/// The bots in the order their steps run: shallowest first, the routine's own bot left out.
pub fn order(depths: &BTreeMap<String, usize>) -> Vec<(&str, usize)> {
    let mut all: Vec<_> = depths.iter().filter(|(_, d)| **d > 0).map(|(b, d)| (b.as_str(), *d)).collect();
    all.sort_by_key(|(bot, depth)| (*depth, *bot));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(from: &str, to: &str) -> FlowLink {
        FlowLink {
            id: format!("{from}-{to}"),
            routine_id: "r".into(),
            owner_bot_id: "a".into(),
            from_bot_id: from.into(),
            to_bot_id: to.into(),
            instruction: String::new(),
            created_at: 0,
        }
    }

    #[test]
    fn depth_is_the_longest_path() {
        let links = [link("a", "b"), link("b", "c"), link("a", "c"), link("a", "d")];
        let d = depths("a", &links).unwrap();
        assert_eq!((d["a"], d["b"], d["c"], d["d"]), (0, 1, 2, 1));
        assert_eq!(order(&d), [("b", 1), ("d", 1), ("c", 2)]);
    }

    #[test]
    fn a_bot_the_flow_does_not_reach_is_absent() {
        let d = depths("a", &[link("a", "b"), link("x", "y")]).unwrap();
        assert!(!d.contains_key("x") && !d.contains_key("y"));
    }

    #[test]
    fn connecting_checks_every_rule() {
        let links = [link("a", "b"), link("b", "c")];
        let check = |from, to| connect_problem("a", &links, from, to, 5);
        assert_eq!(check("b", "b"), Some(Problem::SameBot));
        assert_eq!(check("c", "a"), Some(Problem::IntoOwner));
        assert_eq!(check("x", "d"), Some(Problem::NotOnPath));
        assert_eq!(check("a", "b"), Some(Problem::Duplicate));
        assert_eq!(check("c", "b"), Some(Problem::Cycle));
        assert_eq!(check("b", "d"), None);
        assert_eq!(check("a", "c"), None, "a second path to the same bot is a join");
    }

    #[test]
    fn a_cycle_in_stored_links_is_detected() {
        let links = [link("a", "b"), link("b", "c"), link("c", "b")];
        assert_eq!(depths("a", &links), Err(Cyclic));
        assert_eq!(connect_problem("a", &links, "b", "d", 5), Some(Problem::Cycle));
        assert_eq!(depths("a", &[link("a", "b"), link("b", "a")]), Err(Cyclic));
    }

    #[test]
    fn depth_is_limited_including_what_hangs_below_the_new_link() {
        let chain = [link("a", "b"), link("b", "c")];
        assert_eq!(connect_problem("a", &chain, "c", "d", 3), None);
        assert_eq!(connect_problem("a", &chain, "c", "d", 2), Some(Problem::TooDeep(2)));
        // Hanging a long branch under a shallow bot pushes the branch down.
        let links = [link("a", "b"), link("a", "x"), link("x", "y"), link("y", "z")];
        assert_eq!(connect_problem("a", &links, "b", "x", 3), Some(Problem::TooDeep(3)));
        assert_eq!(connect_problem("a", &links, "b", "x", 4), None);
    }
}
