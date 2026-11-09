# OpenCode

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| 1.x retries a model call its provider refused forever (a used-up free quota, a bad key), so `session/prompt` never answers and the chat stays on "Thinking…"; 2.x answers with the error | `host/src/agent/bot/launch.rs` (`superseded`), `host/src/agent/bot/session.rs`: an installed OpenCode below 2 (its `agentInfo.version`) is stopped and the registry build starts instead | [anomalyco/opencode#41848](https://github.com/anomalyco/opencode/issues/41848) | Fixed in 2.0, keep until no supported install runs 1.x |
