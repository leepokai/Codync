# CodeBuddy and WorkBuddy

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| A failed prompt is `stopReason: "refusal"` with the error in `_meta["codebuddy.ai/errorMessage"]` | `host/src/agent/acp/codebuddy.rs`: turned back into an error (`host/src/agent/bot/turn.rs`, `host/src/chat/oneshot.rs`) | none | A JSON-RPC error |
| WorkBuddy's `session/new` succeeds with no token | `host/src/agent/backends/workbuddy.rs`, `host/src/agent/auth.rs`: asks `_codebuddy.ai/getUserInfo` | none | `session/new` fails when signed out |
| WorkBuddy's sign-in logs out a saved session, and that logout cancels the login it just started | `host/src/agent/backends/workbuddy.rs`: moves `{id}.info` aside first | none | Signing in over a saved session works |
| The desktop app ships only the lite bundle; `--acp` alone exits at once | `host/src/agent/backends/workbuddy.rs`: `CODEBUDDY_FORCE_LITE_WB_BUNDLE=1`, `--serve --acp` | none | The app ships an ACP bundle |
