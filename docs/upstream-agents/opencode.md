# OpenCode

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| 1.x retries a model call its provider refused forever (a used-up free quota, a bad key), so `session/prompt` never answers: the chat stays on "Thinking…", and a background job (naming, call titles, dreaming) runs into its timeout; 2.x answers with the error | `host/src/agent/bot/launch.rs` (`start_first`, which chats and background jobs start their agent through): an installed OpenCode below 2 (its `agentInfo.version`) runs only when the registry build after it doesn't start. That build gets a database of its own (`OPENCODE_DB=~/.codync/opencode/opencode.db`): on the user's, its first start would migrate their 1.x sessions one way. Its sign-in (`auth.json`) stays shared. Left as is: model discovery and the sign-in check (`host/src/agent/auth/agent.rs`) ask the installed 1.x, and the OpenCode Go usage estimate (`host/src/usage/opencode.rs`) reads only the user's database | [anomalyco/opencode#41848](https://github.com/anomalyco/opencode/issues/41848) | Fixed in 2.0, keep until no supported install runs 1.x |
