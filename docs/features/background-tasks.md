# Background tasks

A bot puts long work in its **box** as **items**. **Tasks** take them: other sessions of the same bot that work away
from the chat, so the chat keeps answering. The chat shows only messages and what needs the user (a task's approval
and choice cards, the sign-in card its agent needs, its offer to add a fallback, the files and artifacts it shares);
the work shows as one capsule above the composer, and each
item's report comes back as the bot's own message. A run of one of the bot's routines and another bot's request are
items too ([other sources](#other-sources)).

| | The chat (the coordinator) | A task |
|---|---|---|
| Session | The bot's main one | Its own, forked from the main one where the agent can (a routine's run: a new one) |
| Agent process | The bot's | Its own |
| Transcript | The chat | Its own, read from the item's details |
| Who talks to it | The user | It gets items, one at a time; the user may write to the [thread](#talking-to-a-thread) of the bot's own item |
| Its result | A message | Its item's report, which the bot turns into a message |

Design: [the event box](../plans/event-box.md); why a task is a bot of its own, and the fork spikes:
[the first design](../archive/background-tasks-plan-2026-11-15.md). Code: `host/src/chat/tasks/` (`items.rs`: the box, its wire shape and what the bot reads of it;
`dispatch.rs`: which item a task takes next; `steer.rs`: first, cut in, replace, stop, drop, retry; `mod.rs`: a
task's turn end, the report, host start; `sources.rs`: routine runs and other bots' requests as items; `thread.rs`: the user's messages to an item's task;
`tools.rs`: the bot's tools and what a task may not call; `sweep.rs`: the hourly look; `worktree.rs`: a task's git worktree;
`prompts.rs`: the words); the actor's side is in `host/src/agent/bot/` (`session.rs` for the fork, `turn.rs` for the box in the prompt, the worktree
and the report, `queue.rs` for `turn_over`, `updates.rs` for cards). Git is run through `host/src/git.rs`.

## An item

One piece of work: `id, title, brief, source` (where it came from: the bot, a routine's run, another bot's request),
`words` (the user's own, word for word), `notes` (what the user said about it
since), `status, order, after, task, cutIn, hops, createdAt, startedAt, endedAt, result, report, failure, worktree,
told` (what the user wrote to its thread since its last report), `said` (what it reported before the user wrote to
it again). A bot's items are one
JSON list in kv `work.<bot>`; every change to a box is made under one lock (`items::change`).

| Status | Meaning |
| --- | --- |
| `waiting` | In the box; no task works on it |
| `working` | A task works on it |
| `needsYou` | Its task waits on an approval, a choice or its agent's [sign-in](#fallbacks-and-sign-in) (never saved: a working item reads so while its task's card is open) |
| `done` | Finished; `result` is what its task reported |
| `failed` | It could not be finished; `result` says why |
| `stopped` | The user or the bot stopped it |

## A task is a bot of its own inside the host

