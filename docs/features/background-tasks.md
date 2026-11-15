# Background tasks

A bot puts long work in its **box** as **items**. **Tasks** take them: other sessions of the same bot that work away
from the chat, so the chat keeps answering. The chat shows only messages and what needs the user (a task's approval
cards); the work shows as one capsule above the composer, and each item's report comes back as the bot's own message.

| | The chat (the coordinator) | A task |
|---|---|---|
| Session | The bot's main one | Its own, forked from the main one where the agent can |
| Agent process | The bot's | Its own |
| Transcript | The chat | Its own, read from the item's details |
| Who talks to it | The user | No one: it gets items, one at a time |
| Its result | A message | Its item's report, which the bot turns into a message |

Design: [the event box](../plans/event-box.md); why a task is a bot of its own, and the fork spikes:
[the first design](../archive/background-tasks-plan-2026-11-15.md). Code: `host/src/chat/tasks/` (`items.rs`: the box, its wire shape and what the bot reads of it;
`dispatch.rs`: which item a task takes next; `steer.rs`: first, cut in, replace, stop, drop, retry; `mod.rs`: a
task's turn end, the report, host start; `tools.rs`: the bot's tools and what a task may not call; `prompts.rs`: the
words); the actor's side is in `host/src/agent/bot/` (`session.rs` for the fork, `turn.rs` for the box in the prompt
and the report, `queue.rs` for `turn_over`, `updates.rs` for cards).

## An item

One piece of work: `id, title, brief, words` (the user's own, word for word), `notes` (what the user said about it
since), `status, order, after, task, cutIn, hops, createdAt, startedAt, endedAt, result, report`. A bot's items are one
JSON list in kv `work.<bot>`; every change to a box is made under one lock (`items::change`).

| Status | Meaning |
| --- | --- |
| `waiting` | In the box; no task works on it |
| `working` | A task works on it |
| `needsYou` | Its task waits on an approval (never saved: a working item reads so while its task's card is open) |
| `done` | Finished; `result` is what its task reported |
| `failed` | It could not be finished; `result` says why |
| `stopped` | The user or the bot stopped it |

## A task is a bot of its own inside the host

Everything a running turn touches is keyed by bot id, so a task is a **task bot**: a `bots` row with
`taskOf = <bot id>` and id `task-<uuid>`, its config copied from its bot when it starts (without `computer` and
`manage`). It has its own actor, agent process, session, transcript, restart mark, fallback state and compaction
record. Its memory and instruction snapshot are its bot's (`BotConfig::as_owner`; its `memory` MCP server is started
for its bot).

No client lists a task bot. `Store::bots()` leaves them out, `emit_bot` for one emits its bot instead, and its
entries leave as `taskEntry` events, not `entry`.

A task works one item per turn. When the turn ends the item is done and reported; a task with no item left ends (its
actor and agent process stop; its transcript stays for its items' details). A task bot no item names any more is
deleted with its transcript.

## The dispatcher

The user sets the most tasks a bot runs at once, `maxAgents` (1 to 6, 3 unless changed, `updateBot`). It is
the only ceiling: the computer as a whole has none. `tasks::dispatch(hub, bot)` runs whenever the box or one of its
tasks changes: an item added, moved, dropped, stopped or retried; a task's turn over; the bot's own turn over; `updateBot`; host start. For each waiting
item:

1. **It follows an item** (`after`) that is done and whose task is still there and idle: that task takes it, in the
   same session. These go first, since that task already has its place. While what it follows waits or works, it
   waits (`wait: after`). When what it follows failed, was stopped or dropped, or its task is gone, it is an ordinary
   item.
2. **Otherwise a new task starts for it**, lowest `order` first, when the bot is under `maxAgents`, the bot's own turn is over (a fork taken mid-turn would carry an unanswered tool call), and the agent is
   not near its usage limit (near it, one task at a time: `wait: usage`).
3. **Otherwise it waits**: `wait: next` for the first in line, `behind` with how many are `ahead` for the rest.
   Nothing is refused for being over the ceiling.

A new task waits for any turn of the bot to end, a routine run's or a room's too (they share its actor); that turn's
end looks over the box again.

The decisions are one pure function (`dispatch::standings`), which the wire's `wait` reads too.

## Tools (on the `chat` MCP server)

- `add_work {title, brief, after?}` → `{itemId, status}`: puts an item in the box and answers at once. Refused
  outside the bot's own chat with the user (a room, another bot's request, a routine run: `add_work works only in
  your own chat with the user`), in a task, past 20 waiting items, when `after` names
  no item that is working or waiting, and for the ninth item added in a row with no message from the user (an item
  added from a report's turn carries that report's hops). Stopping the turn that added an item drops it.
