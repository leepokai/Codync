# Doubao

Rules: [README](README.md).

Doubao CLI (`@bytedance/doubao-cli`) speaks ACP v1 on stdio from 2.38: `doubao acp serve` (`doubao --acp` is the same server). Checked against 2.38.2 on 2026-11-26 (`doubao acp --help`). It is not in the ACP registry, so Codync runs the installed CLI only.

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Releases before 2.38 have no `acp` subcommand: `doubao acp` prints `未知命令或选项` and exits during start, so the bot fails with only "agent process exited" | `host/crates/agent/src/backends/catalog.rs` (`OUTDATED`) + `backends::exit_reason`: the start error says the CLI is too old and gives the update command through Codync's own npm | Fixed in 2.38 (the help lists `acp serve`). Nothing to report for the current CLI | Keep while users can still run a Doubao CLI older than 2.38 |