Everything a running turn touches is keyed by bot id, so a task is a **task bot**: a `bots` row with
`taskOf = <bot id>` and id `task-<uuid>`, its config copied from its bot when it starts (without `computer` and
`manage`). It has its own actor, agent process, session, transcript, restart mark, fallback state and compaction
record. Its memory and instruction snapshot are its bot's (`BotConfig::as_owner`); its `memory` MCP server is its
own, and reads and writes its bot's memory as the task's own turn ([memory](#memory)).

No client lists a task bot. `Store::bots()` leaves them out, `emit_bot` for one emits its bot instead, and its
entries leave as `taskEntry` events, not `entry`.

A task works one item per turn. When the turn ends the item is done and reported (unless a message of the user's
waits in its [thread](#talking-to-a-thread): that message's turn is the item's end); a task with no item left ends (its
actor and agent process stop; its transcript stays for its items' details, and a message to one of them starts it again). A task bot no item names any more is
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
  added from a report's turn carries that report's hops). Stopping the turn that added an item drops it. Refused
  too in a turn a network rule holds to a ceiling, such as one that reads another computer's answer
  ([ceilings](codync-network.md#ceilings-auto-and-ask)): its work would run without it.
- `steer_work {item, how, brief?, insteadOf?}`: `first` (a waiting item moves to the front), `now` (it cuts in),
  `replace` (a new brief), `stop`. The user's own words of the running turn are added to the item's `notes`.
  Refused under a ceiling, as `add_work` is.
- `work_status {}`: every item that is working or waiting, and those ended in the last day: status, what its task
  does now, why it waits, seconds, result.

Refused in a task: these three and changes to the bot's profile or fallbacks (`tasks::refuse_chat_only`, the `chat`
server's), and the `routines`, `team` and `manage` tools (`tasks::acting_as`, `tasks::refuse_in_task`). The refusal
says to put what it needs in its report. `request_decision`, `send_file` and artifacts work from a task
([below](#asking-and-sharing-from-a-task)). A task on a routine's run or another bot's request has that turn's own
rules instead ([other sources](#other-sources)).

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

A task that gives its item up is told to stop; when its turn has ended, the dispatcher clears it away. Until then it
keeps its place (`dispatch::places` counts a task whose turn runs with no item at work), so the item that replaces it
waits rather than run beside it. The dispatcher starts a planned task, and hands work to one, under the box's lock and
only while the item is still on it, so a Stop that came in between wins.

## Talking to a thread

The user writes to the task that works, or worked, one of the bot's **own** items, as to a chat (`thread.rs`). The
task is a bot with an actor, so the message is a user entry in the task's own transcript and goes through the
chat's own queue: nothing here is a second copy of it.

- **`send {botId, itemId, text, clientNonce?}`** → `{entry, reopened, queued}`. The caller names the bot and its
  item, never a task: the item must be in that bot's box, and `send`, `steer` and `unqueue` refuse a task's own id
  as `botId` (`unknown bot`). Text only (`a thread takes text only` for attachments, `replyTo` or `call`). `entry`
  is the message as the transcript holds it (`botId` is the task's id); the same `clientNonce` again answers the
  same entry, with `reopened` and `queued` null.
- **While the task's turn runs** the entry is `status: queued`, as in a chat, and leaves as `taskEntry` events like
  the rest of the transcript. **`steer {botId, itemId, entryId}`** sends it now: into the running turn where the
  agent can (`_session/steering`), else the turn is stopped for it. **`unqueue {botId, itemId, entryId}`** takes it
  back (`status: cancelled`, `removed: true`; `taskEntries` no longer lists it). With no turn running the message
  starts one. The statuses are the chat's: `queued`, `sent`, `cancelled`, `failed`.
- **The item isn't over while a message waits.** A turn that ends (done, failed, or stopped for Send now) with a
  message of the user's still queued ends nothing (`tasks::finished`): the item stays `working`, the message's turn
  runs, and that turn's last message is the item's result and its one report.
- **After it ended.** A done, failed or stopped item whose task bot still exists (it is kept seven days with the
  item) takes a message too. The first one reopens it: `working` again, under Threads, `startedAt` now, counted
  among the bot's running tasks even past `maxAgents` (the user asked), so the dispatcher starts nothing else while
  the bot is over its ceiling, and neither it nor the hourly look ends the task. The task's actor starts again and
  opens its session as an actor does after a restart: `session/load` or `session/resume` where the agent can, so
  the session holds the work; else a new session that is told the item first (`prompts::picked_up`: title, brief,
  the user's words and notes, what it reported). A worktree that was kept is its folder again; one that was
  removed leaves the task in the bot's folder. When that turn ends the item ends again and reports again. A
  restart or the hourly look that cuts a reopened item's turn off, where the session can't be picked up, ends it
  `failed` (`restarted` is set as it reopens): the work was done once and isn't started again from its brief.
- **What the task hears.** The user's words as written, then one line saying the user wrote them in the work's
  thread and that the turn's last message is again the report (`prompts::in_thread`), also when Send now hands them
  into the running turn. What a task may not call it
  still may not (`add_work`, the `routines`, `team` and `manage` tools), and it still only adds to memory.
- **The bot hears of it.** A report ends with what the user wrote to the thread since the item's last report
  (`Report.told`): the newest five messages, each cut at 280 characters, in a line that says they are the user's own
  words and not the task's. Only what reached the task counts: a message taken back, dropped by Stop or failed by
  a restart isn't there. Nothing is added when the user wrote nothing. The same words join the item's `notes`, so
  a session that starts the item again reads them.
- **Read-only threads.** A routine's run and another bot's request refuse a message (`"<title>" is a routine's
  run: it can be read and stopped, not written to`, `… is another bot's request: …`): their result has an owner
  already. An item that hasn't started has no thread yet (`"<title>" hasn't started yet: there is nothing to
  write to`), and one whose task is gone can't be picked up (`"<title>" ended too long ago to write to: add new
  work instead`). `work[].talk` says which items take a message, so no client guesses.
- **What the task shares stays in the chat.** Its cards, files, voice notes and artifacts are the chat's entries
  ([below](#asking-and-sharing-from-a-task)), answered, downloaded and opened there. Its thread shows each, in time
  order among the task's own entries, as one quiet line with a symbol (`sharedLine` in every client, the
  terminal's `shared_line`): `Waiting for your approval: <title>` or `Asked: <question>` while it waits,
  `Approved: <title>`, `Denied: <title>` or `Answered: <choice>` once answered, `Not answered: <…>` when it
  expired, `Sent a file: <name>`, `Sent a voice note`, `Made an artifact: <title>`; its agent's
  [sign-in](#fallbacks-and-sign-in) card reads `Needs <agent> sign-in`, then `Signed in` (or `Cancelled`). The
  line opens the chat at that entry; the thread has no card to answer.

With the rest:

| What happens | A message in the thread |
| --- | --- |
| The task waits on a card (`needsYou`) | It waits behind the turn the card holds, and neither answers the card nor ends it: only a press does |
| **Stop** on the item | The messages that waited are dropped (`cancelled`, **Not sent — stopped** in a client), as a chat's Stop drops its queue |
| The item gives way (cut in) or gets a new brief | Its task is closed; what waited and no turn took goes with the item as `notes`, so the session that starts it again reads it. The entries end `cancelled` |
| Host restart | A waiting message is `failed`, as in a chat; the cut-off turn is picked up without it. The thread takes a new message afterwards |
| The item's task is on an item that followed it (`after`) | The message goes to that session and waits behind its turn; the ended item isn't reopened |

## How a task's session begins

- **Fork**: `session/fork` of the bot's main session, sent from the task's own process with the main session's
  frozen instructions, so the prompt cache is the chat's. Conditions: the agent advertises
  `sessionCapabilities.fork`, the bot is on its own agent (not a fallback), the main session has used under half
  its window, and the task starts on the bot's own agent too (a task whose first agent rests starts on a
  [fallback](#fallbacks-and-sign-in), which can't fork another agent's session).
- **Brief**: `session/new`, with the chat so far before the task's first message (`handoff::build`: summary, last
  requests, last reply, plan). Used when a condition fails or the fork errors, and for a task that works in a
  [worktree](#worktrees) (a fork would be taken in another folder than its session's).
- **A routine's run**: `session/new` with the run's own prompt and nothing of the chat, as a run in the chat has:
  no fork and no chat so far, so it costs and sees what it did before runs were items (`sources::from_chat`).

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
stopped reports nothing. Reports are `from: host`: memory and dreaming don't take them as the user's words. A
report of an item that worked in a [worktree](#worktrees) says where its changes are (the branch and the folder),
or that the worktree had none and was removed, and ends with what the user wrote to its
[thread](#talking-to-a-thread) meanwhile.

An item that doesn't finish reports why, the same way: its task's agent couldn't start (`it could not start: …`),
failed mid-turn, or ended short of `end_turn`. Every way a task's turn ends goes through `Actor::turn_over`, so no
item is left working with nothing behind it.

## Approvals

A task's permission request becomes a card in **its bot's chat**, with `data.task = {id, title}` (the task bot's id,
its item's title), `data.item` (the item's id) and the bot as `data.author`, so `respondPermission` reaches the
task's actor and a client can link the card to the item. The approval setting is the bot's (copied when the task
starts). A waiting card stops only its task; the chat goes on.

## Fallbacks and sign-in

A task runs its bot's [fallback chain](agent-fallback.md) with the same actor code as the chat: when its agent runs
out of usage, is rate limited, signed out or won't start, the same turn goes on with the next agent. Rests are the
computer's, so the chat and every task skip a resting login together.

- **The chain is the bot's.** A task copies it when it starts; a change to the bot's fallbacks reaches its tasks at
  once (`tasks::fallbacks_changed`, from `Hub::update_bot`), for their next agent.
- **A new session on another agent hears the chat.** A task forked from the chat, or told it, keeps that only in
  the session it began; when it falls back to a new session elsewhere, that agent is told the chat so far
  (`tasks::chat_so_far`) before the recap of the task's own transcript. Not for a routine's run.
- **What the user must do is asked in the bot's chat** (`agent::bot::sign_in`). A task's agent sign-in card goes to
  its bot's chat with `data.task`, `data.item` and the bot as `data.author`, and `connectionRequest.botId` the bot's
  (the apps check it against the chat). The host keeps one card per login per task (`data.task.id`), apart from
  the chat's own; the task's next item takes it over. The task's own transcript gets one line, `Asked in the chat to sign in to <agent>.`. A failure
  that offers **Add a fallback** goes to the chat the same way; the transcript keeps the failure.
- **An item waits for the sign-in.** When no agent of the chain can take the turn and the card holds the item's
  message, the item doesn't fail: it stays working on its task, reads `needsYou` with the activity
  `Needs <agent> sign-in`, and a needs-you alert goes to the bot's chat. Once that login is signed in, the held
  message goes out again to the same task, however long it waited (`agent_sign_in::send_again` sends each held
  message to the chat it was written in; only the chat's own expire after a day), and the item goes on in its
  session. A message that went out again at once (a process older than the turn) keeps the item going too. A task
  whose item was stopped or went back to the box gets nothing sent; its Stop ends the wait, and the task.
- **A sign-in keeps the session.** A refused sign-in starts a new agent process before the next turn (the old one
  holds the old credentials), as it does for the chat: an agent that can reopen its session goes on in it
  (`restart_before_turn`), so a task keeps its fork or brief and what it did; one that can't starts a new session,
  told the conversation so far as after a move to another agent (`owe_recap`; a task's, the chat it comes from too).
  With no process left (it quit, or the new one couldn't start), the next turn starts one and reopens the session,
  as after any exit.
- **The usage gate looks past a resting agent.** The dispatcher's "near its usage limit" reads the agent a new task
  would start on (`fallback::starting`: the first step that isn't resting), not only the bot's own.

The apps show the item line on the sign-in card and on the failure, and a needs-you row reads the card's ask
(`Needs <agent> sign-in`) and opens the chat at the card, as for an approval. When the turn left its agent signed
out and a fallback was signed out too, there are two cards: the row names, and opens, the one that holds the item's
message (`connectionRequest.entryIds`), whose sign-in is what sends the work again; an approval or choice newer than
it comes first, as what the running turn waits on. In the item's
[thread](#talking-to-a-thread) the sign-in card is one quiet line beside that transcript line, as the card stands
(`Needs <agent> sign-in`, then `Signed in` or `Cancelled`), which opens the chat at the card; the failure that offers
**Add a fallback** has no line there, since the transcript holds the failure itself.

## Asking and sharing from a task

A task on the bot's own work reaches the user through the bot's chat, as its approval card does. What it posts there
is the bot's (`data.author`), numbered with the chat's latest turn, and carries `data.task = {id, title}` and
`data.item`, so a client shows the same `In the background · <title>` line and can open the item
(`Actor::add_for_user`).

- **`request_decision`**: the choice card is posted in the bot's chat, the item reads `needsYou` while it is open,
  the alert (*needs you*) opens the bot's chat, and a press (`respondDecision`) goes to the task (`tasks::card_actor`).
  The user's choice is recorded as their message in the chat, under the card.
- **`send_file`** (a voice note too): the file message is the bot's in the chat. The copy is kept in the bot's files,
  so it outlives the task.
- **`publish_artifact`, `list_artifacts`**: the artifact is the bot's (its Library), and its card is in the chat.

A card that belongs to a task, case by case:

| What happens | A chat's own card | A task's card |
| --- | --- | --- |
| The user types a message | It answers the card, or ends it (`other_message`) | Nothing: in the chat the message is for the bot, and in the task's [thread](#talking-to-a-thread) it waits behind the card's turn. Only a press answers, so the card's text doesn't ask for a typed reply |
| Stop in the chat | Ends it (`stopped`) | Nothing: the chat's Stop is the chat's turn. Stopping the item (`stopWork`, the bot's `steer_work stop`) ends it |
| The item gives way or gets a new brief | | Ends it (`stopped`): its task is closed |
| The turn ends | Ends it (`turn_ended`) | The task's turn, not the chat's: a card the task left open ends with the item |
| Four and a half minutes | Ends it (`timeout`), the call returns | The same; the task goes on without an answer |
| Host restart | Ends it (`restart`) | The same: the resumed task has no call waiting |
| One at a time | One card per bot | One card per task; two tasks may each have one open in the chat |
| Alert | *needs you*, the bot's chat | The same, named for the bot |

A permission card follows the same rows, except that a typed message never touched it. An item waiting on a card is
never taken for lost by the hourly look: its turn runs.

## Other sources

A run of one of the bot's routines and another bot's `message_bot` request are items in the bot's box
(`sources.rs`), so the chat stays free while they work and they wait for a place under `maxAgents` like anything
else. An item's `source` says which: `routine` with the routine's id, or `request` with the asking bot's id (none
when it was asked from another computer or by the cloud assistant). A request starts like any item (a fork of the
chat's session where the agent can, else a new session told the chat so far); a routine's run starts in a new
session that knows only its own prompt, as it did in the chat. Their task is told the routine's own
prompt or the request as it is, not an item's brief, and **their result takes no turn of the bot's**:

- **A routine's run** ([routines](routines.md#execution)): when a task takes it, the run begins as it did in the
  chat (the latest instruction and progress note, the `Routine · <name>` line). Its last message is posted in the
  chat as the bot's message, in the turn that line took, and the run's outcome is saved: the alert, `(pass)`, the
  failure line, the retries after 1, 5 and 15 minutes and the Try again button are the routine's as before. The
  routine's time limit stops its task, its session's setup included, and only while the run is what the task works
  on (an item that followed the run into its task has no limit). The item's title is the routine's name.
- **A request** ([bot collaboration](bot-collaboration.md#how-a-message-runs)): the exchange's notices read
  `Running…` when a task takes it, and the item's last message is the reply the asking bot gets, with the same
  hop count, as it is: a reply of exactly `(pass)` goes back as those words, and a turn that ends with no text is
  `recipient finished without a text reply`, both as from a turn in the chat. Its title is the request's first line.

What such a turn could and couldn't do in the chat holds in its task (`Actor::task_source`): `request_decision`,
`send_message`, `send_file` and artifact cards are refused with the same words; the `routines` tools work, as the
bot (a run saves its progress and sets follow-ups, an asked bot runs its own routine); `message_bot` works from a
run and is refused in a request; `follow_up` is refused in a request. `add_work` is refused as in any task. A
request from another computer under a network rule's `auto` or `ask` holds its task to that
[ceiling](codync-network.md#ceilings-auto-and-ask) (`tasks::access_of`), as it held the turn in the chat.

What differs from the bot's own items:

- **Stop** cancels the run (`Stopped by user`, no failure line) or tells the asking bot its request was stopped.
  **Drop** on a waiting one does the same.
- **Try again** on a routine's item runs the routine again: a new run (with the failed run's event when it kept
  one, else as `runRoutine` does) and a new item in the old one's place. A request can't be tried again: the bot
  that asked has its answer.
- **They never give way** to an item that cuts in, and `steer_work replace` refuses them: started again they could
  act twice.
- **They never start twice.** Cut off by a host restart with a task that can't resume, or lost (the hourly look), a
  run ends `interrupted` at once and its routine's retry rules decide, and a request fails. A request lives in the
  host process (its notices are marked interrupted at start, as before), so its item leaves the box at host start.
- A run whose routine was paused or deleted while it waited leaves the box.
- Stop in the chat no longer cancels a routine's run that waits in the box, nor a request: the box's Stop does.

**Kept as turns in the chat**: a bot with **computer use** or **Manage the team** on takes its routine runs and
requests in its chat as before (`tasks::boxed`), because a task has neither the `computer` nor the `manage` server
(one screen and one team stay with the chat), so a run that drives the screen or sets up a bot would stop working.
Rooms keep their own path.

## Memory

A task's writes to memory are its own turn's (`memory::turns`, keyed by the actor: the bot, or one of its tasks),
credited to its bot and made in the task's session, for the bot's own items, a routine's run and a request alike:

- **`edit_memory`** from a task is a call of the task's own `memory` server. A task's turn is never the user's own
  turn in the bot's chat, not even one their message to its thread started, so it may add lines and never change or
  take one out, whatever the chat is doing meanwhile (before, it borrowed the chat's running turn, and could take a
  line out while the user's own turn ran there).
- **File tools and shell commands** in a task are checked at the end of the task's turn, as a chat turn's are: a
  change that may not stand (a line taken out) is put back, and the task's session hears why. A line it added is
  recorded as the bot's.
- The task's session knows what it wrote itself; the chat's hears of the change with its next message.

## Worktrees

Two tasks in one git repository would write over each other's files. With **`taskWorktrees`** on (per bot, on
unless set; it matters only where the bot's folder is in a git repository, `gitRepo` on the wire), a task for one
of the bot's own items that starts **while another task of the bot is working** gets a git worktree of its own
(`worktree.rs`):

- `~/.codync/worktrees/<bot id>/<item id>`, on a new branch `codync/<title in lowercase, dashes>` (`-2`, `-3`… when
  taken), from the commit the bot's folder is on. The task's agent starts there (in the same place of the
  repository as the bot's folder, when that is a folder inside it), is told so with its item, and starts from a
  brief, not a fork. Uncommitted changes of the bot's folder aren't in it.
- **The first task alone keeps the bot's folder**; a routine's run and a request never get a worktree; an item that
  follows another (`after`) goes on in that task's folder, worktree or not.
- It is made by the task itself as its first turn starts. When git fails (no commit yet, git missing, the folder in
  the way, a checkout hook that fails) or the checkout lacks the bot's folder (git doesn't track it: ignored, or
  never added), the task works in the bot's folder as before, and the failure is logged. A worktree git made and
  the task can't use is taken back at once, with its branch (`git worktree remove --force`, `git branch -D`):
  nothing has worked there.
- **When the item is over** and nothing waits to follow it (an item the hourly look or a host restart ended
  included): a worktree with nothing in `git status --porcelain --untracked-files=all` (untracked files count
  whatever the user's `status.showUntrackedFiles` says), on
  its own branch, with that branch still at the commit it started from, is removed (`git worktree remove`, then
  `git branch -d`, neither forced), and the report says so. Anything else is kept, with its branch, and the
  item's report to the chat names the branch and the folder, for the bot to tell the user and merge when asked.
  An item the user stopped is looked over once its task's turn has ended; it reports nothing.
- An item that gave way, got a new brief or is tried again starts again in the worktree it had. While an ended
  item's worktree is being looked over (`Left::Settling`) the item tried again waits; the look notes kept or removed
  and runs the dispatcher again. A message to the item's thread meanwhile takes the worktree back as in use, which
  the look then leaves as it is.
- **Codync never merges, rebases, resets, forces or deletes a branch that has work on it**, and never touches a
  worktree it didn't make: what a box says is checked first (the folder is `~/.codync/worktrees/<id>/<id>`, the
  branch `codync/…`). Git runs with an argument list, never through a shell, each command for at most a minute.
- Dev servers, `send_file` and `publish_artifact` of a task in a worktree start from the worktree.

## Outside the chat

What shows a bot away from its chat (the iPhone's Live Activity, Android's ongoing notification, both fed by
`push::live_activity_update`) reads the bot through `Hub::outside_runtime` (`tasks::outside`): it **needs you**
while the chat or one of its items waits on the user; it **works** while the chat works or its box holds anything
that isn't over (an item working or waiting, or one whose report the chat hasn't taken yet); otherwise it is the
chat's own status. An update goes out when that status changes (`Hub::outside_changed`: after the chat's or a
task's status changed, and after each dispatcher pass), so an activity doesn't end when the chat's turn does while
items work, and ends once the box is empty and the chat idle. The push carries the status and the start time only,
as before: the line (`Needs you`, the chat's activity, `2 working · 3 waiting`) is each client's, from `bot.work`.
The same status is on the wire as `bot.outside = {status, startedAt}`, beside the chat's own `status`, for what a
client keeps up to date itself (the activity while the app runs, the tray, the island).
An item's end sends no alert of its own: its report is a turn of the bot's, whose reply alerts as any reply does.

## Wire

- Bot: `work: [item]`: every working item (oldest first), then the waiting in dispatch order, then the items ended
  in the last 24 hours (newest first, at most 20). Item: `{id, title, status, activity, task, wait, cutIn,
  createdAt, startedAt, endedAt, result, talk, folder?, source?, worktree?}`; `activity` is its task's, `task` the task bot that has or had it, `talk` whether the user may write to its [thread](#talking-to-a-thread) (the bot's own item whose task bot is there), `folder` where its task works (the worktree's folder for one in a worktree, else the bot's folder as it was when the task began; absent before a task took it, and once its worktree was removed with nothing in it), `wait`
  is `{kind: next | behind | after | usage, ahead?}` on a waiting item and null otherwise, `result` at most 280
  characters. `source` is `{kind: routine, id: <routine id>}` or `{kind: request, id?: <asking bot id>}`, and absent
  on an item the bot added, so a client that doesn't know it shows the row as before. `worktree` is
  `{branch, path}` while the item has a [worktree](#worktrees) that is still there, and absent otherwise. The bot's own `status` stays the chat's.
- Cards and messages a task posts in the chat carry `data.task = {id, title}` and `data.item`: permission cards,
  choice cards (`data.decision`), agent sign-in cards (`data.connectionRequest`, `kind: agent`), failure notices that
  offer **Add a fallback** (`data.addFallback`, without the `stopReason` of the task's own), file messages
  (`data.files`) and artifact cards (`data.artifact`).
- Bot: `maxAgents`, set with `updateBot` (1 to 6).
- Bot: `outside: {status, startedAt}`: how the bot stands [outside its chat](#outside-the-chat) (`idle`, `working`,
  `needsInput`, `error`), where `status` is the chat's alone. Absent on an older host: `status` then.
- Bot: `taskWorktrees` (true unless set; `updateBot`), and `gitRepo`: whether the bot's folder is in a git
  repository, which is where the setting matters and a client shows its switch. Both absent on an older host.
- Bot: `gitInstalled`: whether the host can run git (`git::installed`: the answer of `git::program`, kept for the
  process and looked up again in the background at most every five minutes, so a snapshot never waits for it; true
  until the first look ends, which the host starts as it starts). Absent on an older host: read as installed. With
  `gitRepo` it gives the line under the folder in Settings: `Git repository`, `Not a git repository`, `Git isn't
  installed on this computer`. The manual's page for it: `/docs/working-folders` (`#without-git`).
- `taskEntry` events `{rev, task, entry}` for a task's transcript; none in the catch-up.
- `taskEntries {botId, taskId}` → `{task: {id, title, status, startedAt, endedAt, start}, entries}` (newest 500):
  `taskId` is an item's `task`; `title`, `status` and the times are of the item it works on, else the last it
  worked on. A user entry there is what the task was told (`data.from: "host"`: an item, as its task read it) or the
  user's own message to the thread (no `from`; `data.status` `queued`, `sent`, `cancelled` or `failed`, as in a chat).
- `send {botId, itemId, text, clientNonce?}` → `{entry, reopened, queued}`, `steer {botId, itemId, entryId}`,
  `unqueue {botId, itemId, entryId}`: [talking to a thread](#talking-to-a-thread).
- `workFirst {botId, itemId}`, `workCutIn {botId, itemId, insteadOf?}`, `dropWork {botId, itemId}`,
  `stopWork {botId, itemId?}`, `retryWork {botId, itemId}`.

## In the apps

The capsule, the bot's Details (Threads, Event box, Later, Routines, Dev servers, Recent), `Agents at once` in its
Settings and cutting in are specified in [the event box](../plans/event-box.md#in-the-apps) (stage 2). The sheet of every bot's work, the rows' source marks, the `In the background · <title>` line on what a task posts, the **Worktree per agent** switch and what follows `bot.outside` are in [as built, clients](../plans/event-box.md#as-built-clients-stages-3-to-8). An item's thread view, the where line on a row and in the thread's header, and the git state in Settings are in [stage 11, as built on the terminal](../plans/event-box.md#as-built-host-and-terminal-stage-11) and [on iPhone, Android and desktop](../plans/event-box.md#as-built-iphone-android-and-desktop-stage-11). A client reads `bot.work` and `bot.maxAgents`; both are
absent on an older host, where the feature doesn't show.

## Restart and cleanup

At host start (`tasks::restore`): items ended over 7 days ago are deleted; a working item whose task's turn was cut
off and can be resumed (`agent::bot::resumable`: a restart mark under an hour old) stays working, and its task's
actor resumes the turn; one whose task can't goes back to waiting once, as an item that was cut in on, and fails with
a report the second time. An ended item's undelivered report is sent again: to the bot, or, for a routine's run,
as its message in the chat (posted once, by its entry id) and its saved outcome. A run the box no longer holds is
queued again if it waited, and ends `interrupted` if it ran. Task bots whose bot is gone, or that no
item names, are deleted. Deleting a bot deletes its box and its tasks, and cancels the requests it hadn't answered.

The hourly look (`tasks/sweep.rs`) catches what no event said. After every dispatcher pass that leaves waiting or
working items in a box, one timer per bot is set for an hour later (reset by the next pass, none for an empty box).
When it fires, the host takes a dispatcher pass again, and a working item whose task runs no turn (its actor is
gone or idle, and nothing is about to resume) follows the restart rule: back to waiting once, then failed with a
report (`work_finished` `error: lost`). A turn that runs is never touched, however long and quiet. The look is host
code only: no agent is woken unless an item was lost.

## Limits of this version

- A task never changes or takes out a line of memory, the bot's own items included, also in a turn the user's
  message to its thread started. What the user asks the bot to forget, the bot does in the chat.
- A thread takes text only: no files, no quoted message, no voice. A message sent right as a host restart comes is
  marked failed and has to be sent again.
- A turn that ended with a message of the user's still waiting reports nothing of its own: its last message stays
  in the transcript, and the report is the last turn's.
- No subagent rows.
- A task in a worktree starts from a brief, not a fork (no agent is checked to honor another folder on a fork).
- A worktree is a new checkout: what git ignores in the bot's folder (installed packages, build output, local env
  files) isn't in it, and removing an untouched one deletes what its task left there ignored.
- On Windows a worktree whose agent process is still in it can't be removed and stays (kept). A host that stops
  while it looks a worktree over leaves it, marked kept at its next start (`worktree::unsettled`).
- A turn of the chat that ends while items still work alerts as done with the bot's words; the Live Activity goes
  on.
- A Live Activity or ongoing notification updated by a push shows the status only; the counts line needs the app
  (the relay passes no more than the status and a date).
- What a task's agent runs inside its own process (a harness's background agents and shells in the agent's session)
  ends when the task does: its process and that session's processes are stopped (`Acp::kill`). A program it
  detached into a session of its own (a server from its shell) isn't reached and keeps running; one left listening
  shows among the bot's [dev servers](dev-servers.md).
- A task ends as soon as its item does when nothing waits to follow it, so `after` is for work that is still
  working or waiting.
- A routine's run in a task has its `send_message` refused as before, and what it wrote reaches the chat when it
  ends.
- A task on a routine's run or a request can't change the bot's profile or fallbacks, which that turn could in the chat.
- A typed message never answers a task's choice card; a client without buttons can't answer one.
- A thread's line for what its task shared finds that entry only among the chat's messages the client has loaded,
  as a needs-you row does; an older one shows no line, and the line's tap leaves the chat where it was.

Tests: `host/tests/tasks_e2e.rs` with `tests/fixtures/task_agent.py` (fork, brief, a card with `data.item` answered
from the chat, refusals in a task from each server, the ceiling and the order, first, drop, cut in from the user and
from the bot, replace on a waiting and on a working item, `after` in the same session, stop one and all, retry, an
agent that can't start, a stopped turn dropping the item it added, a deleted bot taking its box and tasks, a host
restart mid-item, cut in on an item that follows working one closing nothing, no `bot` or `entry` event for a task
bot; a routine's run as an item: its source on the wire, waiting for a place, its message posted once in its line's
turn with no turn of the bot's, `(pass)`, its progress note, the refusals of a run, stop, a deleted routine's waiting
run, try again, never giving way, an agent that can't start, a host restart mid-run; another bot's request as an
item: its source, the notices, the reply turn of the asking bot, the refusals of a request, stop, no try again; a
task's choice card answered by a press and not by a typed message, ended by stopping its item and not by the chat's
Stop, its file and artifact in the chat and kept with the bot; a run that never forks nor sees the chat, a request's
`(pass)`; a task's `edit_memory` adding and refused a removal while the user's own turn runs in the chat; a task
beside another in a repository working in its worktree, an untouched one removed with its branch, a touched one
kept and named in the report, the switch off and a folder that is no repository; a message to a working thread
queued, sent twice as one, removed, delivered in the task's session and in the item's one report, the refusals of a
task's own id, another bot's item, a waiting item and a routine's run; Send now stopping the turn for the message
and joining it where the agent steers; a message to an ended item reopening it past the ceiling, `add_work` still
refused there, its second report, and a new session told the work where the agent can't load one; a message
waiting behind a thread's card without answering it; Stop dropping what waited; a host restart failing it and the
thread taking a message afterwards; what waited going with an item that gives way; a task's sign-in asked in the
chat and its item going on in the same task once signed in, a task moving past a signed-out agent and its next
agent told the chat, a task's offer to add a fallback in the chat and new fallbacks reaching a running task, a task
that starts on a fallback never forking the chat's session); `memory_files_e2e` (a line a
routine's run in a task takes out with a shell command is put back); unit tests of a worktree made, removed, kept
and never touched when it isn't Codync's (`worktree.rs`), how a bot stands outside its chat (`items.rs`), unit tests of the dispatch order and the `wait` reasons
(`dispatch.rs`), the agent a new task starts on (`agent/fallback/mod.rs`), a task's held message sent again however
old and not once its item ended (`market/agent_sign_in.rs`), an item's end and the restart rule, also for a run and a request (`mod.rs`), a source on the wire (`items.rs`), which bots keep runs in the chat (`sources.rs`), the 20 waiting items and the title and brief lengths (`tools.rs`), the prompts and the box's lines. Not
covered by a test: a reopened item's kept worktree becoming its folder again and a removed one's task moving to the bot's folder end to end (the unit test covers the folder), a message to an item whose task is on an item that followed it, a message that arrives as the item's turn ends (the lock that orders them), a real agent resuming a reopened session, a replaced brief carrying what waited, a worktree reused by an item that starts again, one inherited by an item that follows, a
stopped item's worktree looked over, an item stopped while it waits for a sign-in ending its task, a worktree's dev server and artifact, git failing, a real agent in a worktree,
the Live Activity push itself (the relay isn't in the tests), a routine's time limit in a task, a lost run or request at the hourly look, a run the box lost at host start, a request from another computer or the cloud assistant as an item, `message_bot` from a run's task, a task's voice note, a choice card's timeout and its end at a host restart, the usage and in-a-row limits, the desktop's Add a fallback opening Settings, `add_work` refused in a room or a bot's request (a routine run is: `routines/tests.rs`), a report turn that fails, a
task's dev server, an item sent back to waiting by a restart end to end, the 7-day purge, and the box's lines in a
known harness's prompt.
