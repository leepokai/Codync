# Hermes

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Starts MCP servers with a filtered environment | `host/src/mcp.rs`: `CODYNC_HOME` in each server's `env` | none: within the spec | Keep |

## Fork

Checked 2026-11-15 for background tasks ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). Hermes 0.22.5 (`hermes acp`, model `deepseek:deepseek-chat`) advertises `sessionCapabilities.fork: {}`, which the host reads (`host/src/agent/bot/launch.rs`). `session/fork` from a second process, right after the first one's turn and while it still held the session, returned a new session that knew the chat; the original never saw what the fork was told after. No history is replayed. The fork's first turn read 18,048 of 18,231 input tokens from the cache.
