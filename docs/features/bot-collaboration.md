# Bot collaboration

Create two or more bots normally, giving each a clear name, description, project folder and agent. Then message one:

> Ask Reviewer to inspect the changes in /path/to/project. Have it report bugs without editing files, then summarize its findings for me.

Every bot receives the built-in `team` MCP server. `list_bots` returns the other visible bots' IDs, kinds, descriptions, agents, folders and status. `message_bot(botId, message)` queues a self-contained request and returns at once, so the sender never waits: it ends its turn (or keeps working) and the recipient's final reply comes back to it later as a new turn in its own chat, where it answers you. The same tools work across ACP backends that support MCP servers.

The host records each exchange as one notice in both chats. Every client shows it as a compact row (centered in the apps, one ` · ` line in the terminal UI), "Messaged [avatar] Owen" in the sender's chat and "Message from [avatar] Egan" in the recipient's, with no request text. In a chat a run of consecutive exchanges with the same peer, in either direction, is one row, "3 messages with [avatar] Owen"; anything else the chat shows (a message, another notice, a card, an exchange with another peer) ends the run, trace entries do not, and the run is marked failed if any exchange failed. Only a failure is marked (danger color and a warning mark). Tapping the row opens a read-only "Egan ⇄ Owen" conversation sheet with the pair's whole history: author-labelled grey bubbles for each request and reply, time separators, "Waiting for Owen…" or the failure detail under a pending or failed exchange. It is laid out like the Full conversation sheet, with the pair as its title and an X to close. Tool details also remain in the trace. There is no additional team setup screen; a lead bot can set up the team itself (see Team management).

## Bots on other computers

When this computer is in a network with the user's other computers, `list_bots` also lists their bots (ids
`name@computer`) and `message_bot` reaches them: the request travels as a letter, runs in that bot's chat on its
computer, and the reply comes back as the same reply turn. `list_bots` takes `query`, `computer`, `online`, `kind`
(`codync`, `claude-code`, `codex`, `terminal`, `other`), `limit` and `cursor`. Notices for such an exchange carry
`botMessage.peerName`. Setup, roles and limits: [codync-network](codync-network.md).

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

`text` is the original request. Completed requests add `reply` (at most 4,000 bytes);
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

## How a message runs

