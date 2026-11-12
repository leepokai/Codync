# DeepSeek Harness

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| 0.1.0 releases (seen on 0.1.0-rc.6) have no `acp` profile: `dsh --profile acp` prints `profile acp does not exist` and exits during `initialize`, so the bot fails with only "agent process exited" | `host/src/agent/backends/catalog.rs` (`OUTDATED`) + `backends::exit_reason`: the start error says the CLI is too old and gives the update command through Codync's own npm | none: later releases have the `acp` profile, nothing to report | Keep while users can still run a 0.1.0 `dsh` (the update hint is cheap) |
