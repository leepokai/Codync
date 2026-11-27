# Bot collaboration

Create two or more bots normally, giving each a clear name, description, projects and agent. Then message one:

> Ask Reviewer to inspect the changes in /path/to/project. Have it report bugs without editing files, then summarize its findings for me.

Every bot receives the built-in `team` MCP server. `list_bots` returns the other visible bots' IDs, kinds, descriptions, agents, folders (`cwd`: the default project, else the bot's own folder; and `projects`) and status. `message_bot(botId, message)` queues a self-contained request and returns at once, so the sender never waits: it ends its turn (or keeps working) and the recipient's final reply comes back to it later as a new turn in its own chat, where it answers you. The same tools work across ACP backends that support MCP servers.

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
migration or transcript-cache reset. At restart, unfinished requests settle as
[Stops, failures and restart](#stops-failures-and-restart) says: one between this
computer's bots that never started is sent again; started work is never replayed.

`botConversation {botId, peerId}` returns `{entries}`: the newest 200 main-chat
notices containing structured messages with that peer, oldest first. It requires
the Control scope. Clients merge these with the live chat (the higher `rev` wins)
and keep the result in the sheet; it is never added to the chat mirror, so main
history paging is unaffected.

## How a message runs

- A request is a task in the recipient's [box](work.md#other-sources): a thread of the recipient answers it (in a new session that sees nothing of the recipient's chat), so the recipient's chat stays free and requests wait for a free thread under its `maxThreads` like its other work. The task's row carries the asking bot (`source: {kind: request, id}`) and the request's first line as its title; the thread's transcript is the request's trace. Its last message is the reply as it is, a `(pass)` included. A recipient with computer use or Manage the team on answers in its chat instead, one turn at a time, as before (a thread has neither). Different bots work concurrently, so a bot that messages several teammates has them work in parallel.
- `message_bot` returns `{requestId, status: "queued"}` as soon as the request is admitted. Nothing waits on another bot, so there are no wait cycles to guard against and no wait limit: the recipient takes as long as the work needs.
- Requests never merge with user messages or another request. Contiguous user messages keep the existing batching behavior.
- The recipient's final reply is addressed to the sender. It appears in both request notices (`botMessage.reply`) and the recipient's trace (its thread's), without a bubble in the recipient's main chat and without a turn of the recipient's own; `send_message`, `send_file` and `message_bot` are refused in a request turn (a reply to a message sent from there would land in the recipient's own chat, and the sender would never get it), and so are `follow_up` (its later turn would run there too: [follow-ups](follow-ups.md)) and `add_task` (its report would come back to the recipient's own chat: [work](work.md)). A recipient that needs another bot's help says so in its reply. A request or reply from a computer whose network rule is `auto` or `ask` holds its turn to a [ceiling](codync-network.md#ceilings-auto-and-ask), which refuses more.
- The reply then reaches the sender's queue as a turn of its own in its main chat (`[Codync: Owen replied to your message_bot request <id> ("<request>").]`, quoting the first 1,000 bytes of the request, since the session taking the reply, the main one, may not be the one that sent it: a routine run's, or one replaced by a new session). Replies that arrive while the sender is busy are taken together in one turn. That turn's final reply is an ordinary message to you, with the usual "done" push; the request turn itself sends no push. A failed request comes back the same way (`couldn't finish …`), so the sender can tell you.
- A reply turn isn't the user speaking: memory doesn't take it as your words. If the sender wants another bot to have the answer, it messages that bot.
- The recipient uses its own folder, tools, memory and permission policy. Permission cards appear in its chat (a thread's, with the task's line). The sender's transcript is not copied; only the request is passed. Requests are not treated as user facts by the memory keeper.
- In a group room turn `message_bot` is refused: the reply would land in the member's own chat, away from the room. Members @-mention each other there instead ([groups](groups-and-replies.md#how-a-group-answers-hostsrcchatgrouprs-grok-bots-design)).
- Native Claude Code/Codex subagents remain managed by the harness. Codync does not turn them into permanent bots or override their delegation settings.

The team tools add no task scheduler or isolated worktrees. Give bots explicit file ownership when they share a working directory.

## Stops, failures and restart

- Every request that returned `queued` ends with its sender hearing how: a reply, a failure or a cancellation comes back as a reply turn (`couldn't finish …`), and nothing leaves a request waiting forever.
- Accepted requests survive the sender's completion, Stop, disconnection and deletion. Stop on the sender drops replies still queued for it; a deleted sender's replies are dropped (both notices still show the outcome).
- Stopping a request's task in the recipient's box (Stop on its row, or stopping all of its work), dropping it while it waits, or deleting the recipient cancels the request; started work may have left partial changes. Stop in the recipient's chat stops only the chat's turn (for a recipient that answers in its chat: its queued requests and running work, as before). The sender is woken with the cancellation, as for a failure, and tells the user; a request stopped on another computer comes back the same way. A request's task can't be tried again: the sender asks again.
- Recipient startup failures, crashes, refusals and empty replies mark both notices failed and come back to the sender as a failed reply.
- Host restart (`host/src/chat/team/restart.rs`, run once the bots' actors and boxes are back): a request between this computer's bots that hadn't started (both notices still `queued`) is sent again, with its notices, its sender's slot and its chain's hops (`hops` on the notice); one that had started (`sent`) fails with "Interrupted by host restart", since its work may be half done, and its sender is woken with that; one whose recipient was deleted or hidden meanwhile fails the same way. A host shutting down leaves its requests' notices as they are for this (`Routines::stopping`), instead of marking them cancelled. A request whose notices a stopping host left half saved (one ended, the other not) ends the other one the same way, with no second outcome for the sender. Requests to or from another computer, or the cloud assistant, are still marked interrupted at start (`Store::expire_pending`) and not replayed. Ordinary user turns and running reply turns retain their existing session-resume behavior.
- A reply on its way to a local sender is marked on the sender's notice (`relaying`) until the sender's reply turn has handed it to the agent (or the user stops the sender, which drops it). A host restart in between sends it to the sender again from that notice, whole: the notice shows only its first 4,000 bytes, so the host keeps the reply itself until the sender's turn takes it. The sender's reply turn is marked to resume after a restart only once its prompt has gone out, so a reply is never both sent again and resumed. A reply from another computer isn't kept this way: the ceiling it came under doesn't outlive the host.

## Limits

The host rejects messages to the bot itself, to hidden or deleted bots, and empty or oversized (over 32,000 bytes) ones. Up to 64 messages can be outstanding host-wide, with at most 16 from any one sender. Queued and running messages both count. A slot is freed when the recipient finishes, fails or cancels the request, or admission fails. Stopping, restarting or deleting the sender does not free slots for its already accepted messages.

Bot request chains can make at most eight handoffs. A request turn messages no one, so a chain goes on only from reply turns: each reply carries the count into the sender's reply turn (the highest of the replies it takes), and finishing an earlier request does not reset it. This bounds repeated A → B → A messages even when few requests are outstanding. At the limit, the tool reports an error without queuing more work, and the turn leaves one info notice in the bot's chat (`continueChain: true`, its text the whole story for an older client). Its **Continue** button (TUI: `F`, or `F` on the picked notice) calls `continueChain {entryId}`, which first clears the flag (the button goes) and then sends the bot "Continue" as the user's own message, so it works once. A bot that isn't running is refused before any of this, and a notice that can't be updated refuses the press and sends nothing; a message that can't be saved puts the button back to press again, while one that was saved keeps it off even if the bot's actor didn't take it, so presses never stack undelivered messages. A new user turn starts a fresh allowance, so that message, or any other, starts a new chain. `add_task`'s limit on tasks added in a row ([work](work.md)) leaves the same notice. Further refusals in the same turn add no second notice; a Continue pressed while the turn still runs waits behind it like any message (`host/src/chat/team/limit.rs`).

## Team management

A bot whose settings have **Manage the team** on (off by default; only the user turns it on, on any client; a client hides the switch for a host that sends no `manageTeam`) also gets the built-in `manage` MCP server, so a lead bot can hire and set up the specialists it needs:

- `list_team`: every agent bot, the manager itself (`you: true`) and hidden ones included, with its setup (instructions, agent, model and its settings, fallbacks, avatar, folder, approvals, agent `mode`, whether it manages the team, who created it) and routines.
- `create_bot {name, instructions?, agent?, model?, modelOptions?, avatarColor?, avatarShape?, folder?, routines?}`: a new bot with the agent a client's new bot gets. It manages the team too, and takes the manager's approval setting and agent mode, connectors and skills, so it can't act more freely or reach more than the manager (a bot under Ask can't create bots that don't ask; connectors the user turned off for the manager stay off); it never gets computer use. A manager in one of its agent's own modes, such as Plan, creates bots only on that agent: another may not have the mode. Without an avatar it gets a random look, as a client's new bot does. `routines` are `save_routine` bodies. Bots stop making bots at 20 that bots made (deleted ones don't count), so managers that make managers can't multiply without end; past that the manager asks the user.
- `delete_bot {botId}`: delete one agent bot after the user's Allow on an approval card, even when this manager works under Auto. The card shows the bot's id as well as its name, so two bots with one name can be told apart. Refuses a group, thread, thread box, itself or the current lead; a bot made lead while the card waits is refused at the write, never half-deleted. Deny, a stopped turn, a timed-out card or MCP cancel leaves the bot alone. The manager's chat records `Deleted <name>`; routines, threads and files go with it.
- `update_bot {botId, name?, instructions?, model?, modelOptions?, fallbacks?, avatarColor?, avatarShape?, hidden?, routines?}`: any agent bot, the manager itself included; a routine with an `id` replaces that routine (one of that bot's). `fallbacks` (up to 3 `{agent, model?}`, [agent fallbacks](agent-fallback.md)) replaces the bot's list, also for a bot whose user turned "Let the bot change its fallbacks" off: that switch is about the bot's own `set_fallbacks`. `modelOptions` sets the model's settings (`{"effort": "high"}` for Claude, `reasoning_effort` for Codex, Cursor's per-model options), as `list_team` shows them; it replaces them all, and `{}` goes back to the model's defaults. Before its card goes up, and within the call's approval window (the card waits only for what the check left), a non-empty map is checked against the agent's model list (`agentModels`, `auth::check_options`) for the bot's model, the new one when the call changes it: an id or value that model doesn't list is refused, as a mode is (the list never holds `mode`-category options), so a value the agent would refuse can't keep the bot from starting. A changed model without `modelOptions` drops the bot's settings, so a manager that switches a bot's model and keeps an effort passes both in one call. Turning Manage the team on or off and every other setting stay with the user; a bot deletes one agent bot only through `delete_bot`, after the user's Allow on its card. No model id sets how the agent approves tools (`opus[mode=acceptEdits]`): the host never sends a model option of the agent's `mode` category. A new agent or model that arrives during a turn (the manager changing its own) restarts the agent once that turn ends, not in the middle of it; this holds for a change from the settings too.

A manager doesn't run other bots' routines. To test a teammate's, it asks that bot with `message_bot` to run it with its own `run_routine` (by id), so the run is the owner's: its routine session, timeout, retries, run history and result in the owner's chat. The `manage` instructions say so, the `routines` instructions let a bot do it when another bot asks for one of its own routines, and `run_routine` with an id that isn't the caller's fails with that hint ([routines](routines.md#conversational-setup-and-tools)). A hidden bot can't be messaged, so for its routine the manager asks the user to run it from that bot.

Under Ask each call first posts an approval card in the manager's chat (Allow / Deny) whose details list everything it will set: a line per field, then the routines, then the instructions. Every client shows a card's details up to 2,000 characters (the terminal UI wrapped, not cut to a few lines), so a change whose details would run longer is refused before its card goes up. The call waits up to four and a half minutes, under Codex's five-minute MCP tool timeout, which drops a call without telling the server; under Auto changes other than a delete go ahead. `delete_bot` always posts the card, even under Auto. A turn a network rule holds to a [ceiling](codync-network.md#ceilings-auto-and-ask) can't call `manage` at all, only its read (`list_team`). A card whose call stops waiting (that timeout, the agent's `notifications/cancelled`, which the `manage` process passes on by dropping the call) expires, and the bot goes back to working. Either way a notice ("Created bot Security Analyst", "Updated Reviewer: model, routines") records what changed. The card is the agent's kind of permission card, answered with `respondPermission`; Claude Code's own permission rules allow `mcp__manage` so it doesn't ask a second time. Routines are checked as `save_routine` checks them (lengths, triggers, timeout, and an `id` must be one of the target bot's), and fields too, before the card goes up.

A bot made this way records its creator (`createdBy`, next to `createdAt`); clients show "Created by Lead · Oct 20, 2026" in the bot's settings (Profile), and the terminal UI on the settings form's Name row. Only the host sets it: `createBot` and `updateBot` ignore it.

## Code and verification

- `host/src/chat/team/remote.rs` and `directory.rs`: requests to and from other computers, and `list_bots`;
  `host/tests/network_e2e.rs` runs two hosts in one network.

- `host/src/chat/team.rs`: discovery, request lifetime, persisted notices, structured conversation data and the reply that goes back (`BotReply`, `reply_prompt`).
- `host/src/store/entries.rs` (`bot_conversation`, `expire_pending`) and `host/src/api/bots.rs` (`botConversation`): the pair history and restart cleanup.
- `host/src/tui/app/exchange.rs`, `manage/bot_chat.rs`, `view/bot_chat.rs`: the terminal client's row logic, keys and conversation sheet.
- `host/src/chat/team/requests.rs`: admission, capacity and chain hop limit; `limit.rs`: the Continue notice and `continueChain`.
- `host/src/agent/bot/` (`queue.rs`, `turn.rs`): queue boundaries, recipient execution, completion and the sender's reply turn.
- `host/src/mcp.rs`: the authenticated local MCP → HTTP bridge.
- `host/tests/team_e2e.rs`: a real host and MCP subprocess, with scripted ACP agents, covering discovery → message → recipient approval → reply turn in the sender's chat → client sync, and the hop limit. No paid provider calls.
- `host/src/chat/manage.rs` (+ `Cmd::Approve` in `agent/bot/updates.rs`): team management; `host/tests/manage_e2e.rs` walks an allowed `create_bot`, a denied `update_bot`, an allowed and denied `delete_bot` (whose card appears even under Auto), and cards that end when the turn stops or the agent cancels the call, through the real `manage` MCP process.

Run `cargo test` and `cargo clippy --all-targets -- -D warnings` in `host/`.
