# DeepSeek Harness

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| 0.1.0 releases (seen on 0.1.0-rc.6) have no `acp` profile: `dsh --profile acp` prints `profile acp does not exist` and exits during `initialize`, so the bot fails with only "agent process exited" | `host/src/agent/backends/catalog.rs` (`OUTDATED`) + `backends::exit_reason`: the start error says the CLI is too old and gives the update command through Codync's own npm | none: later releases have the `acp` profile, nothing to report | Keep while users can still run a 0.1.0 `dsh` (the update hint is cheap) |

## Fork

Checked 2026-11-15 for threads ([spikes](../archive/background-tasks-plan-2026-11-15.md#spikes)). `initialize` advertises no `sessionCapabilities.fork` (deepseek-harness-acp 0.0.1 in dsh 0.2.1-rc.5, `dsh --profile acp`; it has `close`, `list`, `resume`), and `session/fork` answers `-32601 "Method not found": session/fork`, so a thread starts from a brief. Not a defect: the capability is optional.
