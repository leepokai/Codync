# Cursor

Runs as `cursor-agent acp`. Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| `session/load` comes back on the last model used in any session | `host/src/agent/bot/session.rs`: `select_model` again after loading | none | Each session keeps its own model |
| Lists a model's options (effort, fast…) only through `cursor/list_available_models` | `host/src/agent/auth/models.rs` | none | Standard `configOptions` per model |
| Model variants as fixed ids (`grok-4.7[reasoning_effort=high,fast=true]`) | `host/src/agent/bot/launch.rs`, `host/src/agent/auth/agent.rs`: `_meta.parameterizedModelPicker`, saved variant ids split into options | none | Plain models are the default and no saved variant ids are left |