- `steer_work {item, how, brief?, insteadOf?}`: `first` (a waiting item moves to the front), `now` (it cuts in),
  `replace` (a new brief), `stop`. The user's own words of the running turn are added to the item's `notes`.
- `work_status {}`: every item that is working or waiting, and those ended in the last day: status, what its task
  does now, why it waits, seconds, result.

Refused in a task: these three, `request_decision`, `send_file`, artifacts, and changes to the bot's profile or
fallbacks (`tasks::refuse_chat_only`, the `chat` server's), and the `routines`, `team` and `manage` tools
(`tasks::refuse_in_task`). The refusal says to put what it needs in its report.

A task may start [dev servers](dev-servers.md). They are its bot's: listed on the bot, stopped there, counted toward
its limit, and so is what the task's agent leaves listening through its own shell. The approval card is the task's,
in the chat like its others.

The bot's instructions carry `tasks::BRIEF`, and each message in its own chat ends with `tasks::EACH_TURN`: with the
instructions alone a real agent did 90 seconds of reading in the chat's turn (checked with Claude on 2026-11-15; with
the line it handed the work on, and answered a quick question itself). On a known harness each of those turns also
carries the box in a few lines (`tasks::lines`: the counts and how many run at once, then each working and waiting
title with its id), so the
bot can say what it is doing and place an `after` without a tool call.

## Steering

- **First** (`workFirst`, `steer_work first`): a waiting item moves to the front. Order only.
- **Cut in** (`workCutIn`, `steer_work now`): the item moves to the front. With every place taken a working item
  gives way (the one named with `insteadOf`, else the one that started last): its task is stopped and deleted, and it
  waits at the front, behind the item that cut in, with `cutIn` set. It starts again later in a new task and session,
  told that the work was begun before. With a place free, cutting in is moving first; so it is for an item that
  wouldn't start in the freed place (it follows unfinished work, or waits for usage), and the clients don't offer
  Cut in on such a row.
- **Replace** (`steer_work replace`): the brief is replaced. A working item's task is closed the same way and the
  item starts again from the new brief, at the front (`cutIn` stays as it was).
- **Stop** (`stopWork`, `steer_work stop`): the item is `stopped`; a working one's task is stopped. Without an item,
  `stopWork` stops every item that is working or waiting. Items that followed a stopped one become ordinary.
- **Drop** (`dropWork`): a waiting or ended item leaves the box. A working one is refused: stop it first.
- **Try again** (`retryWork`): a stopped or failed item goes back to waiting, last, and starts in a new task.

A task that gives its item up is told to stop; when its turn has ended, the dispatcher clears it away.

## How a task's session begins

- **Fork**: `session/fork` of the bot's main session, sent from the task's own process with the main session's
  frozen instructions, so the prompt cache is the chat's. Conditions: the agent advertises
  `sessionCapabilities.fork`, the bot is on its own agent (not a fallback), and the main session has used under half
  its window.
- **Brief**: `session/new`, with the chat so far before the task's first message (`handoff::build`: summary, last
  requests, last reply, plan). Used when a condition fails or the fork errors.

A task's message for an item says what it is, gives the brief, the user's own words and what they said since, and
how its words reach the user (`prompts::item_prompt`). An item that follows another is the task's next message in
the session it has.

## Report

