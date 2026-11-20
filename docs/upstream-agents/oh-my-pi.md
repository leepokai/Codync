# oh-my-pi

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Tool call titles are prose ("Creating probe bot"), not the tool's name, so Codync's own calls aren't recognized as its bookkeeping | **Not patched** (`host/src/agent/bot/tool_entry.rs` matches names only) | none | Titles or `_meta` carry the tool's name |

## Fork

Checked 2026-11-15 for threads ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). omp 18.14.0 (`omp acp`, model `deepseek/deepseek-v4-pro`) advertises `sessionCapabilities.fork: {}`, which the host reads (`host/src/agent/bot/launch.rs`). `session/fork` from a second process, right after the first one's turn and while it still held the session, returned a new session that knew the chat; the original never saw what the fork was told after. No history is replayed. The fork's first turn read 12,416 of 12,482 input tokens from the cache.
