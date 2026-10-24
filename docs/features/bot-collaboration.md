# Bot collaboration

Create two or more bots normally, giving each a clear name, description, project folder and agent. Then message one:

> Ask Reviewer to inspect the changes in /path/to/project. Have it report bugs without editing files, then summarize its findings for me.

Every bot receives the built-in `team` MCP server. `list_bots` returns the other visible bots' IDs, descriptions, agents, folders and status. `ask_bot(botId, message)` sends a self-contained request and waits for the recipient's final text reply. The requesting bot incorporates that answer into its response to you. `message_bot(botId, message)` queues a request and returns immediately; the recipient reports to you in its own chat. The same tools work across ACP backends that support MCP servers.

The host records each exchange as one notice in both chats. Every client shows it as a compact row (centered in the apps, one ` · ` line in the terminal UI), "Messaged [avatar] Owen" in the sender's chat and "Message from [avatar] Egan" in the recipient's, with no request text. In a chat a run of consecutive exchanges with the same peer, in either direction, is one row, "3 messages with [avatar] Owen"; anything else the chat shows (a message, another notice, a card, an exchange with another peer) ends the run, trace entries do not, and the run is marked failed if any exchange failed. Only a failure is marked (danger color and a warning mark). Tapping the row opens a read-only "Egan ⇄ Owen" conversation sheet with the pair's whole history: author-labelled grey bubbles for each request and reply, time separators, "Waiting for Owen…" or the failure detail under a pending or failed exchange. It is laid out like the Full conversation sheet, with the pair as its title and an X to close. Tool details also remain in the trace. There is no additional team setup screen; a lead bot can set up the team itself (see Team management).

## Notice data

New bot-to-bot notices retain all fields written by the team tools in the base:
`text`, `heading`, `style`, `status`, `delegationId`, `sourceBotId` and `targetBotId`.
They add one optional structured field:

```json
{
  "botMessage": {
    "sourceBotId": "egan",
    "targetBotId": "owen",
    "text": "Review this change",
    "reply": "Looks good",
    "detail": "Failure or cancellation detail, when applicable"
  }
}
```

`text` is the original request. Completed asks add `reply` (at most 4,000 bytes);
failures and cancellations add `detail`. The existing notice `status` determines
whether the exchange is pending, completed, failed or cancelled. The base notice
heading and rendered text remain unchanged for clients that display plain notices.
Clients read the structured data directly; they do not parse display strings.

Existing notices remain ordinary notices. There is no startup rewrite, data
migration or transcript-cache reset. The existing restart cleanup still marks
unfinished requests failed and adds the failure detail to their structured data,
when present. It never replays work.

`botConversation {botId, peerId}` returns `{entries}`: the newest 200 main-chat
notices containing structured messages with that peer, oldest first. It requires
the Control scope. Clients merge these with the live chat (the higher `rev` wins)
and keep the result in the sheet; it is never added to the chat mirror, so main
history paging is unaffected.

## Ask execution

- Each bot still owns an ACP process (its main session, plus one per routine run) and runs one turn at a time. A busy recipient queues the request. Different bots can work concurrently.
- Requests have their own reply channel and never merge with user messages or another request. Contiguous user messages keep the existing batching behavior.
- An ask's final reply is addressed to the requesting bot. It appears in the request notice
  and the recipient's trace, without a second bubble in the recipient's main chat. The recipient
  returns the answer directly rather than sending another `message_bot` request back.
- The recipient uses its own folder, tools, memory and permission policy. Permission cards appear in its chat. The requesting bot's entire transcript is not copied; only the request is passed. Requests are not treated as user facts by the memory keeper.
- The host rejects self-delegation, duplicate outstanding requests to the same recipient, and direct or indirect wait cycles, including queued requests. Up to 64 requests can be outstanding host-wide.
- Native Claude Code/Codex subagents remain managed by the harness. Codync does not turn them into permanent bots or override their delegation settings.

## Ask stops, failures and restart

- The wait limit is ten minutes, including queue and approval time. Failure is a tool error, not a fabricated successful reply. `ask_bot` is not automatically retried.
- Stopping the requesting bot cancels its outgoing requests. A queued request is removed; a running delegated turn's ACP process is stopped. Unrelated queued messages on the recipient remain. Stopping the recipient also releases its requesters once its turn stops.
- Deleting either bot cancels affected requests. Recipient startup failures, crashes, refusals and empty replies surface to the requester.
- Pending request notices are persisted with IDs and statuses. After host restart they become interrupted errors; delegated turns are not automatically replayed. Check partial work before retrying. Ordinary user turns retain their existing session-resume behavior.
- Completion push notifications come from the requesting bot; delegated replies do not send a second “done” push. Recipient permission requests still use the usual “needs you” notifications.

`ask_bot` uses synchronous request/reply over MCP; neither tool adds a task scheduler or isolated worktrees. Give bots explicit file ownership when they share a working directory. Rebuild and restart the host to load the new built-in MCP server.

## Independent messages

`message_bot(botId, message)` queues a bot request and returns immediately, without waiting for a reply. The recipient reports to you in its own chat. Messages use the same queue as asks, run one turn at a time, and never merge with user messages or other requests. Both chats show bot-attributed notices rather than user messages. Up to 64 messages can be outstanding host-wide, with at most 16 from any one sender, separately from asks. Queued and running messages both count. A slot is freed when the recipient finishes, fails or cancels the request, or admission fails. Stopping, restarting or deleting the sender does not free slots for its already accepted messages.

The recipient can use `send_message` for user-facing updates and its report. If it sends none,
its final text becomes the main-chat report. The request notice shows completion
status; it does not copy this user-facing report into a bot reply.

