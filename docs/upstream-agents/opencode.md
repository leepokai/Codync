# OpenCode

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| 1.x retries a model call its provider refused forever (a used-up free quota, a bad key), so `session/prompt` never answers: the chat stays on "Thinking…", and a background job (naming, call titles, dreaming) runs into its timeout; 2.x answers with the error | `host/src/agent/bot/launch.rs` (`start_first`, which chats and background jobs start their agent through): an installed OpenCode below 2 (its `agentInfo.version`) runs only when the registry build after it doesn't start. That build gets a database of its own (`OPENCODE_DB=~/.codync/opencode/opencode.db`): on the user's, its first start would migrate their 1.x sessions one way. Its sign-in (`auth.json`) stays shared. Left as is: model discovery and the sign-in check (`host/src/agent/auth/agent.rs`) ask the installed 1.x, and the OpenCode Go usage estimate (`host/src/usage/opencode.rs`) reads only the user's database | [anomalyco/opencode#41848](https://github.com/anomalyco/opencode/issues/41848) | Fixed in 2.0, keep until no supported install runs 1.x |

## Fork

Checked 2026-11-15 for background tasks ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). OpenCode 1.19.5 (the installed `opencode acp`, model `opencode/big-pickle`; the registry's 2.3.0 was not on the computer and not tried) advertises `sessionCapabilities.fork: {}`, which the host reads (`host/src/agent/bot/launch.rs`). `session/fork` from a second process, right after the first one's turn and while it still held the session, returned a new session that knew the chat; the original never saw what the fork was told after. The fork replays the chat's history as `session/update` before it answers (the host drops it, `drop_replay`). Cache: not shown. The fork's first turn read 1,792 of 82,200 input tokens from the cache, but so did the original's own second turn (1,792 of 82,381), while the fork's second read 82,176: on this free model the cache isn't hit reliably with or without a fork.
