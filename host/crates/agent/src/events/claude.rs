//! What Claude's adapter (claude-agent-acp) adds to a tool call in `_meta.claudeCode`: which subagent a call belongs
//! to, and what a subagent's launch reports. Without the `subagents` client capability the adapter runs a subagent
//! as the parent's `Agent` tool call, and the subagent's own calls arrive on the same session.

use serde_json::Value;

/// A subagent, as its launching `Agent` call reports it. The facts come over several updates (what it was asked as
/// the call streams in, the totals when it ends), so each field is `Some` only in the update that carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Subagent {
    /// The adapter's id for it (`agentId`).
    pub id: Option<String>,
    /// Its agent type, such as `general-purpose`.
    pub kind: Option<String>,
    /// The model it ran on.
    pub model: Option<String>,
    /// Launched to run in the background: it reports no totals.
    pub background: Option<bool>,
    /// Tokens it used in all, once it finished.
    pub tokens: Option<u64>,
    /// How many tools it called, once it finished.
    pub tool_uses: Option<u64>,
    /// How long it ran, once it finished.
    pub duration_ms: Option<u64>,
}

/// The id of the `Agent` call whose subagent made this call (`parentToolUseId`).
pub(super) fn parent(u: &Value) -> Option<String> {
    u["_meta"]["claudeCode"]["parentToolUseId"].as_str().map(str::to_owned)
}

/// What an `Agent` call's update says about its subagent: its `rawInput` as it streams in, its `toolResponse` once
/// launched (in the background) or finished. `None` for any other call, or an update that says nothing about it.
pub(super) fn subagent(u: &Value) -> Option<Subagent> {
    let meta = &u["_meta"]["claudeCode"];
    if meta["toolName"] != "Agent" {
        return None;
    }
    let (input, response) = (&u["rawInput"], &meta["toolResponse"]);
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let found = Subagent {
        id: text(&response["agentId"]),
        kind: text(&response["agentType"]).or_else(|| text(&input["subagent_type"])),
        model: text(&response["resolvedModel"]),
        background: response["isAsync"].as_bool().or_else(|| input["run_in_background"].as_bool()),
        tokens: response["totalTokens"].as_u64(),
        tool_uses: response["totalToolUseCount"].as_u64(),
        duration_ms: response["totalDurationMs"].as_u64(),
    };
    (found != Subagent::default()).then_some(found)
}
