# Gaps in ACP itself

Not one agent's bug: the protocol leaves these open, and the host covers them for every agent. Rules:
[README](README.md).

- No structured MCP server and tool name on a tool call: `host/src/agent/bot/tool_entry.rs` parses titles (`mcp__x__y`, `mcp.x.y`).
- Every adapter names its error category differently (`errorKind`, `category`, `codexErrorInfo`): `host/src/agent/acp.rs`.
- Agents that ignore `session/cancel` are killed after 15 s; a hung `session/new`, `session/load` or `session/resume` after 3 min;
  an old CLI that hangs on `initialize` gets 20 s when another way to start it is left (`host/src/agent/bot/mod.rs`,
  `host/src/agent/bot/session.rs`).
- Agents that send no `compaction_update`: a drop in context use by more than half stands in for one
  (`host/src/agent/bot/compaction.rs`).
- Mode ids have no fixed meaning: Auto is found by id or name, and Goose's `auto` (approves everything) is hidden
  (`host/src/agent/modes.rs`).
- No way in ACP v1 to hand a running turn another message: *Send now* uses the agents' own `_session/steering`
  (advertised in the top-level `_meta.steering.supported`; claude-acp, codex-acp, omp) with `idleBehavior:
  "promptRequired"`, and stops the turn for agents without it (`host/src/agent/bot/steer.rs`). Move to the standard
  once ACP v2's prompts during active work and the `delivery` RFD
  ([agent-client-protocol#1261](https://github.com/agentclientprotocol/agent-client-protocol/pull/1261)) are stable.
