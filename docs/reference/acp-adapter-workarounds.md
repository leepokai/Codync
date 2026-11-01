# ACP adapter workarounds

Codync drives every agent through its ACP adapter (`host/src/agent/acp.rs`, `host/src/agent/bot/`). Some adapters ignore part
of the protocol or behave in their own way, and the host patches around them. This page lists those patches, so that
each one can be deleted once its adapter is fixed. When enough of an adapter's rows are gone, we can reconsider how
Codync uses that adapter, or replace it with another implementation.

Last checked: **2026-11-02**. Versions in the ACP registry then: claude-acp 0.99.1, codex-acp 2.5.0, pi-acp 0.0.34,
cursor 2026.10.22, gemini 0.66.0, github-copilot-cli 1.0.105, grok-build 1.0.73, kilo 7.8.18, opencode 2.2.5,
qwen-code 0.26.5.

## Rules

- **A new patch gets a row in the same change.** Its code comment names the defect and the upstream issue, if
  there is one. If upstream has no issue, open one, or note in the row why we didn't.
- **Remove a patch with its row.** Do it once the fixed adapter version is the one Codync starts: the registry
  build (`~/.codync/registry.json` pins the version), or the oldest CLI we still support for harnesses that run
  locally. If a fix exists but users can still run an older version, keep the patch and write "fixed in X, keep
  until Y" in the row.
- **Check upstream when an adapter's registry version moves**, and whenever a cited issue or PR closes:
  `gh issue view <n> -R <owner/repo>`, `gh pr view <n> -R <owner/repo>`. Then retest the defect with the new
  version before deleting anything.
- Workarounds that only parse adapter-specific fields (error categories, auth `_meta`) are cheap. They are listed
  so they don't surprise anyone, but there is no hurry to remove them.

## pi-acp ([svkozak/pi-acp](https://github.com/svkozak/pi-acp))

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Accepts `session/new` `mcpServers` and never hands them to pi, so a pi bot had none of Codync's tools (`create_bot`, `send_message`, memory, routines) | `host/src/agent/bot/pi_tools.rs`: `PI_ACP_PI_COMMAND` points to Codync's `pi-codync` launcher. The launcher starts pi with `codync.js`, which registers the servers from `CODYNC_PI_MCP_SERVERS` (`pi.registerMcpServer`, pi 0.99+). Set in `host/src/agent/bot/session.rs` for `backend == "pi"`. The extension does nothing on an older pi, or when pi-acp's own bridge env (`PI_ACP_MCP_SERVERS`, PR #164) is set, because pi exits when an extension fails to load | Open PRs [#164](https://github.com/svkozak/pi-acp/pull/164), [#145](https://github.com/svkozak/pi-acp/pull/145); README "Limitations" | pi-acp forwards `mcpServers` itself. Delete `host/src/agent/bot/pi_tools.rs` and its calls in `host/src/agent/bot/session.rs` |
| pi skips a registered server whose name is already in its own `mcp.json` (disabled entries too) | `host/src/agent/bot/pi_tools.rs` reads pi's and the project's `mcp.json` and renames ours to `codync-<name>` (`-` and `_` count as one), through `host/src/agent/bot/codex_names.rs` | none | Codync's servers win over pi's config, or the row above is gone and pi-acp handles name clashes |
| pi gives up on a tool call after 60 s, before a team change's approval card can be answered | `host/src/agent/bot/pi_tools.rs` sets each server's `timeout` past `chat::manage::APPROVAL_TIMEOUT` | none | With the first row |
| Sends banners and command lists on a timer, after it has already answered `session/new` | `host/src/agent/bot/turn.rs`: a fresh session waits 400 ms, then drops what came in (for every adapter) | none | pi-acp sends them before its answer, or not at all |

