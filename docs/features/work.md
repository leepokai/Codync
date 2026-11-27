# Work: tasks and threads

A bot puts long work in its **event box** as **tasks**. **Threads** of the same bot take them: away
from the chat, each with its own agent process and session, so the chat keeps answering. The chat shows only messages and what needs the user (a thread's approval
and choice cards, the sign-in card its agent needs, its offer to add a fallback, the files and artifacts it shares);
the work shows as one capsule above the composer, and each
task's report comes back as the bot's own message. A run of one of the bot's routines and another bot's request are
tasks too ([other sources](#other-sources)).

| | The chat | A thread |
|---|---|---|
| Session | The bot's main one | Its own, new, told its task and nothing of the chat ([how it begins](#how-a-threads-session-begins)) |
| Agent process | The bot's | Its own |
| Transcript | The chat | Its own, read from the task's details |
| Who talks to it | The user | It gets tasks, one at a time; the user may write to the [thread](#talking-to-a-thread) of the bot's own task |
| Its result | A message | Its task's report, which the bot turns into a message |

Design: [the event box](../plans/event-box.md), [bots, projects and threads](../plans/bots-projects-threads.md); why a thread is a bot of its own:
[the first design](../archive/background-tasks-plan-2026-11-15.md). Code: `host/src/chat/work/` (`tasks.rs`: the box, its wire shape and what the bot reads of it;
`dispatch.rs`: which task a thread takes next; `steer.rs`: first, cut in, replace, stop, drop, retry; `mod.rs`: a
thread's turn end, the report, host start; `sources.rs`: routine runs and other bots' requests as tasks; `thread.rs`: the user's messages to a task's thread;
`tools.rs`: the bot's tools and what a thread may not call; `recall.rs`: `recall_task`; `ask.rs`: `ask_task`; `ask_chat.rs`: `ask_chat` and `answer_task`; `sweep.rs`: the hourly look; `watch.rs`: the [watchdog](#watchdog); `worktree.rs`: a thread's git worktree;
`prompts.rs`: the words); the actor's side is in `host/src/agent/bot/` (`session.rs` for its session, `turn.rs` for the box in the prompt, the worktree
and the report, `queue.rs` for `turn_over`, `updates.rs` for cards). Git is run through `host/src/git.rs`.

## A task

One piece of work: `id, title, brief, source` (where it came from: the bot, a routine's run, another bot's request),
`words` (the user's own, word for word), `notes` (what the user said about it
since), `relayed` (what the bot's chat passed on to it since, `tell_task`), `status, order, after, thread, cutIn, hops, createdAt, startedAt, endedAt, result, report, failure, worktree,
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
deleted with its transcript; one whose tasks went to the [archive](#older-threads) is parked there instead.

## The dispatcher

The user sets how many threads a bot runs at once, `maxThreads` (1 to 6, 3 unless changed, `updateBot`). It is
the only ceiling: the computer as a whole has none. `work::dispatch(hub, bot)` runs whenever the box or one of its
threads changes: a task added, moved, dropped, stopped or retried; a thread's turn over; the bot's own turn over; `updateBot`; host start. For each waiting
task:

1. **It follows a task** (`after`) that is done and whose thread is still there and idle: that thread takes it, in the
   same session. These go first, since that thread already runs. While what it follows waits or works, it
   waits (`wait: after`). When what it follows failed, was stopped or dropped, or its thread is gone, it is an ordinary
   task.
2. **Otherwise a new thread starts for it**, lowest `order` first: a thread the user started at once, the bot's own work when the bot is under `maxThreads` (counting only its own), the bot's own turn is over (stopping that turn drops a task it added, and a routine's run or a room's turn shares the actor), and the agent is
   not near its usage limit (near it, one thread at a time: `wait: usage`).
3. **Otherwise it waits**: `wait: next` for the first in line, `behind` with how many are `ahead` for the rest.
   Nothing is refused for being over the ceiling.

A task that can't start now whatever is free (`dispatch::held`: a routine run while a run of its routine is under way
or the host is stopping, a task whose worktree is still being looked over) takes no place: it reports `wait: next`
and the tasks behind it may start first.

**A thread waiting on the user gives its seat up** (`dispatch::free_seats`): while its runtime is `needsInput` (an approval,
a choice, a sign-in) it doesn't count toward `maxThreads`, so the next task can start meanwhile; the user's answer
lets it go on even past the ceiling. Every live thread still counts toward `maxThreads + 1`, so only one seat is ever
lent. A thread that begins to wait on the user looks its bot's box over again (`Work::seat_freed`). Near the usage
limit it still counts as running, so nothing else starts. Where the next thread gets no worktree of its own (a folder
that isn't a Git repository, **Worktree mode** Off, the bot's own folder) it works in the same folder as the
one that waits, even at `maxThreads` 1.

A new thread waits for any turn of the bot to end, a routine run's or a room's too (they share its actor); that turn's
end looks over the box again.

The decisions are one pure function (`dispatch::standings`), which the wire's `wait` reads too.

## Tools (on the `chat` MCP server)

- `add_task {title, brief, after?, folder?, place?}` → `{taskId, status, note, placeNote?}`: puts a task in the box and answers at once.
  `folder` is where its thread works, the full path of any folder that exists (the bot proposes its
  [projects](#where-a-thread-works); `invalid` otherwise); left out, the bot's default project, else its own folder.
  `place` (`here` | `worktree`) is where in a git repository it works, over the bot's [worktree mode](#worktrees);
  left out, the mode decides. A worktree asked of the bot's own folder, or of a folder outside a git repository, is
  left out, and `placeNote` says why; with `after` it says the task works where that one does. Refused
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
- `tell_task {task, text, now?}` → `{taskId, status, note}`: passes words on to a task without starting it again (what
  the user adds about work under way, a fact it needs). The task keeps them in `relayed`, which a session that starts
  it, or starts it again, reads as what its chat passed on, never as the user's words.
  A working one gets them as a message in its [thread](#talking-to-a-thread), from the host (`data.from: host`,
  `data.relay`): it waits behind the running turn as the user's does (with `now`, into that turn at its next step
  where the agent can, else the turn stops for it), and keeps the task going until a turn takes it, so the chat
  still gets one report. Refused for a task that ended (add a new one), for a routine's run and another bot's request
  (read only, as [their thread](#talking-to-a-thread) is), and under a ceiling, as `add_task` is.
- `ask_task {task, question}` → `{taskId, answer}`, or `{taskId, status: "no answer yet", note}` (`work/ask.rs`; the
  name is one macro, `ask_tool!`, in `work/mod.rs`): the bot asks a working task a question. It goes into the
  thread's running turn as `tell_task`'s `now` does, a message from the host marked `data.ask` (and `data.relay`, so
  every client quotes it as what the thread was told), which the thread hears with a line saying the bot asks and to
  answer in one short message, then go on; the thread's instructions say the same. It is never kept in `relayed`. The
  thread's next message after it (the first text the agent writes once the question reached it: joined the running
  turn, or began a turn of its own where the agent can't take it mid-turn, whose running turn then stops for it) is
  the call's result, not the report: it stays in the thread's transcript, and only if it is the turn's last message
  is it the report too. The call
  waits a minute (`ask::WAIT`; the MCP request 20 seconds more); after that it answers `no answer yet` and the
  answer, when it comes, starts a turn of its own in the bot's chat (`Cmd::TaskAnswer`, which alerts as a reply
  does), as a `message_bot` reply does. A question the thread never answers goes when its task ends or is set aside;
  one waiting is lost with the host process. A task the late answer's turn adds carries the task's hops on. Refused
  (counted in `task_asked`) for a task that waits (`tell_task` it) or ended (its report came), a thread the user
  started, a routine's run, another bot's request, and while a question to the same task still waits for its answer;
  refused too in a thread and under a ceiling, as `add_task` is (those not counted there).
- `answer_task {task, answer}` (`work/ask_chat.rs`) answers a thread's `ask_chat {question}`, the thread's own tool
  (listed only for threads, not chat-only): the question starts a turn in the bot's chat (`Cmd::TaskAnswer`) that
  tells it to answer from what it knows, then `(pass)`. That turn is host-started, so it can't call
  `request_decision`: what only the user can decide, the chat says so in its answer and the thread asks them on its
  own card ([below](#asking-and-sharing-from-a-thread)), which makes the task `needsYou`; while it only waits for the
  chat the task stays `working`. The thread's call waits 270 seconds (`ask_chat::WAIT`, under Codex's 300 s tool
  timeout, as `request_decision`'s; the MCP request 20 seconds more) and returns `{answered: true, answer}`, else
  `{answered: false, note}`: it goes on and says what it assumed. A later `answer_task` is passed on as `tell_task`'s
  `now`. Only for the bot's own tasks, one question at a time a thread; it goes when its task ends or is set aside.
  Counted in `chat_asked`.
- `work_status {}` → `{tasks: […]}`: every task that is working or waiting, and those ended in the last day: status, what its thread
  does now, why it waits, seconds, result.
- `recall_task {task?, query?, before?}` (`work/recall.rs`): the bot looks back at its tasks, ended ones too, from
  its box and the [archive](#older-threads) (tasks that ended in the last 30 days, the newest 200). With `task`: its brief, `usersWords`, `usersNotes`, `chatPassedOn`, whole result,
  `after`, `followedBy`, local times, and a page of 30 of its thread's steps (user, agent, notice, tool and card lines,
  each cut at 600 bytes) with `earlierSteps`, the `before` for the page before; none once the thread is gone. With
  `query`: the tasks whose title, brief or result has it (any case); with neither, the latest; 20 at most, each result
  cut at 280 bytes. Read only: allowed under a ceiling.

Refused in a thread: these seven and changes to the bot's profile or fallbacks (`work::refuse_chat_only`, the `chat`
server's), and the `routines`, `team` and `manage` tools (`work::acting_as`, `work::refuse_in_thread`). The refusal
says to put what it needs in its report. `ask_chat`, `request_decision`, `send_file` and artifacts work from a thread
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
  gives way (the one named with `insteadOf`, else the one that started last; never a thread the user started, which
  takes no seat, so naming one is refused): its thread is stopped and deleted, and it
  waits at the front, behind the task that cut in, with `cutIn` set. It starts again later in a new thread and session,
  told that the work was begun before. With a thread free, cutting in is moving first; so it is for a task that
  wouldn't start on the freed thread (it follows unfinished work, or waits for usage), and the clients don't offer
  Cut in on such a row. A held task (a routine run behind its routine's run, a task whose worktree is being looked
  over) still shows Cut in, which only moves it to the front.
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

- **`send {botId, taskId, text, attachments?, clientNonce?}`** → `{entry, reopened, queued}`. The caller names the bot and its
  task, never a thread: the task must be in that bot's box, and `send`, `steer` and `unqueue` refuse a thread's own id
  as `botId` (`unknown bot`). Files go as in a chat: each is first sent with `upload {botId, taskId, …}`, which
  puts it in the thread's own uploads (`uploads::root` of the thread bot, under the host's data folder, never in a
  worktree, and deleted with the thread bot), and the thread's agent gets their paths. `readUpload` reads one back by
  the entry's `botId` (the thread's); the task's report and a session that picks the task up name the files after
  the user's words (`[sent files: …]`). `work[].talkFiles` says a thread takes files: clients show their + (and take
  drops and pastes) only then. No quote or call (`a thread takes no quotes or calls`). `entry`
  is the message as the transcript holds it (`botId` is the thread's id); the same `clientNonce` again answers the
  same entry, with `reopened` and `queued` null.
- **While the thread's turn runs** the entry is `status: queued`, as in a chat, and leaves as `threadEntry` events like
  the rest of the transcript. **`steer {botId, taskId, entryId}`** sends it now: into the running turn where the
  agent can (`_session/steering`), else the turn is stopped for it. **`unqueue {botId, taskId, entryId}`** takes it
  back (`status: cancelled`, `removed: true`; `threadEntries` no longer lists it). With no turn running the message
  starts one. The statuses are the chat's: `queued`, `sent`, `cancelled`, `failed`.
- **What the chat passes on** (`tell_task`) is a message in the thread too, the host's (`from: host`, `relay: true`):
  the thread reads it marked as passed on from its chat, and it never counts as the user's own words in the task's
  report. Clients quote it as what the thread was told.
- **The task isn't over while a message waits.** A turn that ends (done, failed, or stopped for Send now) with a
  message of the user's still queued ends nothing (`work::finished`): the task stays `working`, the message's turn
  runs, and that turn's last message is the task's result and its one report.
- **After it ended.** A done, failed or stopped task whose thread bot still exists (it is kept with the task, in the
  [archive](#older-threads) after a week, until the user's `threadKeepDays`) takes a message too. The first one reopens it: `working` again, under Threads, `startedAt` now, counted
  among the bot's running threads even past `maxThreads` (the user asked), so the dispatcher starts nothing else while
  the bot is over its ceiling, and neither it nor the hourly look ends the thread. The thread's actor starts again and
  opens its session as an actor does after a restart: `session/load` or `session/resume` where the agent can, so
  the session holds the work; else a new session that is told the task first (`prompts::picked_up`: title, brief,
  the user's words and notes, what it reported). A worktree that was kept is its folder again; one that was
  removed leaves the thread in its folder (`Task::folder`). When that turn ends the task ends again and reports again. A
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
  ([below](#asking-and-sharing-from-a-thread)), answered, downloaded and opened there. The desktop and the phones show a thread in the chat's form (desktop
  `ThreadChat`, iPhone `ThreadChatView`, Android `ThreadChat.kt`): each of these is the chat's own row, in time order
  among the thread's own entries, the same entry of the bot's chat, answered, downloaded and opened in the thread as in
  the chat ([UI conventions](../design/ui-conventions.md)). Only the terminal shows each as one quiet line with a
  symbol (`shared_line`): `Waiting for your approval: <title>` or `Asked: <question>` while it waits,
  `Approved: <title>`, `Denied: <title>` or `Answered: <choice>` once answered, `Not answered: <…>` when it
  expired, `Sent a file: <name>`, `Sent a voice note`, `Made an artifact: <title>`; its agent's
  [sign-in](#fallbacks-and-sign-in) card reads `Needs <agent> sign-in`, then `Signed in` (or `Cancelled`). The
  line opens the chat at that entry.

With the rest:

| What happens | A message in the thread |
| --- | --- |
| The thread waits on a card (`needsYou`) | It waits behind the turn the card holds, and neither answers the card nor ends it: only a press does |
| **Stop** on the task | The messages that waited are dropped (`cancelled`, **Not sent — stopped** in a client), as a chat's Stop drops its queue |
| The task gives way (cut in) or gets a new brief | Its thread is closed; what waited and no turn took goes with the task as `notes`, so the session that starts it again reads it. The entries end `cancelled` |
| Host restart | A waiting message is `failed`, as in a chat; the cut-off turn is picked up without it. The thread takes a new message afterwards |
| The task's thread is on a task that followed it (`after`) | The message goes to that session and waits behind its turn; the ended task isn't reopened |

## How a thread's session begins

Every thread starts in a session of its own (`session/new`), never a fork of the chat's, and is told nothing of the
chat: the chat may have talked about many things the task has nothing to do with, so the brief is where what the task
needs is written (`work::BRIEF` and `add_task`'s `brief` say so). What its instructions hold depends on where its
task came from (`chat::context::render_thread`, by the task's `Source`):

| Task from | Told who the bot is | Told where it is | Servers besides `chat`, `memory`, connectors, `workflows` |
| --- | --- | --- | --- |
| The bot (`add_task`) | No: the task alone (`context::TASK`); memory is looked up with the `memory` server | Its folder; its last message is its report | none |
| The user (`startThread`) | Yes: name, description, memory opening | Away from the chat; the user reads it here | none |
| A routine's run | Yes | Running a routine; its reply is the run's result | `routines`, `team` |
| Another bot's request | Yes | Answering that bot | `routines`, `team` (its `message_bot` refused, as in the chat) |

Every thread is also told it runs inside Codync, its folder (the bot's, a project, or a worktree Codync made), how
dev servers work, the bot's skills and the compaction keep-list. None hears the chat's own paragraphs (Messages,
Other conversations, `routines::BRIEF` outside a run or request, `work::BRIEF`). A thread's `chat` server lists only
what a thread may call: `ask_chat`, `request_decision`, `send_file`, artifacts and dev servers; not `send_message` (its last
message is its report), `react`, `save_skill`, the box's tools, the profile's or the fallbacks' (`mcp::chat_tools`, `work::chat_only`;
a call to one is still refused, and counted as `thread_tool_refused`). Its instructions are kept with its bot's
sessions (`context::resolve` keys by `BotConfig::owner`), and a profile update is announced only to a thread that was
told who the bot is.

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

### Background work left running

A thread whose turn ends normally (`end_turn`) while work it started in the background still runs hasn't finished
its task: its last message is a progress line ("started the review, waiting for it to finish"), not a report, and
ending the thread would stop its agent and what runs inside it (#634). The task prompt already says not to; the host
also checks, as a fact, never from the message's wording (`Actor::left_running`, `agent/bot/background_work.rs`):

- what the agent itself [reports running in the background](#background-work-its-agent-reports) (Claude's adapter):
  then nothing else is checked, since the agent says when that work ends;
- otherwise, a process the agent runs detached, in a session of its own (`background_work`), that wasn't there when the task's
  round began (`Actor::detached_before`, noted as its first turn starts and kept while the task goes on with what a
  turn left running), where nothing listens: a server left listening is meant to stay and shows among the bot's
  [dev servers](dev-servers.md). Unsure counts as nothing, so no task waits on a guess; Windows can't tell (no
  sessions there), so it never waits on a process.

The first time (`Task::held`, saved with the task), the task stays working (`Outcome::LeftRunning` →
`work::hold`): the host posts a message in the thread (`from: host`, which every client shows as what the thread was
told) asking it to wait for that in the foreground and then report, and the agent and its session stay as they are
(`release_spent` and `stop_spent` are skipped). That turn's end is the task's end. If it ends normally again while
something still runs, the task fails with `Failure::Background` and the result `it ended its turn while … still ran`,
followed by its last message, so every client shows it failed, with that reason and **Try again**. No alert goes out
for the held turn's end. A message the user wrote to the thread that still waits comes first (`thread::unheard`) and
doesn't use the hold up: the task goes on with the agent and session kept the same way, and that turn's end is
checked again; a turn held for a [sign-in](#fallbacks-and-sign-in) never reaches this check; the watchdog looks at the waiting turn as
at any long one; the hold adds no hop (no report, `MAX_HOPS`). Analytics: `work_held` when a task is held, and
`work_finished` says `held` with how it then ended.

### Background work its agent reports

A thread's Claude session asks its adapter to report the work it runs apart from its turn (`Background::Reported`
in `agent/bot/launch.rs`: `_meta.jetbrains.air` with `asyncTasks` on `session/new`; a chat's session doesn't ask, so
it reads as before). The adapter then sends `async_task_*` updates, which the `codync-agent` crate reads as
`Event::Task` (`events/task.rs`): a command sent to the background, a workflow (Claude's `Workflow` tool), a
monitor, and a turn the agent begins by itself to answer what such work said as it ended. In this mode the adapter
leaves out what a tool call's later updates said before (`_meta.claudeCode`) and a permission request's kind and
diff; the crate puts them back (`acp/calls.rs`), so every event reads as from any other client.

- **The list.** What runs shows in the thread's `runtime.background` (`hub::BackgroundTask`: id, kind, name,
  activity, since when) and on its task's wire row (`background`); a workflow's progress updates its activity
  (`Count: two`). Every client shows it as a capsule over the thread's composer (`Background · <name> · <activity>`,
  or the names of several, with the time). Background subagents aren't in the list: the adapter holds the turn
  that launched them open until they finish, so their steps nest in that turn's as before.
- **The task waits.** A turn that ends while something in the list runs ends `Outcome::Waiting`: its last message is
  kept, no report goes out, the task stays working (the dispatcher counts the seat, `turn_runs`) and the thread isn't
  ended. `work_status` names what runs (`inBackground`).
- **The agent's own turn.** When the work ends, Claude begins a turn by itself (`taskType: turn`). With no turn of
  the thread's running it becomes one (`background_tasks.rs`, `AgentTurn::Own`): the thread shows working, its
  steps and approvals show as a turn's, and its end ends the task's round as a prompt's does, so its last message
  is the report when nothing else still runs. One that begins while a turn runs joins it (`AgentTurn::Joined`): that
  turn ends once both have.
- **A stop.** The agent hears nothing of a stopped piece, so no turn of its own follows: when the stopped piece was
  the last that ran and no turn runs, the host asks the thread for its report (`work::ask_report`,
  `prompts::report_after_stop`).
- **Its agent stops.** The list goes with it (`background_lost`): a turn the agent began by itself ends failed, and
  a task that waited between turns fails (`Failure::Agent`, "its agent stopped while its background work still
  ran").
- Analytics: `thread_background` (`agent`, `kind`, `outcome`: started, completed, failed, stopped) and
  `thread_agent_turn` (`agent`).

## Approvals

A thread's permission request becomes a card in **its bot's chat**, with `data.thread = {id, title}` (the thread bot's id,
its task's title), `data.task` (the task's id) and the bot as `data.author`, so `respondPermission` reaches the
thread's actor and a client can link the card to the task. The approval setting is the bot's (copied when the thread
starts). A waiting card stops only its thread; the chat goes on.

## Fallbacks and sign-in

A thread runs its bot's [fallback chain](agent-fallback.md) with the same actor code as the chat: when its agent runs
out of usage, is rate limited, signed out or won't start, the same turn goes on with the next agent. Rests are the
computer's, so the chat and every thread skip a resting login together.

- **The chain is the bot's** (its threads' own in [Advisor mode](#advisor-mode)). A thread copies it when it starts; a
  change reaches its threads at once (`work::fallbacks_changed`, from `Hub::update_bot`), for their next agent: each
  gets the chain that starts at the agent it started on, and one that started on another keeps its own.
- **A new session on another agent hears the thread's own work.** When a thread falls back to a new session
  elsewhere, that agent is told the recap of the thread's own transcript, never the chat.
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
  (`restart_before_turn`), so a thread keeps its brief and what it did; one that can't starts a new session,
  told the conversation so far as after a move to another agent (`owe_recap`; a thread's own, never the chat).
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

## Advisor mode

The bot's chat plans and hands work on; its threads may run on another agent, model or login, with fallbacks of their
own: a strong model in the chat and a cheaper or faster one for the work, or another vendor's. Bot settings → Agent →
**Advisor mode** sets `threadAgent` (a fallback-shaped step; null: off, the threads run on the bot's own) and
`threadFallbacks` (`updateBot`; `threadAgent: null` turns it off, and the host then keeps no threads' chain).
Turning it on in the apps starts the threads on the bot's own agent, model and fallbacks, for the user to change.

- **`agent::fallback::threads`** is the bot as its threads run it: that agent, model, options and login, its chain, and
  Auto for a mode the chat's agent named (the chat's Fast mode only on the same agent). Every thread starts from it,
  routine runs and other bots' requests too.
- **A thread keeps the agent it started on.** Turning Advisor mode on or off, or picking another threads' agent,
  reaches new threads; running ones keep theirs, and their chain follows the one that starts there.
- **Both chains count**: the host validates the threads' chain as a bot's own (each login once, at most three
  fallbacks), settles and protects its login profiles, reads usage for its agents and shows its rests (`resting`).
- `bot_updated` carries `advisorMode` and `threadAgent` ([analytics](../reference/analytics-events.md)).

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

**A thread the user starts** (`startThread {botId, text, setup?, attachments?}` → `{taskId}`, `sources::add_thread`;
`Source::User`): the user's first message is a task of the bot's box, named by its first line, that starts at once and
may get a worktree like the bot's own work. It is the user's, not the bot's queue: it takes no seat under
`maxThreads` and doesn't wait for the bot's own turn (only near the usage limit does it wait, one thread at a time),
and the bot doesn't see it (its box summary, `work_status` and `recall_task` leave it out) or steer it (`steer_task`
and `tell_task` refuse it, `users`). Its thread opens with that message as the
user's own (no `from: "host"`), in a new session that is told it is a thread the user opened and who the bot is, and
gets nothing of the chat. It is the bot's own work for the user everywhere else: it takes messages ([talking to a
thread](#talking-to-a-thread)), can be steered, stopped and tried again, and on the wire it has no `source`. **Its
result takes no turn of the bot's**: the user reads it in the thread. Only a bot starts one (not a group, not a
thread), with at most 32,000 bytes. `hello` says `startThread: true` on a host that has the method.

**Its files** (`attachments`: upload ids, `hello.threadFiles`): the app uploads them to the bot first (`upload` without
`taskId`; without a bot, without `botId`, which puts them in the computer's box of threads without a bot), and the
task keeps their `{id, name, size}` (`Task::files`). As its thread starts (`dispatch::begin`), each is copied into
the thread's own uploads and the opening message carries them, as a message the user writes to a thread does. Files
alone start one too, named by the first file; an id that was never uploaded starts nothing.

`setup` is what the thread runs with in place of the bot's own (`tasks::Setup`, kept on the task and applied to its
thread bot's config, `Setup::over`): `cwd` (a full path to a folder that exists), `model` (`{id, options}`: a model
with its own settings, `id` null for the agent's default), `approval` (`{permission, mode}`) and `loginProfile` (one
of the agent's login profiles, `""` its default login; left out, the bot's, or the default login of another
`backend`; a step of the bot's chain on that login leaves the thread's; anything else is refused as `unknown login
profile`; `hello.threadLogins: true` on a host that takes it). What it leaves out is the bot's; the bot's setup never changes, and its memory stays the bot's wherever the thread works. It is checked as a
bot's setup is (`hub::validate`), and one that can't be read starts nothing. A thread in another folder gets a
worktree there by the same rule as in the bot's.

A run of one of the bot's routines and another bot's `message_bot` request are tasks in the bot's box
(`sources.rs`), so the chat stays free while they work and they wait for a free thread under `maxThreads` like anything
else. A task's `source` says which: `routine` with the routine's id, or `request` with the asking bot's id (none
when it was asked from another computer or by the cloud assistant). A request and a routine's run start, like every
task, in a new session that knows nothing of the chat; they are told who the bot is
([how a thread's session begins](#how-a-threads-session-begins)). Their thread is told the routine's own
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

- **Stop** cancels the run (`Stopped by user`, no failure line) or tells the asking bot its request was stopped (a reply turn of its own).
  **Drop** on a waiting one does the same.
- **Try again** on a routine's task runs the routine again: a new run (with the failed run's event when it kept
  one, else as `runRoutine` does) and a new task in the old one's place. A request can't be tried again: the bot
  that asked has its answer.
- **They never give way** to a task that cuts in, and `steer_task replace` refuses them: started again they could
  act twice.
- **They never start twice.** Cut off by a host restart with a thread that can't resume, or lost (the hourly look), a
  run ends `interrupted` at once and its routine's retry rules decide, and a request fails. A request lives in the
  host process, so its task leaves the box at host start; one that hadn't started comes back as a new task, sent
  again from its notices, and one that had fails and wakes its sender ([bot collaboration](bot-collaboration.md#stops-failures-and-restart)).
- A run whose routine was paused or deleted while it waited leaves the box.
- Stop in the chat no longer cancels a routine's run that waits in the box, nor a request: the box's Stop does.

### Threads without a bot

`startThread` without `botId` starts a thread that runs an agent on its own (`work::plain`): `setup.backend` names
the agent (refused without one: `a thread without a bot names its agent`), and the app sends the folder the user
picked, the approvals and the model with it. With no folder picked it works in **a folder of its own**,
`<data dir>/threads/<task id>/` (`plain::own_folder`), made as the task is added and versioned as a bot's own folder is
(`agent::versions`: a git repository of its own, saved before and after each turn), so threads without a bot no longer
share the home folder. It counts as the thread's own folder (`workspace::own_folder`), so it never gets a worktree,
and it goes with the task (when the user's [`threadKeepDays`](#older-threads) deletes it, as its transcript; or when
the user drops the task). An app sends no `cwd` for it only on a host that says `hello.threadFolders: true`;
an older host gets the home folder, as before. Its task waits in the computer's **box of
threads without a bot**, a bot the host makes the first time one starts (`BotConfig::plain`, named `Agents`): always
hidden (`updateBot` can't show it, `createBot` can't make one), in the home folder, no connectors, skills, shared
memory or alerts, `maxThreads` 6, the harness's own memory kept. Apps that know it (`hello.threadsWithoutBot: true`)
keep it out of every list of bots and list its threads on the Threads page, under **No bot** when grouped by bot;
an app from before it lists it among the hidden bots. The reply names it (`{botId, taskId}`) and the task's wire row
carries `agent`, the agent it runs on.

Its thread is told nothing of Codync's (`plain` on the thread bot, copied from the box): no system prompt or
`<bot-profile>`, no MCP servers (its own tools only), no memory folders, no message ids, no `[Codync: …]` notes on
the user's messages; the user's words reach the agent as written. Its approval and choice cards and sign-in go to
the box's chat, which the Threads page shows in the thread, as for any thread. Everything else is a thread the user
started: it takes messages, can be stopped, steered and tried again, and reports to no chat.

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

## Where a thread works

A bot lives in its **own folder** (`cwd`, always `agent::workspace::path`: `~/.codync/bots/<id>/workspace`): its chat,
routine runs, group turns and calls start there, and it keeps its own files there, with versions. The user's folders
it sends work to are its **projects** (`BotConfig::projects`, `hub/projects.rs`): absolute paths of folders, each
once, the first its **default**. A new project must be a folder that exists; one saved before whose folder went
stays (an older app can't take it off). Groups, threads and the box of threads without a bot have none.

| Task from | Its thread's folder (`Task::folder`) |
| --- | --- |
| The bot (`add_task`) | `folder` if named, else the default project, else the bot's own folder |
| The user (`startThread`) | `setup.cwd` if picked, else the default project, else the bot's own folder |
| The user, without a bot | `setup.cwd` if picked, else [a folder of its own](#threads-without-a-bot) |
| A routine's run | The routine's `folder` if it has one, else the bot's own folder |
| Another bot's request | The bot's own folder |
| A task that follows another (`after`) | That thread's folder or worktree |

The folder is fixed when the thread starts (a task that waited picks up the default project as it is then), and a
thread keeps it until its task ends. The bot is told its projects in its instructions (the Workspace paragraph:
code work in a project goes to a task with that folder, a project's files are read with explicit paths, its own
folder never changes), and a change to them reaches a running session as a profile update. Readers that mean "the
project" read the default project (`workspace::work_folder`: a thread's own folder for a thread): the terminal tab,
a call's `Folder` line, the team directory's `cwd` and `start_dev_server`'s base.

**Whatever sets `cwd` to another folder** (an older app's folder picker, a managing bot's `folder`, a client's
`createBot`) puts that folder first among the projects instead, and the bot stays home
(`projects::settle`, under `Hub::settle_config`): no `minApp` raise. An edit that leaves `cwd` as it was takes
nothing in.

**Moving bots home, once** (`hub/projects/migrate.rs`, at host start before the routines and actors are read, kv
`projects.migrated`): a bot whose `cwd` is a project gets `projects = [cwd] + its own` and `cwd` its own folder.
Before anything changes, every config and routine it rewrites goes to `<data dir>/backups/bot-projects-<ms>.json`
(logged). Per bot: its uploads move from `bots/<id>/uploads` into its own folder's `uploads` (`uploads::root`
follows `workspace::is_managed`; one already there stays), each routine without a folder is stamped with the old
`cwd` (so its runs keep working there, in a thread; not on a bot whose routines run in its chat, computer use or
Manage the team on, which keep their tools and run at home), `sharedMemory` is stamped `false` where unset (it followed
`is_managed`), the chat owes a recap of the session it leaves (as a folder move does: `owe_move`, read by
`carry_in`), its sessions are let go, and the config is saved last. A failure is logged and the host starts; the
next start finishes it. Groups, threads and the box of threads without a bot are left as they are; a working
thread keeps its folder until its task ends. The harness's own memory and the Memory screen's Claude Code section
now read the bot's own folder, not the project's.

## Worktrees

Two threads in one checkout, or a thread and the user, would write over each other's files. The bot's
**`worktreeMode`** (`off | sideBySide | always`, `sideBySide` unless set; it matters only where a thread's folder is
in a git repository) says when a thread of the bot's own work or the user's gets a git worktree of its own
(`worktree::beside`, `worktree.rs`): **side by side**, when another thread of the bot works now in the same checkout
(`worktree::top`: the folder with `.git` above the thread's folder, as `git rev-parse --show-toplevel` says; a
thread in a worktree is in a checkout of its own, so it doesn't count); **always**, every one, the first included,
so the user's checkout is never touched; **off**, never. A bot's [own folder](bot-workspaces.md#versions) never gets
one: it keeps all its work in one place, with its versions; nor does a thread without a bot's own folder.

A task may **name its place** (`add_task`'s `place`, `startThread`'s `setup.place`: `here` | `worktree`, kept in
the task's `setup`), which wins over the mode (`worktree::wanted`): `worktree` gets one even in Off mode (outside a
git repository it works in the folder, logged), `here` keeps the folder even in Always mode or beside another
thread. The rest holds whatever the task asks: a routine's run and a request never get one, a task that follows
another stays in that one's folder, and the bot's own folder never gets one.

- `~/.codync/worktrees/<bot id>/<task id>`, on a new branch `codync/<title in lowercase, dashes>` (`-2`, `-3`… when
  taken), from the commit the thread's folder is on. The thread's agent starts there (in the same place of the
  repository as its folder, when that is a folder inside it), is told so with its task, and starts from a
  brief. Uncommitted changes of the user's checkout aren't in it.
- Side by side, **the first thread in a checkout keeps it**; a routine's run and a request never get a worktree; a
  task that follows another (`after`) goes on in that thread's folder, worktree or not.
- It is made by the thread itself as its first turn starts. When git fails (no commit yet, git missing, the folder in
  the way, a checkout hook that fails) or the checkout lacks the thread's folder (git doesn't track it: ignored, or
  never added), the thread works in its folder itself, and the failure is logged. A worktree git made and
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
  counts, though it is named after the earlier task; it goes once its last task left the box for the
  [archive](#older-threads), or its bot was deleted: `known`, `tidy_orphans`), once its commits are merged; one that isn't merged stays, as the user's work.
- **In the hourly look** at a box ([restart and cleanup](#restart-and-cleanup)) every kept worktree is looked over
  again the same way (`worktree::kept`, from `sweep`): one the user merged since goes, without a restart.
- A task that gave way, got a new brief or is tried again starts again in the worktree it had. While an ended
  task's worktree is being looked over (`Left::Settling`) the task tried again waits; the look notes kept or removed
  and runs the dispatcher again. A message to the task's thread meanwhile takes the worktree back as in use, which
  the look then leaves as it is.
- **Codync never merges, rebases, resets, forces or deletes a branch that has work on it**, and never touches a
  worktree it didn't make: what a box says is checked first (the folder is `~/.codync/worktrees/<id>/<id>`, the
  branch `codync/…`). Git runs with an argument list, never through a shell, each command for at most a minute.
- Dev servers, `send_file` and `publish_artifact` of a thread in a worktree start from the worktree.

### Filling a new worktree

A new worktree is a clean checkout: none of the main checkout's ignored files, none of the user's uncommitted
changes. Right after Codync makes one, before the thread's agent starts (`work/fill.rs`, from `worktree::enter`;
not for a worktree a task already had):

- **`.worktreeinclude`** at the top of the main checkout lists ignored files or folders to **copy** in (`.env`,
  `.env.local`, editor settings): one literal path a line, relative to the repository's top, `#` for a comment. A
  path that is absolute, leaves the repository, is missing, tracked or not ignored (`git check-ignore`, which also
  passes over tracked paths) is passed over, logged.
- **Shared folders**: per bot and project, the ignored folders to **link** into each new worktree (`node_modules`,
  `.cache`), so a thread doesn't fill them again: the bot's `sharedFolders`, `{<project path>: [<folder inside it>]}`,
  set with `updateBot` and kept to the bot's projects (a project taken off takes its list; a name must be a plain
  relative path, no `..`, kept without `./` or a trailing slash). Only the projects in the worktree's repository count, each at its place inside it. A link
  is a symlink on macOS and Linux and a folder link on Windows (it needs Developer Mode there). Each link is listed
  in the repository's `info/exclude` (`/node_modules`, no trailing slash, which a link wouldn't match), so git never
  shows it: a thread's `git add -A` can't commit it, and a merged link to the main checkout's own folder would
  replace that folder. The thread is told not to commit them.
- Nothing already in the worktree is replaced (a tracked folder of the same name stays as the commit has it). The
  user's uncommitted changes are never carried: the thread is told it started from the project's last commit, and so
  is the report. Each file or folder that can't be copied or linked is logged and counted (`worktree_fill_failed`,
  `stage: include | link`), never fatal.
- When the task is over, the links Codync made don't count as work: git passes over them in the look at
  `git status`, and each path `info/exclude` lists that is a link to the same path in the main checkout is removed (the
  link only) before `git worktree remove`, so an untouched worktree still goes. The copied files are ignored, which git's removal allows.

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

## Older threads

A box is read and written whole on every change, so a task leaves it a week after it ended (`KEEP_MS`, at host
start and once a day: `work::age`) for the **archive** (`work/archive.rs`, table `thread_archive`: bot, task, thread,
`endedAt`, the task's JSON). Its thread bot, once no task left in the box names it, is **parked** (table
`parked_threads`): out of `bots`, so the lists of threads every dispatcher pass and host start read stay short, with
its transcript, session (`session_id`, `read_rev`, put back as they were with the bot row), uploads and folder left
as they are. The tables are made beside the others (`Store::open`): nothing stored changes, so the schema stays. A
host from before kept tasks for `recall_task` in kv `work-past.<bot>`; they move into the archive once, at host start,
a thread a task in the box still names staying where it is. Most of their threads were already gone: those tasks are
only recalled, never listed.

- **Kept for good by default.** `threadKeepDays` (kv, per computer; `setThreadKeepDays {days}`, 0 or 7 to 3650,
  `hello.threadKeepDays`, a `threadKeepDays {days}` event to connected apps) deletes a thread that many days after it
  last ended: the archived task, its parked thread bot with its transcript (`Hub::delete_bot`), its own folder (a
  thread without a bot) and an untouched worktree. A thread that worked a later task stays with it. It runs at host
  start, once a day and as the choice changes. The `threadKeepDays` event goes out after the deletion, as the choice
  changes and whenever the daily look deleted some, and apps list their older threads again on every one, the same
  number or not. Writing to a thread reopens it and its clock starts again when it ends.
- **Listed.** `olderThreads {botId?, before?}` → `{threads, more}`: the ended threads of `botId`, else of every
  bot on the computer, that ended before `before` (the `endedAt` of the oldest one the app has paged; none for the
  newest), newest first, 30 a page, from the boxes and the archive; a page never splits threads that ended in the same
  millisecond (it takes them all), so the next one, before that `endedAt`, misses none. Each is a `work` row with `botId`; the newest overlap the bots' `work`, so an
  app keeps one row per bot and task. Apps list them at the end of Threads (the terminal: under Recent on a bot's
  Details page, with `botId`), loading the next page as the list scrolls to its end, on a host that sends
  `hello.threadKeepDays`. An ended thread that leaves its bot's `work` while the app is connected stays listed as it
  last read (desktop, iPhone, Android), without moving the next page's cursor.
- **Opened and written to** as any thread: `threadEntries` and `readUpload` read a parked thread, and `send`,
  `upload` (with `taskId`), `retryWork` and `dropWork` on an archived task first bring it back into its box with its
  thread bot (`archive::bring_back`, under the box's lock: the dispatcher never sees the thread without its task).
- **A deleted bot** takes its archive and parked threads with it.
- Analytics: `setting_changed` (`thread_keep_days`, `days`), `threads_deleted` (`count`, `days`),
  `older_threads_listed` (`count`, `more`, `first`); a failed archive is a host error.

## Wire

- Bot: `work: [task]`: every working task (oldest first), then the waiting in dispatch order, then the tasks ended
  in the last 24 hours (newest first, at most 20). Task: `{id, title, status, activity, thread, wait, cutIn,
  createdAt, startedAt, endedAt, result, talk, seat, folder?, runsOn?, source?, worktree?}`; `runsOn` the agent (backend id) its thread runs on now, else the one it ran on last: the fallback step it is on or ended on (as kept in `fallback.on.<thread>`), else the thread bot's own (`tasks::agent_of`); absent before a thread took it and on an older host. The desktop, iPhone and Android show its icon at the start of the thread's row on Threads (`runsOn`, else `agent`, else the bot's `backend`, none for the box of threads without a bot); the terminal shows no icons; `seat` whether it takes one of the bot's seats (a working task's thread; one waiting on the user lends its own, [the dispatcher](#the-dispatcher); never a thread the user started), absent on an older host, where every live task takes one; `activity` is its thread's, `thread` the thread bot that has or had it (null while it waits), `talk` whether the user may write to its [thread](#talking-to-a-thread) (the bot's own task whose thread bot is there), `folder` where its thread works (the worktree's folder for one in a worktree, else the folder its thread began in; absent before a thread took it, and once its worktree was removed with nothing in it), `wait`
  is `{kind: next | behind | after | usage, ahead?}` on a waiting task and null otherwise, `result` at most 280
  characters. `source` is `{kind: routine, id: <routine id>}` or `{kind: request, id?: <asking bot id>}`, and absent
  on a task the bot added, so a client that doesn't know it shows the row as before. `worktree` is
  `{branch, path}` while the task has a [worktree](#worktrees) that is still there, and absent otherwise.
  `background` is `[{id, kind: shell | workflow | monitor | other, name, activity?, startedAt}]` while its thread's
  agent [reports work running in the background](#background-work-its-agent-reports), and absent otherwise; a
  client reads a kind it doesn't know as `other`. The bot's own `status` stays the chat's.
- Cards and messages a thread posts in the chat carry `data.thread = {id, title}` (the thread's id, the title of the
  task it works on) and `data.task` (the task's id, or null): permission cards,
  choice cards (`data.decision`), agent sign-in cards (`data.connectionRequest`, `kind: agent`), failure notices that
  offer **Add a fallback** (`data.addFallback`, without the `stopReason` of the thread's own), file messages
  (`data.files`) and artifact cards (`data.artifact`).
- Bot: `maxThreads`, set with `updateBot` (1 to 6).
- Bot: `threadAgent` and `threadFallbacks` ([Advisor mode](#advisor-mode)), set with `updateBot`. Absent on an older host.
- Bot: `outside: {status, startedAt}`: how the bot stands [outside its chat](#outside-the-chat) (`idle`, `working`,
  `needsInput`, `error`), where `status` is the chat's alone. Absent on an older host: `status` then.
- Bot: `projects` (strings, the first the default; `createBot` / `updateBot`, the whole list), and `projectGitRepos`
  (bools in the same order: which projects are in a git repository, where the worktree mode matters and a client
  shows it). `cwd` is always the bot's own folder (`managedWorkspace` true). `hello.projects: true` on a host that
  has them; absent on an older host.
- Bot: `worktreeMode` (`off | sideBySide | always`; `updateBot`). For older apps also `threadWorktrees` (true unless
  `off`; sending `true` turns `off` into `sideBySide` and keeps `always`, `false` sets `off`, and `worktreeMode` wins
  when both are sent) and `gitRepo` (the default project is in a git repository; false without projects).
- Bot: `gitInstalled`: whether the host can run git (`git::installed`: the answer of `git::program`, kept for the
  process and looked up again in the background at most every five minutes, so a snapshot never waits for it; true
  until the first look ends, which the host starts as it starts). Absent on an older host: read as installed. With
  `gitRepo` it gives the line under the folder in Settings: `Git repository`, `Not a git repository`, `Git isn't
  installed on this computer`. The manual's page for it: `/docs/working-folders` (`#without-git`).
- `threadEntry` events `{rev, thread, entry}` for a thread's transcript; none in the catch-up.
- Bot: `sharedFolders` (`{<project path>: [<folder>]}`, `updateBot`; [filling a new worktree](#filling-a-new-worktree)).
  `hello.worktreeFill: true` on a host that fills new worktrees.
- `hello.threadFolders: true`: a thread without a bot that names no `cwd` gets [a folder of its own](#threads-without-a-bot).
- `hello.threadKeepDays` (0: never), `setThreadKeepDays {days}`, the `threadKeepDays {days}` event and
  `olderThreads {botId?, before?}` → `{threads: [task + botId], more}`: [older threads](#older-threads).
- `startThread {botId?, text, attachments?, setup?: {backend?, loginProfile?, cwd?, place?, model?: {id, options}, approval?: {permission, mode}}}` →
  `{botId, taskId}`: [a thread the user starts](#other-sources), without `botId` [without a bot](#threads-without-a-bot).
  `setup.place` (`here` | `worktree`, `hello.threadPlace: true`) is where it works in a git repository, over the
  bot's [worktree mode](#worktrees); an older host ignores it.
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
  reads the old names, not even `updateBot`. Schema 14 (`worktree_modes`) turns a stored `threadWorktrees` into
  `worktreeMode` (`false`: `off`, `true`: `sideBySide`); only the wire keeps the bool, for older apps.
- **The box** (kv `work.<bot>`): a task's `thread` also reads `task`, and a kept report's `task` also reads `item`
  (`work/tasks.rs`, `Report` in `work/mod.rs`); the box is saved under the new names at its next change.
- **Entries a thread put in its bot's chat**: schema 9 (`migrate_9` in `store/schema.rs`) rewrites each stored entry
  whose `data.task` is an object: that object becomes `data.thread`, `data.item` becomes `data.task`, and `data.item`
  is removed. Revs stay as they were; `hello.schema` is 9, so a client that keeps a synced copy fetches the
  conversation again.
- **Thread ids**: a new thread is `thread-<uuid>`. A thread started before the update keeps its id `task-<uuid>`
  (`store::is_thread_id` knows both), since its transcript, session and cards are keyed by it; it goes when its task
  is deleted ([older threads](#older-threads): only as the user's `threadKeepDays` says, never by default).

## In the apps

The capsule, the bot's Details (Threads, Event box, Later, Routines, Dev servers, Recent), `Threads at once` in its
Settings and cutting in are specified in [the event box](../plans/event-box.md#in-the-apps) (stage 2). The sheet of every bot's work, the rows' source marks, the `Work · <title>` line on what a thread posts, the worktree setting (now **Worktree mode**, [where a thread works](#where-a-thread-works)) and what follows `bot.outside` are in [as built, clients](../plans/event-box.md#as-built-clients-stages-3-to-8). A task's thread view, the where line on a row and in the thread's header, and the git state in Settings are in [stage 11, as built on the terminal](../plans/event-box.md#as-built-host-and-terminal-stage-11) and [on iPhone, Android and desktop](../plans/event-box.md#as-built-iphone-android-and-desktop-stage-11). A client reads `bot.work` and `bot.maxThreads`; both are
absent on an older host, where the feature doesn't show.

## Restart and cleanup

At host start (`work::restore`): tasks ended over 7 days ago leave the box for the [archive](#older-threads), their
thread bots parked, and the archive drops what the user's `threadKeepDays` no longer keeps; a working task whose thread's turn was cut
off and can be resumed (`agent::bot::resumable`: a restart mark under an hour old) stays working, and its thread's
actor resumes the turn; one whose thread can't goes back to waiting once, as a task that was cut in on, and fails with
a report the second time. An ended task's undelivered report is sent again: to the bot, or, for a routine's run,
as its message in the chat (posted once, by its entry id) and its saved outcome. A run the box no longer holds is
queued again if it waited, and ends `interrupted` if it ran. Thread bots whose bot is gone, or that no
task names, are deleted. Deleting a bot deletes its box and its threads, and cancels the requests it hadn't answered.

The hourly look (`work/sweep.rs`) catches what no event said. After every dispatcher pass that leaves waiting or
working tasks, or a kept [worktree](#worktrees), in a box, one timer per bot is set for an hour later (reset by the
next pass, none for an empty box). It also looks each kept worktree over again.
When it fires, the host takes a dispatcher pass again, and a working task whose thread runs no turn (its actor is
gone or idle, and nothing is about to resume) follows the restart rule: back to waiting once, then failed with a
report (`work_finished` `error: lost`). A turn that runs is never touched, however long and quiet (the
[watchdog](#watchdog) asks the bot to look at one that seems stuck). The look is host code only: no agent is woken
unless a task was lost.

## Watchdog

Every 5 minutes (`work/watch.rs`: one host-wide timer, started by `work::restore`), host code reads the running turn
of each thread that works on one of a bot's own tasks: its newest 50 entries since the turn began (its runtime's
`startedAt`), and its cards in the bot's chat (`Store::open_cards`). A routine's run and another bot's request
aren't watched: their result has an owner other than the bot's chat. What it flags:

- **looping**: the turn's last 6 tool calls are one call with one result: the same title, output, diffs and
  locations (a title can leave the arguments out: an MCP tool's is its name). Calls with no title of their own
  (`Tool`) never loop, nor does a call run again whose output changes each time (a timing, a temporary path).
- **failing**: its last 5 tool calls ended `failed`.
- **quiet**: no entry of the turn was created or changed for 60 minutes, while no tool call is `pending` or
  `in_progress` (a long build isn't stuck). The hour counts from when the computer last woke, if that is later: a
  look that comes long after the one before means the computer slept.
- **card**: an approval or choice card of the thread has waited on the user for 2 hours. The user gets one reminder
  per card: the needs-you alert (`AlertKind::NeedsInput`, opening the bot's chat; none when the bot's alerts are off).

A thread that waits on the user (a card, a sign-in) is never flagged otherwise, nor is one between turns.

**The wake-up.** On `looping`, `failing` or `quiet` the bot gets a host turn in its own chat (`Cmd::Watchdog`,
`HostTurn::Watchdog`), queued behind the user's messages as a report is: `[Codync watchdog] Task "<title>" (<id>)
looks stuck: <the repeated call, the failed calls' titles, or the minutes without a step>`, asking it to look
(`work_status`, `recall_task`) and do one thing: `tell_task` what to change, `steer_task` it (stop, or replace the
brief), or reply `(pass)` if it is fine. A task that ended or left that thread before the turn comes drops it.
`(pass)` leaves nothing in the chat and alerts no one; another answer alerts like a reply. A task the turn adds
carries the stuck task's hops on, as a report's turn does. A host restart doesn't resume it; the next look asks again.

**Limits.** A thread wakes its bot at most once an hour (kept in memory, so a host restart forgets it). No wake
while the chat's own agent rests (`agent::fallback::rest`, its login) or is near its usage limit
(`usage::near_limit`, as the dispatcher counts it); the next look tries again. A Stop in the chat drops a wake-up
that waits there, and a newer one for the same thread takes the place of one still waiting (the chat was busy for
over an hour).

**The host itself** (`host/src/watchdog.rs`). Under the service (`service::started_by_service`), a beat goes through
the runtime and the hub's emit lock every 10 seconds, and an OS thread outside the runtime looks every 30. No beat for
5 minutes (every worker blocked, a lock never freed) logs, writes `watchdog.restart` in the data folder and aborts
the process; the service starts it again (launchd `KeepAlive`, systemd `Restart=always`, `codync-hostw` on Windows),
whose start reports `host_restarted_stuck` and keeps the time as `watchdog.last`. A stall within 30 minutes of that
restart only logs, so a hang that returns at start can't restart the host, and resume its turns, every few minutes.
A nap of the thread that runs long by the wall clock means the computer slept: the wait starts over.
`CODYNC_WATCHDOG_STALL_SECS` sets the stall and turns the watchdog on outside the service; a debug build with
`CODYNC_WATCHDOG_TEST_HANG` stops beating (`watchdog_e2e`).

**Analytics.** `watchdog_flagged`: `signal` (`looping`, `failing`, `quiet`, `card`), `action` (`woke`,
`reminded`, `skipped`) and, when skipped, `reason` (`cooldown`, `rests`, `near_limit`); a skip is counted once
per thread and reason in a row. Never a title or text. The bot's turn is `turn_finished` `source: watchdog`.
`CODYNC_WORK_WATCH_SECS` sets the period for the end-to-end test (`work_e2e`).

## Limits of this version

- A thread never changes or takes out a line of memory, the bot's own tasks included, also in a turn the user's
  message to the thread started. What the user asks the bot to forget, the bot does in the chat.
- A thread takes text and files, but no quoted message, voice or command. A message sent right as a host restart
  comes is marked failed and has to be sent again.
- A waiting message's files stay with the thread a task leaves (it gave way, or got a new brief): the task starts
  again with the message's words and the names of its files, not the files.
- A turn that ended with a message of the user's still waiting reports nothing of its own: its last message stays
  in the transcript, and the report is the last turn's.
- No subagent rows. A workflow shows its name and its latest activity, not its phases and agents.
- Background work an agent reports needs Claude's adapter (`async_task_*`); other agents' background work is only
  what [background work left running](#background-work-left-running) sees.
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
  shows among the bot's [dev servers](dev-servers.md). Only some of that holds its task when the turn ends
  ([background work left running](#background-work-left-running)): a background shell a harness keeps in its own
  session, another adapter's in-process background work, and on Windows any process aren't seen, so a held turn,
  or one a waiting message took first, that ends again while it runs is done, not failed. A detached
  process meant to stay that doesn't listen (a watcher, a daemon) holds its task once, then fails it.
- A thread ends as soon as its task does when nothing waits to follow it, so `after` is for work that is still
  working or waiting.
- A routine's run in a thread has its `send_message` refused as before, and what it wrote reaches the chat when it
  ends.
- A thread on a routine's run or a request can't change the bot's profile or fallbacks, which that turn could in the chat.
- A typed message never answers a thread's choice card; a client without buttons can't answer one.
- A thread's line for what it shared finds that entry only among the chat's messages the client has loaded,
  as a needs-you row does; an older one shows no line, and the line's tap leaves the chat where it was.

Tests: `host/tests/work_e2e.rs` with `tests/fixtures/work_agent.py` (a brief with nothing of the chat, a card with `data.task` answered
from the chat, refusals in a thread from each server, the ceiling and the order, first, drop, cut in from the user and
from the bot, replace on a waiting and on a working task, `after` in the same session, stop one and all, retry, an
agent that can't start, a stopped turn dropping the task it added, a deleted bot taking its box and threads, a host
restart mid-task, cut in on a task that follows working one closing nothing, no `bot` or `entry` event for a thread
bot; a routine's run as a task: its source on the wire, waiting for a free thread, its message posted once in its line's
turn with no turn of the bot's, `(pass)`, its progress note, the refusals of a run, stop, a deleted routine's waiting
run, try again, never giving way, an agent that can't start, a host restart mid-run; another bot's request as a
task: its source, the notices, the reply turn of the asking bot, the refusals of a request, stop, no try again; a
thread's choice card answered by a press and not by a typed message, ended by stopping its task and not by the chat's
Stop, its file and artifact in the chat and kept with the bot; a run that never sees the chat, a request's
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
chat and its task going on in the same thread once signed in, a thread's offer to add a fallback in the chat and new fallbacks reaching a running thread, a thread
that starts on a fallback never forking the chat's session, a thread moving past a signed-out agent told its own work and not the chat; the watchdog's turn in the chat for a thread that makes one
call again and again, once an hour, and its `(pass)` leaving nothing; `recall_task` and `tell_task` refused in a
thread); `watchdog_e2e` (a hung host aborted with its mark, the next start reporting it, a second stall within 30
minutes only logged); `memory_files_e2e` (a line a
routine's run in a thread takes out with a shell command is put back); unit tests of a worktree made, removed, kept,
removed once merged and never touched when it isn't Codync's, the kept ones looked over again at a restart
(`recheck`), the walk that removes only merged worktrees no task knows (`tidy_orphans`), and an inherited worktree
known after its earlier task is gone (`known`) (`worktree.rs`), how a bot stands outside its chat (`tasks.rs`), a box stored before the rename read and saved under the new names (`tasks.rs`, `store/model.rs`) and schema 9's rewrite of stored entries (`store/schema.rs`), unit tests of the dispatch order and the `wait` reasons
(`dispatch.rs`), the watchdog's signals (looping, failing, quiet, a card overdue), its hourly hold and its reasons,
only the bot's own tasks watched, a wake-up dropped once its task moved on and its analytics (`watch.rs`), the host
watchdog's verdict, a computer that slept and the mark reported once (`watchdog.rs`), `recall_task` by id, query and
latest, its pages of steps and 30 days (`recall.rs`), `ask_chat` refused outside a handed-out task and while a question waits, its answer to the waiting call or passed on later, and its end after the wait or with its task (`ask_chat.rs`), a task ended over a week ago leaving the box for the archive
at host start (`mod.rs`), the archive, parked threads coming back in their session, `threadKeepDays`, paging and an
older host's past tasks (`archive.rs`, `store/archive.rs`), the terminal's Delete old threads choices and older pages
(`tui/app/thread_keep.rs`, `tui/manage/older.rs`), the user's words a dropped turn gave back heard once (`thread.rs`), the agent a new thread starts on (`agent/fallback/mod.rs`), a thread's held message sent again however
old and not once its task ended (`market/agent_sign_in.rs`), a task's end and the restart rule, also for a run and a request (`mod.rs`), a source on the wire (`tasks.rs`), which bots keep runs in the chat (`sources.rs`), the 20 waiting tasks and the title and brief lengths (`tools.rs`), the prompts and the box's lines. Not
covered by a test: a reopened task's kept worktree becoming its folder again and a removed one's thread moving to the bot's folder end to end (the unit test covers the folder), a message to a task whose thread is on a task that followed it, a message that arrives as the task's turn ends (the lock that orders them), a real agent resuming a reopened session, a replaced brief carrying what waited, a worktree reused by a task that starts again, one inherited by a task that follows, a
stopped task's worktree looked over, a task stopped while it waits for a sign-in ending its thread, a worktree's dev server and artifact, git failing, a real agent in a worktree, the host-start reclaim end to end (`tidy` spawning
`tidy_orphans`, `pick_up` settling what `recheck` took),
the Live Activity push itself (the relay isn't in the tests), a routine's time limit in a thread, a lost run or request at the hourly look, a run the box lost at host start, a request from another computer or the cloud assistant as a task, `message_bot` from a run's thread, a thread's voice note, a choice card's timeout and its end at a host restart, the usage and in-a-row limits, the desktop's Add a fallback opening Settings, `add_task` refused in a room or a bot's request (a routine run is: `routines/tests.rs`), a report turn that fails, a
thread's dev server, a task sent back to waiting by a restart end to end, a task moving to the past ones end to end,
`recall_task` end to end, a Stop dropping a waiting watchdog wake-up, one dropped because its task ended before its
turn end to end, a newer wake-up taking a waiting one's place, the card reminder's push, a thread's message given back to its task
by a dropped turn end to end (`requeue_unheard` in a thread), and the box's lines in a
known harness's prompt.
