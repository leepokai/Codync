# Grok Build

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Not every build answers ACP's extension name `_x.ai/billing`, and the 1.0.75 binary names `x.ai/billing` | `host/src/usage/grok.rs`: asks `x.ai/billing`, then `_x.ai/billing` | No issue opened: seen only in the binary's strings, with no login to run it | The oldest build we support answers one name |

## Fork

Checked 2026-11-15 for threads ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). `initialize` advertises no `sessionCapabilities.fork` (grok 1.0.77, `grok agent stdio`; it has `list`, `resume`, `close`). Nothing further was run: not signed in here (`session/new`: `Authentication required`), so a thread starts from a brief. Not a defect: the capability is optional.
