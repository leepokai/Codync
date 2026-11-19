# Work: tasks and threads

A bot puts long work in its **event box** as **tasks**. **Threads** of the same bot take them: away
from the chat, each with its own agent process and session, so the chat keeps answering. The chat shows only messages and what needs the user (a thread's approval
and choice cards, the sign-in card its agent needs, its offer to add a fallback, the files and artifacts it shares);
the work shows as one capsule above the composer, and each
task's report comes back as the bot's own message. A run of one of the bot's routines and another bot's request are
tasks too ([other sources](#other-sources)).

| | The chat | A thread |
|---|---|---|
| Session | The bot's main one | Its own, forked from the main one where the agent can (a routine's run: a new one) |
| Agent process | The bot's | Its own |
| Transcript | The chat | Its own, read from the task's details |
| Who talks to it | The user | It gets tasks, one at a time; the user may write to the [thread](#talking-to-a-thread) of the bot's own task |
| Its result | A message | Its task's report, which the bot turns into a message |

Design: [the event box](../plans/event-box.md); why a thread is a bot of its own, and the fork spikes:
[the first design](../archive/background-tasks-plan-2026-11-15.md). Code: `host/src/chat/work/` (`tasks.rs`: the box, its wire shape and what the bot reads of it;
`dispatch.rs`: which task a thread takes next; `steer.rs`: first, cut in, replace, stop, drop, retry; `mod.rs`: a
thread's turn end, the report, host start; `sources.rs`: routine runs and other bots' requests as tasks; `thread.rs`: the user's messages to a task's thread;
`tools.rs`: the bot's tools and what a thread may not call; `sweep.rs`: the hourly look; `worktree.rs`: a thread's git worktree;
`prompts.rs`: the words); the actor's side is in `host/src/agent/bot/` (`session.rs` for the fork, `turn.rs` for the box in the prompt, the worktree
and the report, `queue.rs` for `turn_over`, `updates.rs` for cards). Git is run through `host/src/git.rs`.

## A task

One piece of work: `id, title, brief, source` (where it came from: the bot, a routine's run, another bot's request),
`words` (the user's own, word for word), `notes` (what the user said about it
since), `status, order, after, thread, cutIn, hops, createdAt, startedAt, endedAt, result, report, failure, worktree,
told` (what the user wrote to its thread since its last report), `said` (what it reported before the user wrote to
it again). A bot's tasks are one
JSON list in kv `work.<bot>`; every change to a box is made under one lock (`tasks::change`).

| Status | Meaning |
| --- | --- |
| `waiting` | In the box; no thread works on it |
| `working` | A thread works on it |
| `needsYou` | Its thread waits on an approval, a choice or its agent's [sign-in](#fallbacks-and-sign-in) (never saved: a working task reads so while its thread's card is open) |
| `done` | Finished; `result` is what its thread reported |
| `failed` | It could not be finished; `result` says why |
| `stopped` | The user or the bot stopped it |

## A thread is a bot of its own inside the host

