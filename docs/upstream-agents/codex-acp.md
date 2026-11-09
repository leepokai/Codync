# Codex (codex-acp)

Upstream: [agentclientprotocol/codex-acp](https://github.com/agentclientprotocol/codex-acp). Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Silently leaves out a requested MCP server whose name Codex's own config uses, even a disabled one, and says so only in a notice before the first turn | `host/src/agent/bot/codex_names.rs`: reads the Codex config files and renames the clashing server `codync-<name>`; `host/src/agent/bot/tool_entry.rs` tracks the new names | Codync #217 | Requested servers override the config, or codex-acp returns an error |
| `/compact` ignores its instructions | `host/src/agent/bot/session.rs`: `CODEX_CONFIG={"compact_prompt": KEEP}` | none | `/compact <text>` uses the text |
| Sets no tool timeout for Codync's servers, so Codex's 300 s default drops a call without telling the server | `host/src/chat/manage.rs`: `APPROVAL_TIMEOUT` is 270 s | none | A per-server timeout, or Codex sends `notifications/cancelled` |
| Sends 429s and overloads as reply text, not as errors | **Not patched**: only `usageLimitExceeded` moves a Codex bot to its fallback ([agent fallbacks](../features/agent-fallback.md)) | [#647](https://github.com/agentclientprotocol/codex-acp/issues/647) | When fixed, map the new categories in `host/src/agent/fallback/reason.rs` |
| Keeps a closed session's MCP servers until the agent stops | Nothing beyond the idle stop in `host/src/agent/bot/lifecycle.rs` ([conversation runtime](../architecture/conversation-runtime.md)) | none | Upstream fix: nothing to delete |
| Never closes the thread it titles each new session in, so that thread's MCP servers stay up until the agent stops | Nothing beyond the idle stop in `host/src/agent/bot/lifecycle.rs` ([conversation runtime](../architecture/conversation-runtime.md)) | [#628](https://github.com/agentclientprotocol/codex-acp/issues/628) | Upstream fix: nothing to delete |
| The Codex CLI deletes the current credentials as soon as a new sign-in starts | `host/src/agent/term.rs`: no Login step while signed in | none | The CLI keeps them until the new login succeeds |
