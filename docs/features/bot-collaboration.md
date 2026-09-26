# Bot collaboration

Create two or more bots normally, giving each a clear name, description, project folder and agent. Then message one:

> Ask Reviewer to inspect the changes in /path/to/project. Have it report bugs without editing files, then summarize its findings for me.

Every bot receives the built-in `team` MCP server. `list_bots` returns the other visible bots' IDs, descriptions, agents, folders and status. `ask_bot(botId, message)` sends a self-contained request and waits for the recipient's final text reply. The requesting bot incorporates that answer into its response to you. The same tools work across ACP backends that support MCP servers.

The host records the request and its outcome in both chats using existing notice entries, so the current iOS, macOS, Linux and terminal clients can display them. Tool details also remain in the trace. There is no additional team setup screen.

## Execution

- Each bot still owns one ACP process/session and runs one turn at a time. A busy recipient queues the request. Different bots can work concurrently.
- Requests have their own reply channel and never merge with user messages or another request. Contiguous user messages keep the existing batching behavior.
- The recipient uses its own folder, tools, memory and permission policy. Permission cards appear in its chat. The requesting bot's entire transcript is not copied; only the request is passed. Requests are not treated as user facts by the memory keeper.
- The host rejects self-delegation, duplicate outstanding requests to the same recipient, and direct or indirect wait cycles, including queued requests. Up to 64 requests can be outstanding host-wide.
- Native Claude Code/Codex subagents remain managed by the harness. Codync does not turn them into permanent bots or override their delegation settings.

## Stops, failures and restart

- The wait limit is ten minutes, including queue and approval time. Failure is a tool error, not a fabricated successful reply. `ask_bot` is not automatically retried.
- Stopping the requesting bot cancels its outgoing requests. A queued request is removed; a running delegated turn's ACP process is stopped. Unrelated queued messages on the recipient remain. Stopping the recipient also releases its requesters once its turn stops.
- Deleting either bot cancels affected requests. Recipient startup failures, crashes, refusals and empty replies surface to the requester.
- Pending request notices are persisted with IDs and statuses. After host restart they become interrupted errors; delegated turns are not automatically replayed. Check partial work before retrying. Ordinary user turns retain their existing session-resume behavior.
- Completion push notifications come from the requesting bot; delegated replies do not send a second “done” push. Recipient permission requests still use the usual “needs you” notifications.

This first version uses synchronous request/reply over MCP and does not introduce a task scheduler or isolated worktrees. Give bots explicit file ownership when they share a working directory. Rebuild and restart the host to load the new built-in MCP server; phone clients need no protocol upgrade.

## Code and verification

- `host/src/team.rs`: discovery, wait graph, request lifetime and persisted notices.
- `host/src/bot.rs`: queue boundaries, recipient execution, completion and targeted cancellation.
- `host/src/mcp.rs`: the authenticated local MCP → HTTP bridge.
- `host/tests/team_e2e.rs`: a real host and MCP subprocess, with scripted ACP agents, covering discovery → delegation → recipient approval → reply → client sync. No paid provider calls.

Run `cargo test` and `cargo clippy --all-targets -- -D warnings` in `host/`.
