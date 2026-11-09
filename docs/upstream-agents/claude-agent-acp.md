# Claude Code (claude-agent-acp)

Upstream: [agentclientprotocol/claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp). Rules:
[README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| The standard `systemPrompt` field drops the `_meta.claudeCode.options`; ACP can't change a session's system prompt | `host/src/agent/bot/mod.rs`, `host/src/agent/bot/session.rs`: always `_meta.systemPrompt`; after a compaction, `session/load` again to swap the prompt in | none | The adapter merges both, and ACP can update a system prompt |
| After an internal error with no category, the session stays dead ("The Claude Agent session has ended") | `host/src/agent/acp.rs`, `host/src/agent/bot/lifecycle.rs`: `session/close` so the next turn loads it again (any adapter's uncategorized -32603) | none | The adapter restarts its query stream |
| Opens a session without checking the login | `host/src/agent/auth.rs`: for agents with `_meta.authStatus`, only `_auth/status_update` counts | none | A session fails when signed out |
| Gives its own `claude auth status` only 5 s, too few on many Windows computers | `host/src/agent/auth.rs`, `host/src/agent/backends/sign_in.rs`: waits 8 s, then runs the check itself for 15 s | [investigation](../investigations/windows-claude-sign-in-2026-10-20.md) | A longer budget, or it always reports |
| `claude auth status` still says signed in when the token was revoked | `host/src/agent/backends/sign_in.rs`, `host/src/agent/bot/queue.rs`: a prompt refused for auth wins until a sign-in finishes | none | The status check notices |
| The process keeps the credentials it started with | `host/src/agent/bot/queue.rs`: restart after a turn that failed signing in | none | The adapter reads them again |
| Killed during an OAuth refresh, it loses the new refresh token and with it the login | `host/src/agent/acp.rs`: close stdin first, 10 s `EXIT_GRACE` | none | Keep: a graceful stop costs nothing |
| A usage limit arrives as `rate_limit` / `billing_error` text | `host/src/agent/fallback/reason.rs`: matches the Agent SDK's usage-limit prefixes | none | A distinct `errorKind` |
| Why Fast mode is refused is only in the option's description | `host/src/agent/config.rs`, `host/src/agent/bot/fast_mode.rs`: split on `" — "` | none | A structured reason |
| The compaction summary rides on one of several `completed` `compaction_update`s | `host/src/agent/bot/updates.rs`, `host/src/agent/bot/compaction.rs`: dedupe by `compactionId`, attach a late summary | none | One update per compaction |