Each independent request includes reporting guidance, even when it reuses a
session that previously handled an ask. The ask's reply restrictions apply
only to that ask; later user turns and independent requests can report in
the recipient's chat normally.

Accepted messages survive sender completion, Stop, disconnection and deletion. Recipient Stop or deletion cancels queued messages and stops running work; started work may have left partial changes. Outcomes update both notices; failures mark them as failed. The recipient uses its normal completion notifications. Messages have no ask timeout or automatic retry and are not replayed after host restart.

Bot request chains can make at most eight handoffs. Both `message_bot` and
`ask_bot` carry the count into the recipient's turn, including queued work;
finishing an earlier request does not reset it. This bounds repeated A → B → A
messages even when few requests are outstanding. At the limit, the tool reports
an error without queuing more work. A new user turn starts a fresh allowance.

## Team management

A bot whose settings have **Manage the team** on (off by default; only the user turns it on, on any client; a client hides the switch for a host that sends no `manageTeam`) also gets the built-in `manage` MCP server, so a lead bot can hire and set up the specialists it needs:

- `list_team`: every agent bot, the manager itself (`you: true`) and hidden ones included, with its setup (instructions, agent, model and its settings, fallbacks, avatar, folder, approvals, whether it manages the team, who created it) and routines.
- `create_bot {name, instructions?, agent?, model?, avatarColor?, avatarShape?, folder?, routines?}`: a new bot with the agent a client's new bot gets. It manages the team too, and takes the manager's approval setting, connectors and skills, so it can't act more freely or reach more than the manager (a bot under Ask can't create bots that don't ask; connectors the user turned off for the manager stay off); it never gets computer use. Without an avatar it gets a random look, as a client's new bot does. `routines` are `save_routine` bodies. Bots stop making bots at 20 that bots made (deleted ones don't count), so managers that make managers can't multiply without end; past that the manager asks the user.
- `update_bot {botId, name?, instructions?, model?, fallbacks?, avatarColor?, avatarShape?, hidden?, routines?}`: any agent bot, the manager itself included; a routine with an `id` replaces that routine (one of that bot's). `fallbacks` (up to 3 `{agent, model?}`, [agent fallbacks](agent-fallback.md)) replaces the bot's list, also for a bot whose user turned "Let the bot change its fallbacks" off: that switch is about the bot's own `set_fallbacks`. A changed model drops the bot's model settings (effort…); setting those, turning Manage the team on or off, deleting bots and every other setting stay with the user. No model id sets how the agent approves tools (`opus[mode=acceptEdits]`): the host never sends a model option of the agent's `mode` category. A new agent or model that arrives during a turn (the manager changing its own) restarts the agent once that turn ends, not in the middle of it; this holds for a change from the settings too.

A manager doesn't run other bots' routines. To test a teammate's, it asks that bot with `message_bot` to run it with its own `run_routine` (by id), so the run is the owner's: its routine session, timeout, retries, run history and result in the owner's chat. The `manage` instructions say so, the `routines` instructions let a bot do it when another bot asks for one of its own routines, and `run_routine` with an id that isn't the caller's fails with that hint ([routines](routines.md#conversational-setup-and-tools)). A hidden bot can't be messaged, so for its routine the manager asks the user to run it from that bot.

Under Ask each call first posts an approval card in the manager's chat (Allow / Deny) whose details list everything it will set: a line per field, then the routines, then the instructions. Every client shows a card's details up to 2,000 characters (the terminal UI wrapped, not cut to a few lines), so a change whose details would run longer is refused before its card goes up. The call waits up to four and a half minutes, under Codex's five-minute MCP tool timeout, which drops a call without telling the server; under Auto it goes ahead. A card whose call stops waiting (that timeout, the agent's `notifications/cancelled`, which the `manage` process passes on by dropping the call) expires, and the bot goes back to working. Either way a notice ("Created bot Security Analyst", "Updated Reviewer: model, routines") records what changed. The card is the agent's kind of permission card, answered with `respondPermission`; Claude Code's own permission rules allow `mcp__manage` so it doesn't ask a second time. Routines are checked as `save_routine` checks them (lengths, triggers, timeout, and an `id` must be one of the target bot's), and fields too, before the card goes up.

A bot made this way records its creator (`createdBy`, next to `createdAt`); clients show "Created by Lead · Oct 20, 2026" in the bot's settings (Profile), and the terminal UI on the settings form's Name row. Only the host sets it: `createBot` and `updateBot` ignore it.

## Code and verification

- `host/src/chat/team.rs`: discovery, request lifetime and persisted notices and structured conversation data.
- `host/src/store/entries.rs` (`bot_conversation`, `expire_pending`) and `host/src/api/bots.rs` (`botConversation`): the pair history and restart cleanup.
- `host/src/tui/app/exchange.rs`, `manage/bot_chat.rs`, `view/bot_chat.rs`: the terminal client's row logic, keys and conversation sheet.
- `host/src/chat/team/requests.rs`: admission, wait graph, capacity and chain hop limit.
- `host/src/agent/bot/` (`queue.rs`, `turn.rs`): queue boundaries, recipient execution, completion and targeted cancellation.
- `host/src/mcp.rs`: the authenticated local MCP → HTTP bridge.
- `host/tests/team_e2e.rs`: a real host and MCP subprocess, with scripted ACP agents, covering discovery → delegation → recipient approval → reply → client sync. No paid provider calls.
- `host/src/chat/manage.rs` (+ `Cmd::Approve` in `agent/bot/updates.rs`): team management; `host/tests/manage_e2e.rs` walks an allowed `create_bot`, a denied `update_bot`, and cards that end when the turn stops or the agent cancels the call, through the real `manage` MCP process.

Run `cargo test` and `cargo clippy --all-targets -- -D warnings` in `host/`.
