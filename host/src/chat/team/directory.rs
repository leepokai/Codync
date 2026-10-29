//! `list_bots`: this computer's bots, then those the user's other computers offer, narrowed and
//! paged so a computer with many bots doesn't flood the agent's context.

use super::visible_bots;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::Result;
use codync_network::{AgentKind, Peer};
use serde::Deserialize;
use serde_json::{Value, json};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;
/// The `computer` that names the one the listing runs on, also before it is in a network.
const HERE: &str = "this";

struct Row {
    value: Value,
    /// Lowercase name and description, for `query`.
    words: String,
    /// The other computer it is on; `None` for this one.
    computer: Option<String>,
    online: bool,
    kind: AgentKind,
}

/// `keep` picks which of this computer's bots the caller may see.
pub(super) fn list(
    hub: &Hub,
    from: &str,
    args: &Value,
    remote: &[Peer],
    keep: impl Fn(&BotConfig) -> bool,
) -> Result<Value> {
    let filter = Filter::of(args);
    let here = hub.network.my_name();
    let rows: Vec<Row> =
        rows(hub, from, remote, keep)?.into_iter().filter(|r| filter.keeps(r, here.as_deref())).collect();
    Ok(page(&rows, args, !filter.narrows()))
}

/// Everyone `from` can message, nearest first: this computer's bots, then the ones on computers
/// that can take a message now.
fn rows(hub: &Hub, from: &str, remote: &[Peer], keep: impl Fn(&BotConfig) -> bool) -> Result<Vec<Row>> {
    let mut rows: Vec<Row> = visible_bots(&hub.store)?
        .into_iter()
        .filter(|c| c.id != from && keep(c))
        .map(|c| Row {
            words: format!("{} {}", c.name, c.description).to_lowercase(),
            value: json!({
                "id": c.id, "name": c.name, "description": c.description, "kind": AgentKind::Codync,
                "backend": c.backend, "cwd": c.cwd, "status": hub.runtime(&c.id).status,
            }),
            computer: None,
            online: true,
            kind: AgentKind::Codync,
        })
        .collect();
    let mut others: Vec<Row> = remote
        .iter()
        .flat_map(|p| {
            p.agents.iter().map(|a| Row {
                words: format!("{} {}", a.name, a.about).to_lowercase(),
                value: json!({
                    "id": format!("{}@{}", a.id, p.name), "name": a.name, "description": a.about, "kind": a.kind,
                    "computer": p.name, "status": if p.online { "online" } else { "away" },
                }),
                computer: Some(p.name.clone()),
                online: p.online,
                kind: a.kind,
            })
        })
        .collect();
    others.sort_by(|a, b| (!a.online, &a.computer).cmp(&(!b.online, &b.computer)));
    rows.extend(others);
    Ok(rows)
}

/// What `query`, `computer`, `online` and `kind` keep.
struct Filter {
    words: Vec<String>,
    computer: Option<String>,
    online: bool,
    /// A kind this version doesn't know reads as `other`, never as no filter.
    kind: Option<AgentKind>,
}

impl Filter {
    fn of(args: &Value) -> Self {
        Self {
            words: args["query"].as_str().unwrap_or_default().split_whitespace().map(str::to_lowercase).collect(),
            computer: args["computer"].as_str().map(str::to_lowercase),
            online: args["online"].as_bool() == Some(true),
            kind: AgentKind::deserialize(&args["kind"]).ok(),
        }
    }

    fn narrows(&self) -> bool {
        !self.words.is_empty() || self.computer.is_some() || self.online || self.kind.is_some()
    }

    /// `here` is this computer's name in its network.
    fn keeps(&self, row: &Row, here: Option<&str>) -> bool {
        let at = row.computer.as_deref().or(here).map(str::to_lowercase);
        // `desk` also finds `desk.home`, a computer in one of this computer's several networks.
        let at_computer =
            |c: &String| at.as_deref().is_some_and(|a| a == c || crate::network::split_computer(a).0 == c);
        let on = |c: &String| at_computer(c) || (row.computer.is_none() && c == HERE);
        self.words.iter().all(|w| row.words.contains(w))
            && self.computer.as_ref().is_none_or(on)
            && (!self.online || row.online)
            && self.kind.is_none_or(|kind| kind == row.kind)
    }
}

/// The `kind` argument of the tools' `list_bots`, which [`Filter::of`] reads.
pub(super) fn kind_property() -> Value {
    json!({
        "type": "string",
        "enum": ["codync", "claude-code", "codex", "terminal", "other"],
        "description": "Only this kind of agent: codync (Codync bots), claude-code (Claude Code sessions), codex (Codex threads), terminal (a person at a terminal on another computer) or other (on a computer whose Codync doesn't say its kind yet).",
    })
}