Everything a running turn touches is keyed by bot id, so a thread is a **thread bot**: a `bots` row with
`threadOf = <bot id>` and id `thread-<uuid>`, its config copied from its bot when it starts (without `computer` and
`manage`). It has its own actor, agent process, session, transcript, restart mark, fallback state and compaction
record. Its memory and instruction snapshot are its bot's (`BotConfig::as_owner`); its `memory` MCP server is its
own, and reads and writes its bot's memory as the thread's own turn ([memory](#memory)).

No client lists a thread bot. `Store::bots()` leaves them out, `emit_bot` for one emits its bot instead, and its
entries leave as `threadEntry` events, not `entry`.

A thread works one task per turn. When the turn ends the task is done and reported (unless a message of the user's
waits in its [thread](#talking-to-a-thread): that message's turn is the task's end); a thread with no task left ends (its
actor and agent process stop; its transcript stays for its tasks' details, and a message to one of them starts it again). A thread bot no task names any more is
deleted with its transcript.

## The dispatcher

The user sets how many threads a bot runs at once, `maxThreads` (1 to 6, 3 unless changed, `updateBot`). It is
the only ceiling: the computer as a whole has none. `work::dispatch(hub, bot)` runs whenever the box or one of its
threads changes: a task added, moved, dropped, stopped or retried; a thread's turn over; the bot's own turn over; `updateBot`; host start. For each waiting
task:

1. **It follows a task** (`after`) that is done and whose thread is still there and idle: that thread takes it, in the
   same session. These go first, since that thread already runs. While what it follows waits or works, it
   waits (`wait: after`). When what it follows failed, was stopped or dropped, or its thread is gone, it is an ordinary
   task.
2. **Otherwise a new thread starts for it**, lowest `order` first, when the bot is under `maxThreads`, the bot's own turn is over (a fork taken mid-turn would carry an unanswered tool call), and the agent is
   not near its usage limit (near it, one thread at a time: `wait: usage`).
3. **Otherwise it waits**: `wait: next` for the first in line, `behind` with how many are `ahead` for the rest.
   Nothing is refused for being over the ceiling.

A new thread waits for any turn of the bot to end, a routine run's or a room's too (they share its actor); that turn's
end looks over the box again.

The decisions are one pure function (`dispatch::standings`), which the wire's `wait` reads too.

## Tools (on the `chat` MCP server)

- `add_task {title, brief, after?}` → `{taskId, status, note}`: puts a task in the box and answers at once. Refused
  outside the bot's own chat with the user (a room, another bot's request, a routine run: `add_task works only in
  your own chat with the user`), in a thread, past 20 waiting tasks, when `after` names
  no task that is working or waiting, and for the ninth task added in a row with no message from the user (a task
  added from a report's turn carries that report's hops; the user gets a Continue notice, as at the
  [handoff limit](bot-collaboration.md#limits)). Stopping the turn that added a task drops it. Refused
  too in a turn a network rule holds to a ceiling, such as one that reads another computer's answer
  ([ceilings](codync-network.md#ceilings-auto-and-ask)): its work would run without it.
- `steer_task {task, how, brief?, insteadOf?}` → `{taskId, status}`: `first` (a waiting task moves to the front), `now` (it cuts in),
  `replace` (a new brief), `stop`. The user's own words of the running turn are added to the task's `notes`.
  Refused under a ceiling, as `add_task` is.
- `work_status {}` → `{tasks: […]}`: every task that is working or waiting, and those ended in the last day: status, what its thread
  does now, why it waits, seconds, result.

Refused in a thread: these three and changes to the bot's profile or fallbacks (`work::refuse_chat_only`, the `chat`
server's), and the `routines`, `team` and `manage` tools (`work::acting_as`, `work::refuse_in_thread`). The refusal
says to put what it needs in its report. `request_decision`, `send_file` and artifacts work from a thread
([below](#asking-and-sharing-from-a-thread)). A thread on a routine's run or another bot's request has that turn's own
rules instead ([other sources](#other-sources)).

A thread may start [dev servers](dev-servers.md). They are its bot's: listed on the bot, stopped there, counted toward
its limit, and so is what the thread's agent leaves listening through its own shell. The approval card is the thread's,
in the chat like its others.

The bot's instructions carry `work::BRIEF`, and each message in its own chat ends with `work::EACH_TURN`: with the
instructions alone a real agent did 90 seconds of reading in the chat's turn (checked with Claude on 2026-11-15; with
the line it handed the work on, and answered a quick question itself). On a known harness each of those turns also
carries the box in a few lines (`work::lines`: the counts and how many run at once, then each working and waiting
title with its id), so the
bot can say what it is doing and place an `after` without a tool call.

## Steering

- **First** (`workFirst`, `steer_task first`): a waiting task moves to the front. Order only.
- **Cut in** (`workCutIn`, `steer_task now`): the task moves to the front. With every thread busy a working task
  gives way (the one named with `insteadOf`, else the one that started last): its thread is stopped and deleted, and it
  waits at the front, behind the task that cut in, with `cutIn` set. It starts again later in a new thread and session,
  told that the work was begun before. With a thread free, cutting in is moving first; so it is for a task that
  wouldn't start on the freed thread (it follows unfinished work, or waits for usage), and the clients don't offer
  Cut in on such a row.
- **Replace** (`steer_task replace`): the brief is replaced. A working task's thread is closed the same way and the
  task starts again from the new brief, at the front (`cutIn` stays as it was).
- **Stop** (`stopWork`, `steer_task stop`): the task is `stopped`; a working one's thread is stopped. Without a task,
  `stopWork` stops every task that is working or waiting. Tasks that followed a stopped one become ordinary.
- **Drop** (`dropWork`): a waiting or ended task leaves the box. A working one is refused: stop it first.
- **Try again** (`retryWork`): a stopped or failed task goes back to waiting, last, and starts in a new thread.

A thread that gives its task up is told to stop; when its turn has ended, the dispatcher clears it away. Until then it
still counts toward `maxThreads` (`dispatch::capacity` counts a thread whose turn runs with no task at work), so the task that replaces it
waits rather than run beside it. The dispatcher starts a planned thread, and hands work to one, under the box's lock and
only while the task is still on it, so a Stop that came in between wins.

## Talking to a thread

The user writes to the thread that works, or worked, one of the bot's **own** tasks, as to a chat (`thread.rs`). The
thread is a bot with an actor, so the message is a user entry in the thread's own transcript and goes through the
chat's own queue: nothing here is a second copy of it.

- **`send {botId, taskId, text, clientNonce?}`** → `{entry, reopened, queued}`. The caller names the bot and its
  task, never a thread: the task must be in that bot's box, and `send`, `steer` and `unqueue` refuse a thread's own id
  as `botId` (`unknown bot`). Text only (`a thread takes text only` for attachments, `replyTo` or `call`). `entry`
  is the message as the transcript holds it (`botId` is the thread's id); the same `clientNonce` again answers the
  same entry, with `reopened` and `queued` null.
- **While the thread's turn runs** the entry is `status: queued`, as in a chat, and leaves as `threadEntry` events like
  the rest of the transcript. **`steer {botId, taskId, entryId}`** sends it now: into the running turn where the
  agent can (`_session/steering`), else the turn is stopped for it. **`unqueue {botId, taskId, entryId}`** takes it
  back (`status: cancelled`, `removed: true`; `threadEntries` no longer lists it). With no turn running the message
  starts one. The statuses are the chat's: `queued`, `sent`, `cancelled`, `failed`.
- **The task isn't over while a message waits.** A turn that ends (done, failed, or stopped for Send now) with a
  message of the user's still queued ends nothing (`work::finished`): the task stays `working`, the message's turn
  runs, and that turn's last message is the task's result and its one report.
- **After it ended.** A done, failed or stopped task whose thread bot still exists (it is kept seven days with the
  task) takes a message too. The first one reopens it: `working` again, under Threads, `startedAt` now, counted
  among the bot's running threads even past `maxThreads` (the user asked), so the dispatcher starts nothing else while
  the bot is over its ceiling, and neither it nor the hourly look ends the thread. The thread's actor starts again and
  opens its session as an actor does after a restart: `session/load` or `session/resume` where the agent can, so
  the session holds the work; else a new session that is told the task first (`prompts::picked_up`: title, brief,
  the user's words and notes, what it reported). A worktree that was kept is its folder again; one that was
  removed leaves the thread in the bot's folder. When that turn ends the task ends again and reports again. A
  restart or the hourly look that cuts a reopened task's turn off, where the session can't be picked up, ends it
  `failed` (`restarted` is set as it reopens): the work was done once and isn't started again from its brief.
- **What the thread hears.** The user's words as written, then one line saying the user wrote them in the task's
  thread and that the turn's last message is again the report (`prompts::in_thread`), also when Send now hands them
  into the running turn. What a thread may not call it
  still may not (`add_task`, the `routines`, `team` and `manage` tools), and it still only adds to memory.
- **The bot hears of it.** A report ends with what the user wrote to the thread since the task's last report
  (`Report.told`): the newest five messages, each cut at 280 characters, in a line that says they are the user's own
  words and not the thread's. Only what reached the thread counts: a message taken back, dropped by Stop or failed by
  a restart isn't there. Nothing is added when the user wrote nothing. The same words join the task's `notes`, so
  a session that starts the task again reads them.
- **Read-only threads.** A routine's run and another bot's request refuse a message (`"<title>" is a routine's
  run: it can be read and stopped, not written to`, `… is another bot's request: …`): their result has an owner
  already. A task that hasn't started has no thread yet (`"<title>" hasn't started yet: there is nothing to
  write to`), and one whose thread is gone can't be picked up (`"<title>" ended too long ago to write to: add new
  work instead`). `work[].talk` says which tasks take a message, so no client guesses.
- **What the thread shares stays in the chat.** Its cards, files, voice notes and artifacts are the chat's entries
  ([below](#asking-and-sharing-from-a-thread)), answered, downloaded and opened there. The thread's transcript shows each, in time
  order among the thread's own entries, as one quiet line with a symbol (`sharedLine` in every client, the
  terminal's `shared_line`): `Waiting for your approval: <title>` or `Asked: <question>` while it waits,
  `Approved: <title>`, `Denied: <title>` or `Answered: <choice>` once answered, `Not answered: <…>` when it
  expired, `Sent a file: <name>`, `Sent a voice note`, `Made an artifact: <title>`; its agent's
  [sign-in](#fallbacks-and-sign-in) card reads `Needs <agent> sign-in`, then `Signed in` (or `Cancelled`). The
  line opens the chat at that entry; the thread has no card to answer.

With the rest:

| What happens | A message in the thread |
| --- | --- |
| The thread waits on a card (`needsYou`) | It waits behind the turn the card holds, and neither answers the card nor ends it: only a press does |
| **Stop** on the task | The messages that waited are dropped (`cancelled`, **Not sent — stopped** in a client), as a chat's Stop drops its queue |
| The task gives way (cut in) or gets a new brief | Its thread is closed; what waited and no turn took goes with the task as `notes`, so the session that starts it again reads it. The entries end `cancelled` |
| Host restart | A waiting message is `failed`, as in a chat; the cut-off turn is picked up without it. The thread takes a new message afterwards |
| The task's thread is on a task that followed it (`after`) | The message goes to that session and waits behind its turn; the ended task isn't reopened |

## How a thread's session begins

- **Fork**: `session/fork` of the bot's main session, sent from the thread's own process with the main session's
  frozen instructions, so the prompt cache is the chat's. Conditions: the agent advertises
  `sessionCapabilities.fork`, the bot is on its own agent (not a fallback), the main session has used under half
  its window, and the thread starts on the bot's own agent too (a thread whose first agent rests starts on a
  [fallback](#fallbacks-and-sign-in), which can't fork another agent's session).
- **Brief**: `session/new`, with the chat so far before the thread's first message (`handoff::build`: summary, last
  requests, last reply, plan). Used when a condition fails or the fork errors, and for a thread that works in a
  [worktree](#worktrees) (a fork would be taken in another folder than its session's).
- **A routine's run**: `session/new` with the run's own prompt and nothing of the chat, as a run in the chat has:
  no fork and no chat so far, so it costs and sees what it did before runs were tasks (`sources::from_chat`).

A thread's message for a task says what it is, gives the brief, the user's own words and what they said since, and
how its words reach the user (`prompts::task_prompt`). A task that follows another is the thread's next message in
the session it has.

## Report

When a thread's turn ends, its last message is its task's `result` and goes to its bot as `Cmd::TaskReport`, one per
task, which becomes a turn in the chat (`HostTurn::TaskReport`; reports that arrive together are one turn). The
bot's reply there is an ordinary message with the usual alert; `(pass)` stays silent. A report is kept on the task
until that turn has ended: a host restart before then sends it again (the cut-off report turn isn't resumed). When
the turn ends without the bot's answer (stopped, failed, or it never started), the chat shows the report as a notice
(`"<title>" finished in the background: …`), so what the thread found isn't lost with the turn. A task the user
stopped reports nothing. Reports are `from: host`: memory and dreaming don't take them as the user's words. A
report of a task that worked in a [worktree](#worktrees) says where its changes are (the branch and the folder),
or that the worktree had none and was removed, and ends with what the user wrote to its
[thread](#talking-to-a-thread) meanwhile.

A task that doesn't finish reports why, the same way: its thread's agent couldn't start (`it could not start: …`),
failed mid-turn, or ended short of `end_turn`. Every way a thread's turn ends goes through `Actor::turn_over`, so no
task is left working with nothing behind it.

## Approvals

A thread's permission request becomes a card in **its bot's chat**, with `data.thread = {id, title}` (the thread bot's id,
its task's title), `data.task` (the task's id) and the bot as `data.author`, so `respondPermission` reaches the
thread's actor and a client can link the card to the task. The approval setting is the bot's (copied when the thread
starts). A waiting card stops only its thread; the chat goes on.

## Fallbacks and sign-in

A thread runs its bot's [fallback chain](agent-fallback.md) with the same actor code as the chat: when its agent runs
out of usage, is rate limited, signed out or won't start, the same turn goes on with the next agent. Rests are the
computer's, so the chat and every thread skip a resting login together.

- **The chain is the bot's.** A thread copies it when it starts; a change to the bot's fallbacks reaches its threads at
  once (`work::fallbacks_changed`, from `Hub::update_bot`), for their next agent.
- **A new session on another agent hears the chat.** A thread forked from the chat, or told it, keeps that only in
  the session it began; when it falls back to a new session elsewhere, that agent is told the chat so far
  (`work::chat_so_far`) before the recap of the thread's own transcript. Not for a routine's run.
- **What the user must do is asked in the bot's chat** (`agent::bot::sign_in`). A thread's agent sign-in card goes to
  its bot's chat with `data.thread`, `data.task` and the bot as `data.author`, and `connectionRequest.botId` the bot's
  (the apps check it against the chat). The host keeps one card per login per thread (`data.thread.id`), apart from
  the chat's own; the thread's next task takes it over. The thread's own transcript gets one line, `Asked in the chat to sign in to <agent>.`. A failure
  that offers **Add a fallback** goes to the chat the same way; the transcript keeps the failure.
- **A task waits for the sign-in.** When no agent of the chain can take the turn and the card holds the task's
  message, the task doesn't fail: it stays working on its thread, reads `needsYou` with the activity
  `Needs <agent> sign-in`, and a needs-you alert goes to the bot's chat. Once that login is signed in, the held
  message goes out again to the same thread, however long it waited (`agent_sign_in::send_again` sends each held
  message to the chat it was written in; only the chat's own expire after a day), and the task goes on in its
  session. A message that went out again at once (a process older than the turn) keeps the task going too. A thread
  whose task was stopped or went back to the box gets nothing sent; its Stop ends the wait, and the thread.
- **A sign-in keeps the session.** A refused sign-in starts a new agent process before the next turn (the old one
  holds the old credentials), as it does for the chat: an agent that can reopen its session goes on in it
  (`restart_before_turn`), so a thread keeps its fork or brief and what it did; one that can't starts a new session,
  told the conversation so far as after a move to another agent (`owe_recap`; a thread's, the chat it comes from too).
  With no process left (it quit, or the new one couldn't start), the next turn starts one and reopens the session,
  as after any exit.
- **The usage gate looks past a resting agent.** The dispatcher's "near its usage limit" reads the agent a new thread
  would start on (`fallback::starting`: the first step that isn't resting), not only the bot's own.

The apps show the `Work · <title>` line on the sign-in card and on the failure, and a needs-you row reads the card's ask
(`Needs <agent> sign-in`) and opens the chat at the card, as for an approval. When the turn left its agent signed
out and a fallback was signed out too, there are two cards: the row names, and opens, the one that holds the task's
message (`connectionRequest.entryIds`), whose sign-in is what sends the work again; an approval or choice newer than
it comes first, as what the running turn waits on. In the task's
[thread](#talking-to-a-thread) the sign-in card is one quiet line beside that transcript line, as the card stands
(`Needs <agent> sign-in`, then `Signed in` or `Cancelled`), which opens the chat at the card; the failure that offers
**Add a fallback** has no line there, since the transcript holds the failure itself.

## Asking and sharing from a thread

A thread on the bot's own work reaches the user through the bot's chat, as its approval card does. What it posts there
is the bot's (`data.author`), numbered with the chat's latest turn, and carries `data.thread = {id, title}` and
`data.task`, so a client shows the same `Work · <title>` line and can open the task
(`Actor::add_for_user`).

- **`request_decision`**: the choice card is posted in the bot's chat, the task reads `needsYou` while it is open,
  the alert (*needs you*) opens the bot's chat, and a press (`respondDecision`) goes to the thread (`work::card_actor`).
  The user's choice is recorded as their message in the chat, under the card.
- **`send_file`** (a voice note too): the file message is the bot's in the chat. The copy is kept in the bot's files,
  so it outlives the thread.
- **`publish_artifact`, `list_artifacts`**: the artifact is the bot's (its Library), and its card is in the chat.

A card that belongs to a thread, case by case:

| What happens | A chat's own card | A thread's card |
| --- | --- | --- |
| The user types a message | It answers the card, or ends it (`other_message`) | Nothing: in the chat the message is for the bot, and in the [thread](#talking-to-a-thread) it waits behind the card's turn. Only a press answers, so the card's text doesn't ask for a typed reply |
| Stop in the chat | Ends it (`stopped`) | Nothing: the chat's Stop is the chat's turn. Stopping the task (`stopWork`, the bot's `steer_task stop`) ends it |
| The task gives way or gets a new brief | | Ends it (`stopped`): its thread is closed |
| The turn ends | Ends it (`turn_ended`), unless its call already ran out (the card is open) | The thread's turn, not the chat's: a card the thread left open ends with the task |
| Four and a half minutes | The call returns `answered: false` and the card stays open: a press or the user's next message answers it later as their next message ([choice cards](chat-messages.md)) | Ends it (`timeout`), the call returns an error; the thread goes on without an answer, since a press there has no next message to become |
| The bot asks again | Ends the open card (`asked_again`) | One card per thread, as below |
| Host restart | Ends it (`restart`) | The same: the resumed thread has no call waiting |
| One at a time | One card per bot | One card per thread; two threads may each have one open in the chat |
| Alert | *needs you*, the bot's chat | The same, named for the bot |

A permission card follows the same rows, except that a typed message never touched it. A task waiting on a card is
never taken for lost by the hourly look: its turn runs.

## Other sources

A run of one of the bot's routines and another bot's `message_bot` request are tasks in the bot's box
(`sources.rs`), so the chat stays free while they work and they wait for a free thread under `maxThreads` like anything
else. A task's `source` says which: `routine` with the routine's id, or `request` with the asking bot's id (none
when it was asked from another computer or by the cloud assistant). A request starts like any task (a fork of the
chat's session where the agent can, else a new session told the chat so far); a routine's run starts in a new
session that knows only its own prompt, as it did in the chat. Their thread is told the routine's own
prompt or the request as it is, not a task's brief, and **their result takes no turn of the bot's**:

- **A routine's run** ([routines](routines.md#execution)): when a thread takes it, the run begins as it did in the
  chat (the latest instruction and progress note, the `Routine · <name>` line). Its last message is posted in the
  chat as the bot's message, in the turn that line took, and the run's outcome is saved: the alert, `(pass)`, the
  failure line, the retries after 1, 5 and 15 minutes and the Try again button are the routine's as before. The
  routine's time limit stops its thread, its session's setup included, and only while the run is what the thread works
  on (a task that followed the run into its thread has no limit). The task's title is the routine's name.
- **A request** ([bot collaboration](bot-collaboration.md#how-a-message-runs)): the exchange's notices read
  `Running…` when a thread takes it, and the thread's last message is the reply the asking bot gets, with the same
  hop count, as it is: a reply of exactly `(pass)` goes back as those words, and a turn that ends with no text is
  `recipient finished without a text reply`, both as from a turn in the chat. Its title is the request's first line.

What such a turn could and couldn't do in the chat holds in its thread (`Actor::task_source`): `request_decision`,
`send_message`, `send_file` and a request's artifact cards are refused with the same words (a run's artifact card
goes into the bot's chat, as from a run there); the `routines` tools work, as the
bot (a run saves its progress and sets follow-ups, an asked bot runs its own routine); `message_bot` works from a
run and is refused in a request; `follow_up` is refused in a request. `add_task` is refused as in any thread. A
request from another computer under a network rule's `auto` or `ask` holds its thread to that
[ceiling](codync-network.md#ceilings-auto-and-ask) (`work::access_of`), as it held the turn in the chat.

What differs from the bot's own tasks:

- **Stop** cancels the run (`Stopped by user`, no failure line) or tells the asking bot its request was stopped.
  **Drop** on a waiting one does the same.
- **Try again** on a routine's task runs the routine again: a new run (with the failed run's event when it kept
  one, else as `runRoutine` does) and a new task in the old one's place. A request can't be tried again: the bot
  that asked has its answer.
- **They never give way** to a task that cuts in, and `steer_task replace` refuses them: started again they could
  act twice.
- **They never start twice.** Cut off by a host restart with a thread that can't resume, or lost (the hourly look), a
  run ends `interrupted` at once and its routine's retry rules decide, and a request fails. A request lives in the
  host process (its notices are marked interrupted at start, as before), so its task leaves the box at host start.
- A run whose routine was paused or deleted while it waited leaves the box.
- Stop in the chat no longer cancels a routine's run that waits in the box, nor a request: the box's Stop does.

**Kept as turns in the chat**: a bot with **computer use** or **Manage the team** on takes its routine runs and
requests in its chat as before (`work::boxed`), because a thread has neither the `computer` nor the `manage` server
(one screen and one team stay with the chat), so a run that drives the screen or sets up a bot would stop working.
Rooms keep their own path.

## Memory

A thread's writes to memory are its own turn's (`memory::turns`, keyed by the actor: the bot, or one of its threads),
credited to its bot and made in the thread's session, for the bot's own tasks, a routine's run and a request alike:

- **`edit_memory`** from a thread is a call of the thread's own `memory` server. A thread's turn is never the user's own
  turn in the bot's chat, not even one their message to the thread started, so it may add lines and never change or
  take one out, whatever the chat is doing meanwhile (before, it borrowed the chat's running turn, and could take a
  line out while the user's own turn ran there).
- **File tools and shell commands** in a thread are checked at the end of the thread's turn, as a chat turn's are: a
  change that may not stand (a line taken out) is put back, and the thread's session hears why. A line it added is
  recorded as the bot's.
- The thread's session knows what it wrote itself; the chat's hears of the change with its next message.

## Worktrees

Two threads in one git repository would write over each other's files. With **`threadWorktrees`** on (per bot, on
unless set; it matters only where the bot's folder is a project folder in a git repository, `gitRepo` on the wire:
a bot's [own folder](bot-workspaces.md#versions) keeps all its work in one place, with its versions), a thread for one
of the bot's own tasks that starts **while another thread of the bot is working** gets a git worktree of its own
(`worktree.rs`):

- `~/.codync/worktrees/<bot id>/<task id>`, on a new branch `codync/<title in lowercase, dashes>` (`-2`, `-3`… when
  taken), from the commit the bot's folder is on. The thread's agent starts there (in the same place of the
  repository as the bot's folder, when that is a folder inside it), is told so with its task, and starts from a
  brief, not a fork. Uncommitted changes of the bot's folder aren't in it.
- **The first thread alone keeps the bot's folder**; a routine's run and a request never get a worktree; a task that
  follows another (`after`) goes on in that thread's folder, worktree or not.
- It is made by the thread itself as its first turn starts. When git fails (no commit yet, git missing, the folder in
  the way, a checkout hook that fails) or the checkout lacks the bot's folder (git doesn't track it: ignored, or
  never added), the thread works in the bot's folder as before, and the failure is logged. A worktree git made and
  the thread can't use is taken back at once, with its branch (`git worktree remove --force`, `git branch -D`):
  nothing has worked there.
- **When the task is over** and nothing waits to follow it (a task the hourly look or a host restart ended
  included): a worktree with nothing in `git status --porcelain --untracked-files=all` (untracked files count
  whatever the user's `status.showUntrackedFiles` says), on
  its own branch, with that branch still at the commit it started from or merged into the repository's own checkout
  (`git merge-base --is-ancestor <tip> HEAD`), is removed (`git worktree remove`, then `git branch -d`, neither
  forced), and the report says so. Anything else is kept, with its branch, and the task's report to the chat names
  the branch and the folder, for the bot to tell the user and merge when asked. A task the user stopped is looked
  over once its thread's turn has ended; it reports nothing.
- **At host start** every kept worktree, and one whose look the last process didn't finish, is looked over again
  (`recheck`, from `pick_up`): one the user has merged since goes, with its branch. So does a worktree under
  `~/.codync/worktrees` that no task has as its own or works in any more (one inherited by a task that followed
  counts, though it is named after the earlier task; it goes once its last task ended more than a week ago, or its
  bot was deleted: `known`, `tidy_orphans`), once its commits are merged; one that isn't merged stays, as the user's work. Between restarts a
  kept worktree stays as it is.
- A task that gave way, got a new brief or is tried again starts again in the worktree it had. While an ended
  task's worktree is being looked over (`Left::Settling`) the task tried again waits; the look notes kept or removed
  and runs the dispatcher again. A message to the task's thread meanwhile takes the worktree back as in use, which
  the look then leaves as it is.
- **Codync never merges, rebases, resets, forces or deletes a branch that has work on it**, and never touches a
  worktree it didn't make: what a box says is checked first (the folder is `~/.codync/worktrees/<id>/<id>`, the
  branch `codync/…`). Git runs with an argument list, never through a shell, each command for at most a minute.
- Dev servers, `send_file` and `publish_artifact` of a thread in a worktree start from the worktree.

## Outside the chat

What shows a bot away from its chat (the iPhone's Live Activity, Android's ongoing notification, both fed by
`push::live_activity_update`) reads the bot through `Hub::outside_runtime` (`work::outside`): it **needs you**
while the chat or one of its tasks waits on the user; it **works** while the chat works or its box holds anything
that isn't over (a task working or waiting, or one whose report the chat hasn't taken yet); otherwise it is the
chat's own status. An update goes out when that status changes (`Hub::outside_changed`: after the chat's or a
thread's status changed, and after each dispatcher pass), so an activity doesn't end when the chat's turn does while
tasks work, and ends once the box is empty and the chat idle. The push carries the status and the start time only,
as before: the line (`Needs you`, the chat's activity, `2 working · 3 waiting`) is each client's, from `bot.work`.
The same status is on the wire as `bot.outside = {status, startedAt}`, beside the chat's own `status`, for what a
client keeps up to date itself (the activity while the app runs, the tray, the island).
A task's end sends no alert of its own: its report is a turn of the bot's, whose reply alerts as any reply does.

## Wire

- Bot: `work: [task]`: every working task (oldest first), then the waiting in dispatch order, then the tasks ended
  in the last 24 hours (newest first, at most 20). Task: `{id, title, status, activity, thread, wait, cutIn,
  createdAt, startedAt, endedAt, result, talk, folder?, source?, worktree?}`; `activity` is its thread's, `thread` the thread bot that has or had it (null while it waits), `talk` whether the user may write to its [thread](#talking-to-a-thread) (the bot's own task whose thread bot is there), `folder` where its thread works (the worktree's folder for one in a worktree, else the bot's folder as it was when the thread began; absent before a thread took it, and once its worktree was removed with nothing in it), `wait`
  is `{kind: next | behind | after | usage, ahead?}` on a waiting task and null otherwise, `result` at most 280
  characters. `source` is `{kind: routine, id: <routine id>}` or `{kind: request, id?: <asking bot id>}`, and absent
  on a task the bot added, so a client that doesn't know it shows the row as before. `worktree` is
  `{branch, path}` while the task has a [worktree](#worktrees) that is still there, and absent otherwise. The bot's own `status` stays the chat's.
- Cards and messages a thread posts in the chat carry `data.thread = {id, title}` (the thread's id, the title of the
  task it works on) and `data.task` (the task's id, or null): permission cards,
  choice cards (`data.decision`), agent sign-in cards (`data.connectionRequest`, `kind: agent`), failure notices that
  offer **Add a fallback** (`data.addFallback`, without the `stopReason` of the thread's own), file messages
  (`data.files`) and artifact cards (`data.artifact`).
- Bot: `maxThreads`, set with `updateBot` (1 to 6).
- Bot: `outside: {status, startedAt}`: how the bot stands [outside its chat](#outside-the-chat) (`idle`, `working`,
  `needsInput`, `error`), where `status` is the chat's alone. Absent on an older host: `status` then.
- Bot: `threadWorktrees` (true unless set; `updateBot`), and `gitRepo`: whether the bot's folder is in a git
  repository, which, for a project folder (not `managedWorkspace`), is where the setting matters and a client shows
  its switch. Both absent on an older host.
- Bot: `gitInstalled`: whether the host can run git (`git::installed`: the answer of `git::program`, kept for the
  process and looked up again in the background at most every five minutes, so a snapshot never waits for it; true
  until the first look ends, which the host starts as it starts). Absent on an older host: read as installed. With
  `gitRepo` it gives the line under the folder in Settings: `Git repository`, `Not a git repository`, `Git isn't
  installed on this computer`. The manual's page for it: `/docs/working-folders` (`#without-git`).
- `threadEntry` events `{rev, thread, entry}` for a thread's transcript; none in the catch-up.
- `threadEntries {botId, threadId}` → `{thread: {id, title, status, startedAt, endedAt, start}, entries}` (newest 500):
  `threadId` is a task's `thread`; `title`, `status` and the times are of the task it works on, else the last it
  worked on. A user entry there is what the thread was told (`data.from: "host"`: a task, as its thread read it) or the
  user's own message to the thread (no `from`; `data.status` `queued`, `sent`, `cancelled` or `failed`, as in a chat).
- `send {botId, taskId, text, clientNonce?}` → `{entry, reopened, queued}`, `steer {botId, taskId, entryId}`,
  `unqueue {botId, taskId, entryId}`: [talking to a thread](#talking-to-a-thread).
- `workFirst {botId, taskId}`, `workCutIn {botId, taskId, insteadOf?}`, `dropWork {botId, taskId}`,
  `stopWork {botId, taskId?}`, `retryWork {botId, taskId}`.

A thread's id is opaque: a client never tests its prefix.

## Stored data from before the rename

Until host schema 9 a task was called an item and a thread a task, in the database as on the wire. The wire has no
aliases: the host sends and takes only the new names. What it had stored is read as follows.

- **Bot config**: schema 9 (`migrate_9` in `store/schema.rs`) renames `taskOf`, `maxAgents` and `taskWorktrees` to
  `threadOf`, `maxThreads` and `threadWorktrees` in each stored `bots.config`, once at startup. After that nothing
  reads the old names, not even `updateBot`.
- **The box** (kv `work.<bot>`): a task's `thread` also reads `task`, and a kept report's `task` also reads `item`
  (`work/tasks.rs`, `Report` in `work/mod.rs`); the box is saved under the new names at its next change.
- **Entries a thread put in its bot's chat**: schema 9 (`migrate_9` in `store/schema.rs`) rewrites each stored entry
  whose `data.task` is an object: that object becomes `data.thread`, `data.item` becomes `data.task`, and `data.item`
  is removed. Revs stay as they were; `hello.schema` is 9, so a client that keeps a synced copy fetches the
  conversation again.
- **Thread ids**: a new thread is `thread-<uuid>`. A thread started before the update keeps its id `task-<uuid>`
  (`store::is_thread_id` knows both), since its transcript, session and cards are keyed by it; it goes when its task
  is purged, 7 days after it ended.

## In the apps

The capsule, the bot's Details (Threads, Event box, Later, Routines, Dev servers, Recent), `Threads at once` in its
Settings and cutting in are specified in [the event box](../plans/event-box.md#in-the-apps) (stage 2). The sheet of every bot's work, the rows' source marks, the `Work · <title>` line on what a thread posts, the **Worktree per thread** switch and what follows `bot.outside` are in [as built, clients](../plans/event-box.md#as-built-clients-stages-3-to-8). A task's thread view, the where line on a row and in the thread's header, and the git state in Settings are in [stage 11, as built on the terminal](../plans/event-box.md#as-built-host-and-terminal-stage-11) and [on iPhone, Android and desktop](../plans/event-box.md#as-built-iphone-android-and-desktop-stage-11). A client reads `bot.work` and `bot.maxThreads`; both are
absent on an older host, where the feature doesn't show.

## Restart and cleanup

At host start (`work::restore`): tasks ended over 7 days ago are deleted; a working task whose thread's turn was cut
off and can be resumed (`agent::bot::resumable`: a restart mark under an hour old) stays working, and its thread's
actor resumes the turn; one whose thread can't goes back to waiting once, as a task that was cut in on, and fails with
a report the second time. An ended task's undelivered report is sent again: to the bot, or, for a routine's run,
as its message in the chat (posted once, by its entry id) and its saved outcome. A run the box no longer holds is
queued again if it waited, and ends `interrupted` if it ran. Thread bots whose bot is gone, or that no
task names, are deleted. Deleting a bot deletes its box and its threads, and cancels the requests it hadn't answered.

The hourly look (`work/sweep.rs`) catches what no event said. After every dispatcher pass that leaves waiting or
working tasks in a box, one timer per bot is set for an hour later (reset by the next pass, none for an empty box).
When it fires, the host takes a dispatcher pass again, and a working task whose thread runs no turn (its actor is
gone or idle, and nothing is about to resume) follows the restart rule: back to waiting once, then failed with a
report (`work_finished` `error: lost`). A turn that runs is never touched, however long and quiet. The look is host
code only: no agent is woken unless a task was lost.

## Limits of this version

- A thread never changes or takes out a line of memory, the bot's own tasks included, also in a turn the user's
  message to the thread started. What the user asks the bot to forget, the bot does in the chat.
- A thread takes text only: no files, no quoted message, no voice. A message sent right as a host restart comes is
  marked failed and has to be sent again.
- A turn that ended with a message of the user's still waiting reports nothing of its own: its last message stays
  in the transcript, and the report is the last turn's.
- No subagent rows.
- A thread in a worktree starts from a brief, not a fork (no agent is checked to honor another folder on a fork).
- A worktree is a new checkout: what git ignores in the bot's folder (installed packages, build output, local env
  files) isn't in it, and removing an untouched one deletes what its thread left there ignored.
- On Windows a worktree whose agent process is still in it can't be removed and stays (kept). A host that stops
  while it looks a worktree over looks it over again at its next start (`worktree::recheck`).
- A turn of the chat that ends while tasks still work alerts as done with the bot's words; the Live Activity goes
  on.
- A Live Activity or ongoing notification updated by a push shows the status only; the counts line needs the app
  (the relay passes no more than the status and a date).
- What a thread's agent runs inside its own process (a harness's background agents and shells in the agent's session)
  ends when the thread does: its process and that session's processes are stopped (`Acp::kill`). A program it
  detached into a session of its own (a server from its shell) isn't reached and keeps running; one left listening
  shows among the bot's [dev servers](dev-servers.md).
- A thread ends as soon as its task does when nothing waits to follow it, so `after` is for work that is still
  working or waiting.
- A routine's run in a thread has its `send_message` refused as before, and what it wrote reaches the chat when it
  ends.
- A thread on a routine's run or a request can't change the bot's profile or fallbacks, which that turn could in the chat.
- A typed message never answers a thread's choice card; a client without buttons can't answer one.
- A thread's line for what it shared finds that entry only among the chat's messages the client has loaded,
  as a needs-you row does; an older one shows no line, and the line's tap leaves the chat where it was.

Tests: `host/tests/work_e2e.rs` with `tests/fixtures/work_agent.py` (fork, brief, a card with `data.task` answered
from the chat, refusals in a thread from each server, the ceiling and the order, first, drop, cut in from the user and
from the bot, replace on a waiting and on a working task, `after` in the same session, stop one and all, retry, an
agent that can't start, a stopped turn dropping the task it added, a deleted bot taking its box and threads, a host
restart mid-task, cut in on a task that follows working one closing nothing, no `bot` or `entry` event for a thread
bot; a routine's run as a task: its source on the wire, waiting for a free thread, its message posted once in its line's
turn with no turn of the bot's, `(pass)`, its progress note, the refusals of a run, stop, a deleted routine's waiting
run, try again, never giving way, an agent that can't start, a host restart mid-run; another bot's request as a
task: its source, the notices, the reply turn of the asking bot, the refusals of a request, stop, no try again; a
thread's choice card answered by a press and not by a typed message, ended by stopping its task and not by the chat's
Stop, its file and artifact in the chat and kept with the bot; a run that never forks nor sees the chat, a request's
`(pass)`; a thread's `edit_memory` adding and refused a removal while the user's own turn runs in the chat; a thread
beside another in a repository working in its worktree, an untouched one removed with its branch, a touched one
kept and named in the report, the switch off, a project folder that is no repository and a bot's own folder (a
repository, never a worktree); a message to a working thread
queued, sent twice as one, removed, delivered in the thread's session and in the task's one report, the refusals of a
thread's own id, another bot's task, a waiting task and a routine's run; Send now stopping the turn for the message
and joining it where the agent steers; a message to an ended task reopening it past the ceiling, `add_task` still
refused there, its second report, and a new session told the work where the agent can't load one; a message
waiting behind a thread's card without answering it; Stop dropping what waited; a host restart failing it and the
thread taking a message afterwards; what waited going with a task that gives way; a thread's sign-in asked in the
chat and its task going on in the same thread once signed in, a thread moving past a signed-out agent and its next
agent told the chat, a thread's offer to add a fallback in the chat and new fallbacks reaching a running thread, a thread
that starts on a fallback never forking the chat's session); `memory_files_e2e` (a line a
routine's run in a thread takes out with a shell command is put back); unit tests of a worktree made, removed, kept,
removed once merged and never touched when it isn't Codync's, the kept ones looked over again at a restart
(`recheck`), the walk that removes only merged worktrees no task knows (`tidy_orphans`), and an inherited worktree
known after its earlier task is gone (`known`) (`worktree.rs`), how a bot stands outside its chat (`tasks.rs`), a box stored before the rename read and saved under the new names (`tasks.rs`, `store/model.rs`) and schema 9's rewrite of stored entries (`store/schema.rs`), unit tests of the dispatch order and the `wait` reasons
(`dispatch.rs`), the agent a new thread starts on (`agent/fallback/mod.rs`), a thread's held message sent again however
old and not once its task ended (`market/agent_sign_in.rs`), a task's end and the restart rule, also for a run and a request (`mod.rs`), a source on the wire (`tasks.rs`), which bots keep runs in the chat (`sources.rs`), the 20 waiting tasks and the title and brief lengths (`tools.rs`), the prompts and the box's lines. Not
covered by a test: a reopened task's kept worktree becoming its folder again and a removed one's thread moving to the bot's folder end to end (the unit test covers the folder), a message to a task whose thread is on a task that followed it, a message that arrives as the task's turn ends (the lock that orders them), a real agent resuming a reopened session, a replaced brief carrying what waited, a worktree reused by a task that starts again, one inherited by a task that follows, a
stopped task's worktree looked over, a task stopped while it waits for a sign-in ending its thread, a worktree's dev server and artifact, git failing, a real agent in a worktree, the host-start reclaim end to end (`tidy` spawning
`tidy_orphans`, `pick_up` settling what `recheck` took),
the Live Activity push itself (the relay isn't in the tests), a routine's time limit in a thread, a lost run or request at the hourly look, a run the box lost at host start, a request from another computer or the cloud assistant as a task, `message_bot` from a run's thread, a thread's voice note, a choice card's timeout and its end at a host restart, the usage and in-a-row limits, the desktop's Add a fallback opening Settings, `add_task` refused in a room or a bot's request (a routine run is: `routines/tests.rs`), a report turn that fails, a
thread's dev server, a task sent back to waiting by a restart end to end, the 7-day purge, and the box's lines in a
known harness's prompt.
