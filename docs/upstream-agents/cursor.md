# Cursor

Runs as `cursor-agent acp`. Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| `session/load` comes back on the last model used in any session | `host/src/agent/bot/session.rs`: `select_model` again after loading | none | Each session keeps its own model |
| Lists a model's options (effort, fast…) only through `cursor/list_available_models` | `host/src/agent/auth/models.rs` | none | Standard `configOptions` per model |
| No `session/close`, so a finished routine run's session keeps its MCP servers while the bot's own chat stays open | `host/src/agent/bot/lifecycle.rs`: `release_spent` keeps the run's session as spent, and `stop_spent` (or `release_idle` after background work) stops the agent once nothing runs or waits; the chat reopens its session with `session/load` | none | Cursor offers `session/close` (`sessionCapabilities.close`): the run's session then closes like any other agent's |
| Model variants as fixed ids (`grok-4.7[reasoning_effort=high,fast=true]`) | `host/src/agent/bot/launch.rs`, `host/src/agent/auth/agent.rs`: `_meta.parameterizedModelPicker`, saved variant ids split into options | none | Plain models are the default and no saved variant ids are left |

## Fork

Checked 2026-11-15 for threads ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). `initialize` advertises no `sessionCapabilities.fork` (cursor-agent 2026.10.22, `cursor-agent acp`; it has `list` only), and `session/fork` answers `-32601 "Method not found": session/fork`, so a thread starts from a brief. Not a defect: the capability is optional.
