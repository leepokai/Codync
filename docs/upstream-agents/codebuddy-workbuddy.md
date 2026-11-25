# CodeBuddy and WorkBuddy

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| A failed prompt is `stopReason: "refusal"` with the error in `_meta["codebuddy.ai/errorMessage"]` | `host/crates/agent/src/acp/codebuddy.rs`: turned back into an error (`host/src/agent/bot/turn.rs`, `host/src/chat/oneshot.rs`) | none | A JSON-RPC error |
| WorkBuddy's `session/new` succeeds with no token | `host/crates/agent/src/backends/workbuddy.rs`, `host/src/agent/auth.rs`: asks `_codebuddy.ai/getUserInfo` | none | `session/new` fails when signed out |
| WorkBuddy's sign-in logs out a saved session, and that logout cancels the login it just started | `host/crates/agent/src/backends/workbuddy.rs`: moves `{id}.info` aside first | none | Signing in over a saved session works |
| WorkBuddy sends `tool_call` twice for one `toolCallId`: first as its arguments start streaming (`status: in_progress`, title `Bash`, empty `rawInput`), then again once they're complete; the finished update carries the command's output only in `rawOutput` (seen by the [upstream check](../guides/test-stack.md#upstream-agents) on 2026-11-24) | none: `host/src/agent/bot/updates.rs` adds an entry for each `tool_call`, so the chat keeps a Bash card stuck at in progress beside the finished one, and the finished card shows no output. WorkBuddy isn't in the daily check: it runs only from the macOS and Windows desktop app | Not reported yet | WorkBuddy sends one `tool_call` per call, or the host updates the entry a repeated `tool_call` names |
| The desktop app ships only the lite bundle; `--acp` alone exits at once | `host/crates/agent/src/backends/workbuddy.rs`: `CODEBUDDY_FORCE_LITE_WB_BUNDLE=1`, `--serve --acp` | none | The app ships an ACP bundle |
