# Antigravity

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Signed out, it starts its browser sign-in as soon as a session opens, so a usage reading would open a browser at every poll | `host/src/usage/acp_read.rs`: started with `BROWSER=true`, which its Python honors. Windows has no such command, so `host/src/usage/antigravity.rs` needs a saved login file first | No issue opened yet: this only surfaced reading usage | `session/new` doesn't start a sign-in |
| Each new session leaves a conversation database behind, about 400 KB | `host/src/usage/acp_read.rs`: readings share one session, resumed each time (`usage.session.<id>` in the kv) | No issue opened yet: keeping sessions may be intended | A session with no turn leaves nothing behind |