/// `limit` rows from `cursor`, with `next` for the rest; an unnarrowed listing that doesn't fit
/// also says how many bots each computer has.
fn page(rows: &[Row], args: &Value, summarize: bool) -> Value {
    let total = rows.len();
    let limit =
        args["limit"].as_u64().map_or(DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(MAX_LIMIT)).clamp(1, MAX_LIMIT);
    let start = args["cursor"].as_str().and_then(|c| c.parse::<usize>().ok()).unwrap_or(0).min(total);
    let end = (start + limit).min(total);
    let mut out = json!({"bots": rows[start..end].iter().map(|r| r.value.clone()).collect::<Vec<_>>(), "total": total});
    if end < total {
        out["next"] = end.to_string().into();
        if summarize {
            out["summary"] = summary(rows).into();
        }
    }
    out
}

/// How many bots each computer has, so the agent can narrow the list.
fn summary(rows: &[Row]) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for r in rows {
        let name = r.computer.clone().unwrap_or_else(|| "this computer".into());
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => counts.push((name, 1)),
        }
    }
    let parts: Vec<String> = counts.iter().map(|(name, n)| format!("{name} {n}")).collect();
    format!("{} bots: {}. Narrow with computer or query.", rows.len(), parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, about: &str, computer: Option<&str>, online: bool) -> Row {
        Row {
            value: json!({"id": name}),
            words: format!("{name} {about}").to_lowercase(),
            computer: computer.map(str::to_owned),
            online,
            kind: AgentKind::Codync,
        }
    }

    fn kept<'a>(rows: &'a [Row], args: &Value, here: Option<&str>) -> Vec<&'a str> {
        let filter = Filter::of(args);
        rows.iter().filter(|r| filter.keeps(r, here)).map(|r| r.value["id"].as_str().unwrap()).collect()
    }

    #[test]
    fn list_bots_narrows_and_pages() {
        let rows = [
            row("Coder", "Writes code", None, true),
            row("Code Reviewer", "Finds bugs in changes", None, true),
            row("Tester", "Runs the tests", Some("laptop"), false),
        ];
        assert_eq!(kept(&rows, &json!({"query": "BUGS changes"}), None), ["Code Reviewer"]);
        assert_eq!(kept(&rows, &json!({"computer": "Laptop"}), Some("desk")), ["Tester"]);
        assert_eq!(kept(&rows, &json!({"computer": "desk"}), Some("desk")), ["Coder", "Code Reviewer"]);
        assert_eq!(kept(&rows, &json!({"computer": HERE}), None), ["Coder", "Code Reviewer"]);
        assert_eq!(kept(&rows, &json!({"online": true}), None), ["Coder", "Code Reviewer"]);
        assert_eq!(kept(&rows, &json!({"computer": "elsewhere"}), None), Vec::<&str>::new());
        // In several networks, a computer is named with its network: its bare name still finds it.
        let several = [row("Lead", "", Some("laptop.home"), true), row("Ops", "", Some("laptop-2.work"), true)];
        assert_eq!(kept(&several, &json!({"computer": "laptop"}), None), ["Lead"]);
        assert_eq!(kept(&several, &json!({"computer": "laptop-2.work"}), None), ["Ops"]);

        let first = page(&rows, &json!({"limit": 2}), true);
        assert_eq!((first["total"].as_u64(), first["next"].as_str()), (Some(3), Some("2")));
        assert_eq!(first["summary"], "3 bots: this computer 2, laptop 1. Narrow with computer or query.");
        let rest = page(&rows, &json!({"limit": 2, "cursor": "2"}), true);
        assert!(rest.get("next").is_none() && rest["bots"].as_array().unwrap().len() == 1);
        assert!(page(&rows, &json!({"limit": 2}), false).get("summary").is_none());
    }

    #[test]
    fn list_bots_picks_a_kind_of_agent() {
        let mut session = row("main", "", Some("laptop"), true);
        session.kind = AgentKind::ClaudeCode;
        let mut thread = row("refactor", "", Some("laptop"), true);
        thread.kind = AgentKind::Codex;
        let rows = [row("Coder", "", None, true), session, thread];
        assert_eq!(kept(&rows, &json!({"kind": "codync"}), None), ["Coder"]);
        assert_eq!(kept(&rows, &json!({"kind": "claude-code"}), None), ["main"]);
        assert_eq!(kept(&rows, &json!({"kind": "codex", "computer": "laptop"}), None), ["refactor"]);
        assert_eq!(
            kept(&rows, &json!({"kind": "gemini"}), None),
            Vec::<&str>::new(),
            "an unknown kind isn't no filter"
        );
        assert!(Filter::of(&json!({"kind": "codex"})).narrows());
    }
}
