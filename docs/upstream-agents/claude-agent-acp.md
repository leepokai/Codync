# Claude Code (claude-agent-acp)

Upstream: [agentclientprotocol/claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp). Rules:
[README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| The standard `systemPrompt` field drops the `_meta.claudeCode.options`; ACP can't change a session's system prompt | `host/src/agent/bot/mod.rs`, `host/src/agent/bot/session.rs`: always `_meta.systemPrompt`; after a compaction, `session/load` again to swap the prompt in | none | The adapter merges both, and ACP can update a system prompt |
| After an internal error with no category, the session stays dead ("The Claude Agent session has ended") | `host/crates/agent/src/acp.rs`, `host/src/agent/bot/lifecycle.rs`: `session/close` so the next turn loads it again (any adapter's uncategorized -32603) | none | The adapter restarts its query stream |
| Defers MCP tools behind tool search: a bot called `message_bot` without its schema ("botId is required") | `host/src/chat/team.rs`: `list_bots` and `message_bot` carry `_meta {"anthropic/alwaysLoad": true}` | none | Keep: two small tools a bot needs in every turn |
| Opens a session without checking the login | `host/src/agent/auth.rs`: for agents with `_meta.authStatus`, only `_auth/status_update` counts | none | A session fails when signed out |
| Gives its own `claude auth status` only 5 s, too few on many Windows computers | `host/src/agent/auth.rs`, `host/crates/agent/src/backends/sign_in.rs`: waits 8 s, then runs the check itself for 15 s | [investigation](../investigations/windows-claude-sign-in-2026-10-20.md) | A longer budget, or it always reports |
| `claude auth status` still says signed in when the token was revoked | `host/crates/agent/src/backends/sign_in.rs`, `host/src/agent/bot/sign_in.rs`: a prompt refused for auth wins until a sign-in finishes | none | The status check notices |
| The process keeps the credentials it started with | `host/src/agent/bot/sign_in.rs`: restart after a turn that failed signing in | none | The adapter reads them again |
| Killed during an OAuth refresh, it loses the new refresh token and with it the login | `host/crates/agent/src/acp.rs`: close stdin first, 10 s `EXIT_GRACE` | none | Keep: a graceful stop costs nothing |
| A usage limit arrives as `rate_limit` / `billing_error` text | `host/src/agent/fallback/reason.rs`: matches the Agent SDK's usage-limit prefixes | none | A distinct `errorKind` |
| Why Fast mode is refused is only in the option's description | `host/crates/agent/src/config.rs`, `host/src/agent/bot/fast_mode.rs`: split on `" — "` | none | A structured reason |
| The compaction summary rides on one of several `completed` `compaction_update`s | `host/crates/agent/src/events.rs` (reads `compactionId`), `host/src/agent/bot/updates.rs`, `host/src/agent/bot/compaction.rs`: dedupe by `compactionId`, attach a late summary | none | One update per compaction |
| Behind a gateway, a stream's start (`message_start` / `response.created`) becomes a `usage_update` with `used: 0` before the model answers ([#496](https://github.com/leepokai/Codync/issues/496)) | `host/src/agent/bot/compaction.rs`: `record_usage` ignores a zero, so it is neither a fall read as a compaction nor the saved baseline | none | The adapter sends usage only once the model reports it |
| A background subagent (`isAsync`) never says that it finished ([#634](https://github.com/leepokai/Codync/issues/634)) | `host/src/agent/bot/background_work.rs` (`left_running`): a subagent launched in the background holds its thread's task only in the turn that launched it, so the thread is asked once to wait for it, and a held turn that ends again isn't failed for it. Not yet checked against a real Claude run: whether a background shell (`run_in_background`) runs detached, so the host sees it | none | The adapter reports a background task's end, or that one still runs as its turn ends |
| `claude -p /usage` routed off the subscription (a gateway's `ANTHROPIC_BASE_URL` and key or token, or a `CLAUDE_CODE_USE_*` provider switch, in the environment or its settings' `env`) reports token costs with no plan limits, so the shown limits went stale (2.1.327) | `host/crates/agent/src/usage/claude.rs`: the check passes `--settings` with `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX` and `CLAUDE_CODE_USE_FOUNDRY` blank | [#372](https://github.com/leepokai/Codync/issues/372) | `/usage` reports the subscription's limits whatever the model routing |

## Subagents

Not a defect: what Codync reads to show which subagent did each step and what it cost (claude-agent-acp 0.109.1,
checked 2026-11-22). Codync doesn't declare the `subagents` client capability, so the adapter runs a subagent as the
parent's tool call titled `Task` (`_meta.claudeCode.toolName` `Agent`, kind `think`), on the parent's session:

- Every tool call the subagent makes (its `SubagentHandback` too) carries `_meta.claudeCode.parentToolUseId`, the
  `Agent` call's `toolCallId`. The subagent's own text never arrives.
- The `Agent` call's updates stream `rawInput` (`subagent_type`, `run_in_background`). When a foreground subagent
  ends, one update's `_meta.claudeCode.toolResponse` holds `agentId`, `agentType`, `resolvedModel`, `totalTokens`,
  `totalDurationMs` and `totalToolUseCount`. A background one's `toolResponse` is `{isAsync: true, status:
  "async_launched", agentId, …}`, with no type and no totals ever; its later calls still name the `Agent` call.
- `host/crates/agent/src/events/claude.rs` reads these into `ToolCall.parent` and `ToolCall.subagent`; the host keeps
  them on the tool entries (`agent/bot/tool_entry.rs`, `merge_subagent`).

Declaring `clientCapabilities.subagents` switches the adapter to ACP child sessions (`subagent_update`, the child's
calls under its own `sessionId`, no `Agent` call on the parent) and still reports no usage per child. Codync leaves it
off: the host holds one session per connection (permissions, replay and session-wide updates assume it), and it would
gain nothing. OpenCode 1.19.5 (one `task` tool call) and codex-acp 2.9.0 (`Start subagent` / `Complete subagent`
calls with none of the child's activity) send no such link, so their steps stay flat.