When a task's turn ends, its last message is its item's `result` and goes to its bot as `Cmd::TaskReport`, one per
item, which becomes a turn in the chat (`HostTurn::TaskReport`; reports that arrive together are one turn). The
bot's reply there is an ordinary message with the usual alert; `(pass)` stays silent. A report is kept on the item
until that turn has ended: a host restart before then sends it again (the cut-off report turn isn't resumed). When
the turn ends without the bot's answer (stopped, failed, or it never started), the chat shows the report as a notice
(`"<title>" finished in the background: …`), so what the task found isn't lost with the turn. An item the user
stopped reports nothing. Reports are `from: host`: memory and dreaming don't take them as the user's words.

An item that doesn't finish reports why, the same way: its task's agent couldn't start (`it could not start: …`),
failed mid-turn, or ended short of `end_turn`. Every way a task's turn ends goes through `Actor::turn_over`, so no
item is left working with nothing behind it.

## Approvals

A task's permission request becomes a card in **its bot's chat**, with `data.task = {id, title}` (the task bot's id,
its item's title), `data.item` (the item's id) and the bot as `data.author`, so `respondPermission` reaches the
task's actor and a client can link the card to the item. The approval setting is the bot's (copied when the task
starts). A waiting card stops only its task; the chat goes on.

## Wire

- Bot: `work: [item]`: every working item (oldest first), then the waiting in dispatch order, then the items ended
  in the last 24 hours (newest first, at most 20). Item: `{id, title, status, activity, task, wait, cutIn,
  createdAt, startedAt, endedAt, result}`; `activity` is its task's, `task` the task bot that has or had it, `wait`
  is `{kind: next | behind | after | usage, ahead?}` on a waiting item and null otherwise, `result` at most 280
  characters. The bot's own `status` stays the chat's.
- Bot: `maxAgents`, set with `updateBot` (1 to 6).
- `taskEntry` events `{rev, task, entry}` for a task's transcript; none in the catch-up.
- `taskEntries {botId, taskId}` → `{task: {id, title, status, startedAt, endedAt, start}, entries}` (newest 500):
  `taskId` is an item's `task`; `title`, `status` and the times are of the item it works on, else the last it
  worked on.
- `workFirst {botId, itemId}`, `workCutIn {botId, itemId, insteadOf?}`, `dropWork {botId, itemId}`,
  `stopWork {botId, itemId?}`, `retryWork {botId, itemId}`.

## In the apps

The capsule, the bot's Details (Threads, Event box, Later, Routines, Dev servers, Recent), `Agents at once` in its
Settings and cutting in are specified in [the event box](../plans/event-box.md#in-the-apps) (stage 2). A client reads `bot.work` and `bot.maxAgents`; both are
absent on an older host, where the feature doesn't show.

## Restart and cleanup

At host start (`tasks::restore`): items ended over 7 days ago are deleted; a working item whose task's turn was cut
off and can be resumed (`agent::bot::resumable`: a restart mark under an hour old) stays working, and its task's
actor resumes the turn; one whose task can't goes back to waiting once, as an item that was cut in on, and fails with
a report the second time. An ended item's undelivered report is sent again. Task bots whose bot is gone, or that no
item names, are deleted. Deleting a bot deletes its box and its tasks.

The hourly look (`tasks/sweep.rs`) catches what no event said. After every dispatcher pass that leaves waiting or
working items in a box, one timer per bot is set for an hour later (reset by the next pass, none for an empty box).
When it fires, the host takes a dispatcher pass again, and a working item whose task runs no turn (its actor is
gone or idle, and nothing is about to resume) follows the restart rule: back to waiting once, then failed with a
report (`work_finished` `error: lost`). A turn that runs is never touched, however long and quiet. The look is host
code only: no agent is woken unless an item was lost.

## Limits of this version

- A task's memory tools run as its bot, so a task's `edit_memory` is credited to the chat's session, and its edits
  through file tools are taken at the bot's next scan as changes outside a turn.
- No worktree for tasks that run side by side; no subagent rows.
- What a task's agent runs inside its own process (a harness's background agents and shells in the agent's session)
  ends when the task does: its process and that session's processes are stopped (`Acp::kill`). A program it
  detached into a session of its own (a server from its shell) isn't reached and keeps running; one left listening
  shows among the bot's [dev servers](dev-servers.md).
- A voice call reads the box (`bot_status`, and the call's opening context) and its `stop_bot` stops the items too,
  but a task's activity isn't in the call's progress cues.
- A task ends as soon as its item does when nothing waits to follow it, so `after` is for work that is still
  working or waiting.
- Only the bot adds items; routines and other bots' requests keep their own paths.

Tests: `host/tests/tasks_e2e.rs` with `tests/fixtures/task_agent.py` (fork, brief, a card with `data.item` answered
from the chat, refusals in a task from each server, the ceiling and the order, first, drop, cut in from the user and
from the bot, replace on a waiting and on a working item, `after` in the same session, stop one and all, retry, an
agent that can't start, a stopped turn dropping the item it added, a deleted bot taking its box and tasks, a host
restart mid-item, cut in on an item that follows working one closing nothing, no `bot` or `entry` event for a task
bot); unit tests of the dispatch order and the `wait` reasons
(`dispatch.rs`), an item's end and the restart rule (`mod.rs`), the 20 waiting items and the title and brief lengths (`tools.rs`), the prompts and the box's lines. Not
covered by a test: the usage and in-a-row limits, the desktop's Add a fallback opening Settings, `add_work` refused in a room or a bot's request (a routine run is: `routines/tests.rs`), a report turn that fails, a
task's dev server, an item sent back to waiting by a restart end to end, the 7-day purge, and the box's lines in a
known harness's prompt.