## codex-acp ([agentclientprotocol/codex-acp](https://github.com/agentclientprotocol/codex-acp))

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Silently leaves out a requested MCP server whose name Codex's own config uses, even a disabled one, and says so only in a notice before the first turn | `host/src/agent/bot/codex_names.rs`: reads the Codex config files and renames the clashing server `codync-<name>`; `host/src/agent/bot/tool_entry.rs` tracks the new names | Codync #217 | Requested servers override the config, or codex-acp returns an error |
| `/compact` ignores its instructions | `host/src/agent/bot/session.rs`: `CODEX_CONFIG={"compact_prompt": KEEP}` | none | `/compact <text>` uses the text |
| Sets no tool timeout for Codync's servers, so Codex's 300 s default drops a call without telling the server | `host/src/chat/manage.rs`: `APPROVAL_TIMEOUT` is 270 s | none | A per-server timeout, or Codex sends `notifications/cancelled` |
| Sends 429s and overloads as reply text, not as errors | **Not patched**: only `usageLimitExceeded` moves a Codex bot to its fallback ([agent fallbacks](../features/agent-fallback.md)) | [#647](https://github.com/agentclientprotocol/codex-acp/issues/647) | When fixed, map the new categories in `host/src/agent/fallback/reason.rs` |
| Never closes the thread it titles each new session in, so that thread's MCP servers stay up until the agent stops | Nothing beyond the idle stop in `host/src/agent/bot/lifecycle.rs` ([conversation runtime](../architecture/conversation-runtime.md)) | [#628](https://github.com/agentclientprotocol/codex-acp/issues/628) | Upstream fix: nothing to delete |
| The Codex CLI deletes the current credentials as soon as a new sign-in starts | `host/src/agent/term.rs`: no Login step while signed in | none | The CLI keeps them until the new login succeeds |

## claude-agent-acp ([agentclientprotocol/claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp))

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

## Cursor (`cursor-agent acp`)

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| `session/load` comes back on the last model used in any session | `host/src/agent/bot/session.rs`: `select_model` again after loading | none | Each session keeps its own model |
| Lists a model's options (effort, fast…) only through `cursor/list_available_models` | `host/src/agent/auth/models.rs` | none | Standard `configOptions` per model |
| Model variants as fixed ids (`grok-4.7[reasoning_effort=high,fast=true]`) | `host/src/agent/bot/launch.rs`, `host/src/agent/auth/agent.rs`: `_meta.parameterizedModelPicker`, saved variant ids split into options | none | Plain models are the default and no saved variant ids are left |

## CodeBuddy and WorkBuddy

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| A failed prompt is `stopReason: "refusal"` with the error in `_meta["codebuddy.ai/errorMessage"]` | `host/src/agent/acp/codebuddy.rs`: turned back into an error (`host/src/agent/bot/turn.rs`, `host/src/chat/oneshot.rs`) | none | A JSON-RPC error |
| WorkBuddy's `session/new` succeeds with no token | `host/src/agent/backends/workbuddy.rs`, `host/src/agent/auth.rs`: asks `_codebuddy.ai/getUserInfo` | none | `session/new` fails when signed out |
| WorkBuddy's sign-in logs out a saved session, and that logout cancels the login it just started | `host/src/agent/backends/workbuddy.rs`: moves `{id}.info` aside first | none | Signing in over a saved session works |
| The desktop app ships only the lite bundle; `--acp` alone exits at once | `host/src/agent/backends/workbuddy.rs`: `CODEBUDDY_FORCE_LITE_WB_BUNDLE=1`, `--serve --acp` | none | The app ships an ACP bundle |

## Others

| Adapter | Defect | Codync's patch | Drop when |
|---|---|---|---|
| Hermes | Starts MCP servers with a filtered environment | `host/src/mcp.rs`: `CODYNC_HOME` in each server's `env` | Keep: within the spec |
| Kilo | `_meta.terminal-auth` names `opencode` | `host/src/agent/auth.rs`: that method is left out | Kilo names its own CLI |
| Qwen Code | Terminal sign-in in `_meta: {type: "terminal", args}` instead of the spec's fields | `host/src/agent/auth.rs` reads it | The spec's fields |
| oh-my-pi | Tool call titles are prose ("Creating probe bot"), not the tool's name, so Codync's own calls aren't recognized as its bookkeeping | **Not patched** (`host/src/agent/bot/tool_entry.rs` matches names only) | Titles or `_meta` carry the tool's name |

## Gaps in ACP itself

Not one adapter's bug: the protocol leaves these open, and the host covers them for every agent.

- No structured MCP server and tool name on a tool call: `host/src/agent/bot/tool_entry.rs` parses titles (`mcp__x__y`, `mcp.x.y`).
- Every adapter names its error category differently (`errorKind`, `category`, `codexErrorInfo`): `host/src/agent/acp.rs`.
- Agents that ignore `session/cancel` are killed after 15 s; a hung `session/new` or `session/load` after 3 min;
  an old CLI that hangs on `initialize` gets 20 s when another way to start it is left (`host/src/agent/bot/mod.rs`,
  `host/src/agent/bot/session.rs`).
- Agents that send no `compaction_update`: a drop in context use by more than half stands in for one
  (`host/src/agent/bot/compaction.rs`).
- Mode ids have no fixed meaning: Auto is found by id or name, and Goose's `auto` (approves everything) is hidden
  (`host/src/agent/modes.rs`).
