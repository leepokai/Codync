# Hermes

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Starts MCP servers with a filtered environment | `host/src/mcp.rs`: `CODYNC_HOME` in each server's `env` | none: within the spec | Keep |
| A turn starts the profile's shared gateway when none runs (`hermes_cli.main --profile default gateway run --quiet --idle-exit`, its MCP servers under it), detached into its own process group, so it outlives the host (seen 2026-11-23, Hermes runtime 20261021) | none: Hermes's other clients use the same gateway, so the host leaves it alone; it exits by itself after `gateway.unmanaged_idle_exit_seconds` (default 600 s) of serving no one | none: by design (`gateway/run_idle_exit.py`) | Keep |
| A new `HERMES_HOME` builds its dependency environment on first launch, and a second `hermes` started meanwhile (the host's sign-in check beside the session) fails with "no dependency environment is committed" (seen by the [upstream check](../guides/test-stack.md#upstream-agents) on 2026-11-24) | none in the host: a first Hermes bot on a fresh home (new install or login profile) can fail once. The check builds it first with `hermes acp --check` (`tools/agent-upstream/agents.mjs`) | Not reported yet | A second `hermes` waits for the environment, or the host's sign-in check moves off a second process |

## Fork

Checked 2026-11-15 for threads ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). Hermes 0.22.5 (`hermes acp`, model `deepseek:deepseek-chat`) advertises `sessionCapabilities.fork: {}`, which the host reads (`host/crates/agent/src/acp/capabilities.rs`). `session/fork` from a second process, right after the first one's turn and while it still held the session, returned a new session that knew the chat; the original never saw what the fork was told after. No history is replayed. The fork's first turn read 18,048 of 18,231 input tokens from the cache.