- Each bot still owns an ACP process (its main session, plus one per routine run) and runs one turn at a time. A busy recipient queues the request. Different bots work concurrently, so a bot that messages several teammates has them work in parallel.
- `message_bot` returns `{requestId, status: "queued"}` as soon as the request is admitted. Nothing waits on another bot, so there are no wait cycles to guard against and no wait limit: the recipient takes as long as the work needs.
- Requests never merge with user messages or another request. Contiguous user messages keep the existing batching behavior.
- The recipient's final reply is addressed to the sender. It appears in both request notices (`botMessage.reply`) and the recipient's trace, without a bubble in the recipient's main chat; `send_message`, `send_file` and `message_bot` are refused in a request turn (a reply to a message sent from there would land in the recipient's own chat, and the sender would never get it). A recipient that needs another bot's help says so in its reply.
- The reply then reaches the sender's queue as a turn of its own in its main chat (`[Codync: Owen replied to your message_bot request <id> ("<request>").]`, quoting the first 1,000 bytes of the request, since the session taking the reply, the main one, may not be the one that sent it: a routine run's, or one replaced by a new session). Replies that arrive while the sender is busy are taken together in one turn. That turn's final reply is an ordinary message to you, with the usual "done" push; the request turn itself sends no push. A failed request comes back the same way (`couldn't finish …`), so the sender can tell you.
- A reply turn isn't the user speaking: memory doesn't take it as your words. If the sender wants another bot to have the answer, it messages that bot.
- The recipient uses its own folder, tools, memory and permission policy. Permission cards appear in its chat. The sender's transcript is not copied; only the request is passed. Requests are not treated as user facts by the memory keeper.
- In a group room turn `message_bot` is refused: the reply would land in the member's own chat, away from the room. Members @-mention each other there instead ([groups](groups-and-replies.md#how-a-group-answers-hostsrcchatgrouprs-grok-bots-design)).
- Native Claude Code/Codex subagents remain managed by the harness. Codync does not turn them into permanent bots or override their delegation settings.

The team tools add no task scheduler or isolated worktrees. Give bots explicit file ownership when they share a working directory.

## Stops, failures and restart

- Accepted requests survive the sender's completion, Stop, disconnection and deletion. Stop on the sender drops replies still queued for it; a deleted sender's replies are dropped (both notices still show the outcome).
- Stopping or deleting the recipient cancels its queued requests and stops running work; started work may have left partial changes. A cancellation the user caused doesn't wake the sender; its notices show it.
- Recipient startup failures, crashes, refusals and empty replies mark both notices failed and come back to the sender as a failed reply.
- Pending request notices are persisted with IDs and statuses. After host restart they become interrupted errors; request turns are not automatically replayed. Check partial work before retrying. There is no automatic retry. Ordinary user turns and running reply turns retain their existing session-resume behavior.
- A reply still queued for a busy sender lives only in memory: a host restart drops it, and the sender never relays it. Both notices are already completed and keep the reply (`botMessage.reply`), so it stays readable in the conversation sheet.

## Limits

The host rejects messages to the bot itself, to hidden or deleted bots, and empty or oversized (over 32,000 bytes) ones. Up to 64 messages can be outstanding host-wide, with at most 16 from any one sender. Queued and running messages both count. A slot is freed when the recipient finishes, fails or cancels the request, or admission fails. Stopping, restarting or deleting the sender does not free slots for its already accepted messages.

Bot request chains can make at most eight handoffs. A request turn messages no one, so a chain goes on only from reply turns: each reply carries the count into the sender's reply turn (the highest of the replies it takes), and finishing an earlier request does not reset it. This bounds repeated A → B → A messages even when few requests are outstanding. At the limit, the tool reports an error without queuing more work. A new user turn starts a fresh allowance.

## Team management

A bot whose settings have **Manage the team** on (off by default; only the user turns it on, on any client; a client hides the switch for a host that sends no `manageTeam`) also gets the built-in `manage` MCP server, so a lead bot can hire and set up the specialists it needs:

- `list_team`: every agent bot, the manager itself (`you: true`) and hidden ones included, with its setup (instructions, agent, model and its settings, fallbacks, avatar, folder, approvals, agent `mode`, whether it manages the team, who created it) and routines.
- `create_bot {name, instructions?, agent?, model?, avatarColor?, avatarShape?, folder?, routines?}`: a new bot with the agent a client's new bot gets. It manages the team too, and takes the manager's approval setting and agent mode, connectors and skills, so it can't act more freely or reach more than the manager (a bot under Ask can't create bots that don't ask; connectors the user turned off for the manager stay off); it never gets computer use. A manager in one of its agent's own modes, such as Plan, creates bots only on that agent: another may not have the mode. Without an avatar it gets a random look, as a client's new bot does. `routines` are `save_routine` bodies. Bots stop making bots at 20 that bots made (deleted ones don't count), so managers that make managers can't multiply without end; past that the manager asks the user.
- `update_bot {botId, name?, instructions?, model?, fallbacks?, avatarColor?, avatarShape?, hidden?, routines?}`: any agent bot, the manager itself included; a routine with an `id` replaces that routine (one of that bot's). `fallbacks` (up to 3 `{agent, model?}`, [agent fallbacks](agent-fallback.md)) replaces the bot's list, also for a bot whose user turned "Let the bot change its fallbacks" off: that switch is about the bot's own `set_fallbacks`. A changed model drops the bot's model settings (effort…); setting those, turning Manage the team on or off, deleting bots and every other setting stay with the user. No model id sets how the agent approves tools (`opus[mode=acceptEdits]`): the host never sends a model option of the agent's `mode` category. A new agent or model that arrives during a turn (the manager changing its own) restarts the agent once that turn ends, not in the middle of it; this holds for a change from the settings too.

A manager doesn't run other bots' routines. To test a teammate's, it asks that bot with `message_bot` to run it with its own `run_routine` (by id), so the run is the owner's: its routine session, timeout, retries, run history and result in the owner's chat. The `manage` instructions say so, the `routines` instructions let a bot do it when another bot asks for one of its own routines, and `run_routine` with an id that isn't the caller's fails with that hint ([routines](routines.md#conversational-setup-and-tools)). A hidden bot can't be messaged, so for its routine the manager asks the user to run it from that bot.

Under Ask each call first posts an approval card in the manager's chat (Allow / Deny) whose details list everything it will set: a line per field, then the routines, then the instructions. Every client shows a card's details up to 2,000 characters (the terminal UI wrapped, not cut to a few lines), so a change whose details would run longer is refused before its card goes up. The call waits up to four and a half minutes, under Codex's five-minute MCP tool timeout, which drops a call without telling the server; under Auto it goes ahead. A card whose call stops waiting (that timeout, the agent's `notifications/cancelled`, which the `manage` process passes on by dropping the call) expires, and the bot goes back to working. Either way a notice ("Created bot Security Analyst", "Updated Reviewer: model, routines") records what changed. The card is the agent's kind of permission card, answered with `respondPermission`; Claude Code's own permission rules allow `mcp__manage` so it doesn't ask a second time. Routines are checked as `save_routine` checks them (lengths, triggers, timeout, and an `id` must be one of the target bot's), and fields too, before the card goes up.

A bot made this way records its creator (`createdBy`, next to `createdAt`); clients show "Created by Lead · Oct 20, 2026" in the bot's settings (Profile), and the terminal UI on the settings form's Name row. Only the host sets it: `createBot` and `updateBot` ignore it.

## Code and verification

- `host/src/chat/team/remote.rs` and `directory.rs`: requests to and from other computers, and `list_bots`;
  `host/tests/network_e2e.rs` runs two hosts in one network.

- `host/src/chat/team.rs`: discovery, request lifetime, persisted notices, structured conversation data and the reply that goes back (`BotReply`, `reply_prompt`).
- `host/src/store/entries.rs` (`bot_conversation`, `expire_pending`) and `host/src/api/bots.rs` (`botConversation`): the pair history and restart cleanup.
- `host/src/tui/app/exchange.rs`, `manage/bot_chat.rs`, `view/bot_chat.rs`: the terminal client's row logic, keys and conversation sheet.
- `host/src/chat/team/requests.rs`: admission, capacity and chain hop limit.
- `host/src/agent/bot/` (`queue.rs`, `turn.rs`): queue boundaries, recipient execution, completion and the sender's reply turn.
- `host/src/mcp.rs`: the authenticated local MCP → HTTP bridge.
- `host/tests/team_e2e.rs`: a real host and MCP subprocess, with scripted ACP agents, covering discovery → message → recipient approval → reply turn in the sender's chat → client sync, and the hop limit. No paid provider calls.
- `host/src/chat/manage.rs` (+ `Cmd::Approve` in `agent/bot/updates.rs`): team management; `host/tests/manage_e2e.rs` walks an allowed `create_bot`, a denied `update_bot`, and cards that end when the turn stops or the agent cancels the call, through the real `manage` MCP process.

Run `cargo test` and `cargo clippy --all-targets -- -D warnings` in `host/`.
