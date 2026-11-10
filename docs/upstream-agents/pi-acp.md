# pi (pi-acp)

Upstream: [svkozak/pi-acp](https://github.com/svkozak/pi-acp). Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Accepts `session/new` `mcpServers` and never hands them to pi, so a pi bot had none of Codync's tools (`create_bot`, `send_message`, memory, routines) | `host/src/agent/bot/pi_tools.rs`: `PI_ACP_PI_COMMAND` points to Codync's `pi-codync` launcher. The launcher starts pi with `codync.js`, which registers the servers from `CODYNC_PI_MCP_SERVERS` (`pi.registerMcpServer`, pi 0.99+). Set in `host/src/agent/bot/session.rs` for `backend == "pi"`. The extension does nothing on an older pi, or when pi-acp's own bridge env (`PI_ACP_MCP_SERVERS`, PR #164) is set, because pi exits when an extension fails to load | Open PRs [#164](https://github.com/svkozak/pi-acp/pull/164), [#145](https://github.com/svkozak/pi-acp/pull/145); README "Limitations" | pi-acp forwards `mcpServers` itself. Delete `host/src/agent/bot/pi_tools.rs` and its calls in `host/src/agent/bot/session.rs` |
| pi skips a registered server whose name is already in its own `mcp.json` (disabled entries too) | `host/src/agent/bot/pi_tools.rs` reads pi's and the project's `mcp.json` and renames ours to `codync-<name>` (`-` and `_` count as one), through `host/src/agent/bot/codex_names.rs` | none | Codync's servers win over pi's config, or the row above is gone and pi-acp handles name clashes |
| pi gives up on a tool call after 60 s, before a team change's approval card can be answered | `host/src/agent/bot/pi_tools.rs` sets each server's `timeout` past `chat::manage::APPROVAL_TIMEOUT` | none | With the first row |
| Sends banners and command lists on a timer, after it has already answered `session/new` | `host/src/agent/bot/turn.rs`: a fresh session waits 400 ms, then drops what came in (for every adapter) | none | pi-acp sends them before its answer, or not at all |
