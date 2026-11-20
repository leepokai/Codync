# Event box

Status: **being built**, 2026-11-15, on `feat/background-tasks`. Builds on background tasks as
merged in 2.48.0 ([its first design](../archive/background-tasks-plan-2026-11-15.md)): what was "one task, a second
refused" becomes a queue.

Everything a bot should work on away from its chat is a **task** in a durable box. A bounded number of the bot's
threads take tasks from it. The owner's decisions are marked *(owner)*.

## Words

- **Coordinator**: the bot's main conversation. It talks to the user, decides what is worth a task, and tells the
  user what came of it.
- **Task**: one piece of work in the box: a title, a brief, the user's own words, a status.
- **Thread**: where the bot works tasks away from the chat, one at a time, with a session of its own: a thread bot
  ([work](../features/work.md)). A task names the thread that has it.
- **Box**: the bot's tasks. It is the source of truth: a thread that dies leaves its task there.

## A task

| Status | Meaning |
| --- | --- |
| `waiting` | In the box; no thread works on it |
| `working` | A thread works on it |
| `needsYou` | Its thread waits on an approval |
| `done` | Finished; `result` is what its thread reported |
| `failed` | It could not be finished; `result` says why |
| `stopped` | The user or the bot stopped it |

These six are the only words for work anywhere: the wire, every client, the bot's tools.

Record (kv `work.<bot>`, one JSON list per bot, replacing `task.<id>`): `id, title, brief, words, notes (what the
user said about it since, with their words), status, order, after (a task it should follow in the same session),
thread (the thread bot that has or had it), cutIn (it gave way once), hops, createdAt, startedAt, endedAt, result,
report (until the coordinator's turn for it has ended)`.

## What goes in

*(owner)* The user talks to the coordinator; when it judges the request worth it, it adds a task. Short things it
answers itself. A run of one of the bot's routines and another bot's request are tasks too (stage 3): the host adds
them, and their result goes to the chat as the routine's message or back to the asking bot, not through the
coordinator.

## The dispatcher

*(owner)* The user sets one number per bot, **threads at once** (`maxThreads`, 1 to 6, 3 unless changed).
*(owner, 2026-11-15)* There is no limit for the computer as a whole: each bot's own number is the only ceiling. Within it the dispatcher decides how many run: the number is a ceiling, not a target.

One function, `work::dispatch(hub, bot)`, run whenever the bot's box or one of its threads changes (a task added,
moved, dropped or retried; a thread's turn over; the coordinator's turn over; the setting changed; host start). For
each waiting task, lowest `order` first:

1. **It follows a task** (`after`) whose thread is alive and idle (its last task ended): that thread takes it, in the
   same session. If that thread is working, the task waits for it. If that thread is gone, the task is an ordinary one.
2. **Otherwise a new thread starts for it** when the bot is under `maxThreads` and the agent is
   not near its usage limit (near it, only when none of the bot's threads runs).
3. **Otherwise it waits.** Nothing is refused for being over the ceiling.

A task added during the coordinator's turn starts when that turn ends (a fork taken mid-turn would carry an
unanswered tool call), as tasks do today. A thread with no task left ends: its session is closed and its process
stopped; its transcript stays for its tasks' details. The dispatcher never starts a thread only because the bot is under its number.

Why a task waits is on the wire (`wait`): `next` (it starts when a thread is free), `behind` with how many are
ahead, `after` (it follows a working task), `usage`.

## Steering

*(owner)* Three places to cut in; the box's own is only about order.

| Where | What it does |
| --- | --- |
| The chat with the coordinator | As today: a message sent while it answers waits above the composer, and can be sent now or removed. |
| The box | **First**: a waiting task moves to the front. Order only. |
| A running thread | The user writes to the thread of the bot's own task as to a chat (stage 11; before it, a thread's session was never written into). To put a task on a busy bot **now**, a thread gives way: its work is closed and a new session starts for the task. |

**Cutting in** (`now`), when every thread is busy: the user picks which working task gives way (or the coordinator
names it; with none named, the one that started last). That thread is stopped and deleted, its task returns to the
front of the box with `cutIn` set, and starts again later in a new session from its brief and notes. A new thread
starts the task that cut in. With a thread free, `now` is just `first`; so it is for a task that couldn't start
on the freed thread (it follows unfinished work, or waits for usage): no session is closed for it, and the clients don't
offer Cut in on such a row.

**Changing a task** (`replace`): its brief is replaced. Waiting: nothing else. Working: its thread's work is closed
the same way and the task starts again from the new brief.

**More for the same session** is not steering: it is a new task with `after`, which that thread takes when it has
finished what it is on.

## Tools (on the `chat` MCP server, replacing the task tools)

- `add_task {title, brief, after?}` → `{taskId, status}`. Refused outside the bot's own chat, in a thread, past 20
  waiting tasks, and for the ninth task added in a row with no message from the user.
- `steer_task {task, how: "first" | "now" | "replace" | "stop", brief?, insteadOf?}`. The user's own words of the
  running turn are added to the task's `notes`.
- `work_status {}` → every task that is live or ended in the last day: status, what it does now, seconds, its
  result.

`work::EACH_TURN` and `work::BRIEF` name `add_task`. Each of the coordinator's turns in its own chat also carries
the box in a few lines (counts, then the live and waiting titles with their ids), so it can answer "what is it
doing?" and place an `after` without a tool call.

A thread's first message is the task (title, brief, words, notes). A thread's last message of a turn is its task's
result and report: `Cmd::TaskReport` as today, one per task, reports that arrive together taken in one turn.

## API (clients)

`workFirst {botId, taskId}`, `workCutIn {botId, taskId, insteadOf}`, `dropWork {botId, taskId}` (a waiting task, or
one that ended), `stopWork {botId, taskId?}` (one working task, or all; replaces `stopTask`), `retryWork {botId,
taskId}` (a stopped or failed task goes back to waiting, last). `threadEntries {botId, threadId}` and `threadEntry` events
stay: a thread's transcript, by the task's `thread`.

## Wire

- Bot: `work: [task]`, replacing `tasks`: every live or waiting task in dispatch order, then tasks ended in the last
  24 hours (newest first, at most 20). Task: `{id, title, status, activity, thread, wait: {kind, ahead?}, cutIn,
  createdAt, startedAt, endedAt, result}` (`result` at most 280 characters; `activity` is its thread's).
- Bot: `maxThreads`. Set with `updateBot`.
- Absent on an older host: the feature doesn't show.

## In the apps

*(owner)* The chat stays as it is: messages and what needs the user. The capsule is the one sign of work. The bot's
inspector changes so that **Details is what the bot does** and **Settings is how it is set up**.

**Capsule** (above the composer of the bot's own chat, while any task is live or waiting):
`Work · 2 working · 3 waiting`; `Work · Needs you` first when any task needs the user,
with the counts after it. One working task and nothing waiting: `Work · <its activity or title>` and
the time, as today. It opens Details. Its Stop (one task working) asks once, then `stopWork`.

**Details** (the inspector's first tab; the phones' bot details), top to bottom, empty sections left out:

| Section | Rows |
| --- | --- |
| **Threads** | *(owner: the running threads and the box are told apart.)* The threads that run now. The heading carries `2 / 3`: how many run, of the most allowed. Tasks that need you first (the attention mark, what the card asks; opens the chat at the card), then working tasks (a moving dot, the thread's activity, elapsed time). Both kinds of row have Stop. A working row opens its thread (stage 11; the thread's transcript before it) and says where it works. A thread the bot may still start is one faint row, `Free`, so the ceiling is visible. |
| **Event box** | What waits for a thread, numbered 1, 2, 3 in the order it will be taken, with why it waits. The heading carries how many. **First** on every row but the first; **Cut in** on the first row when every thread of the bot is busy (not on a row that follows unfinished work or waits for usage), which asks which working task gives way; **Drop**. Left out when nothing waits. |
| **Later** | Follow-ups the bot set (the dashed card, as today). |
| **Routines** | As today, with **+**. |
| **Dev servers** | As today, when the bot has some. |
| **Recent** | Tasks ended in the last day: done (a ring, time taken; opens its thread), stopped and failed (the danger mark, why; **Try again**). |

**Settings** (the inspector's Settings tab; the phones' bot settings) gains what Details held: **Instructions** and
**Agent** (the agent, model, fast mode, fallbacks) go first, then Profile, Workspace (with **Threads at once**, a
1 to 6 stepper), Activity, Connectors, Skills.

**Cut in** asks with the client's dialog: `Let "<title>" cut in`, `Every thread is busy. Pick the task that gives
way: its thread closes and the task starts again later.`, one row per working task (title, what it does, how long), `Cancel`.

**Roster row**, the most pressing thing first: needs you; the chat's own activity; `2 working · 3 waiting` (either
part alone when the other is zero); else the last message.

**The same on every client**: a task whose status the client doesn't know is left out; a waiting task with no
reason reads `Waiting`; a host that sends no `maxThreads` gets no stepper, and the client counts 3 for whether every
thread is busy; the Threads section shows only while the bot has a live or waiting task (an idle bot's Details has
no `Free` rows), and its heading says what is true (`2 / 1` right after the ceiling was lowered).

**Terminal**: the line over the composer says the same as the capsule; `B` opens the bot's Details page, which lists
the same sections; on a row `↵` opens the transcript (the terminal's thread view), `X` stops (asks), `F` moves first, `C` cuts in (asks which),
`D` drops, `A` tries again. Agent and Instructions are on its Settings page. `E` from the roster opens the page of
every bot's work (stage 4).

**Words** (English key = Traditional Chinese): `Threads` Threads, `Event box` Event Box, `Free` 空位,
`Later` 之後, `Recent` 最近,
`{count} working` {count} 進行中, `{count} waiting` {count} 等待中, `Next up` 下一個, `{count} ahead` 前面還有
{count} 件, `Follows what it works on` 接在進行中的事後面, `Near the usage limit: one at a time` 用量接近上限，一次只跑一個
(stage 11; `Waiting for usage` 等用量恢復 before it), `Was cut in on; starts
again` 被插隊，會重新開始, `First` 先做, `Cut in` 插隊, `Drop` 丟掉, `Try again` 再試, `Threads at once` 同時幾個
thread, `Let "{title}" cut in` 讓「{title}」插隊, `Every thread is busy. Pick the task that gives way: its thread
closes and the task starts again later.` 所有 thread 都在忙。選一個讓位的 task：它的 thread 會關掉，task 之後重新開始。 The strings of 2.48.0 that still
apply are reused; `{count} tasks running` goes.

## Restart, failure, limits

- At host start: a working task whose thread resumes stays working; one whose thread can't goes back to waiting once
  (as a task that was cut in on, from its brief), then fails.
- A thread that fails fails its task with the reason; the coordinator hears it in a report.
- Every hour while a box holds waiting or working tasks, the host looks it over again (`work/sweep.rs`): the
  dispatcher takes a pass, and a working task whose thread runs no turn waits again once, then fails. A turn that
  runs, however long and quiet, is left alone. The hour counts from the box's last change; an empty box sets no
  look. Nothing here wakes an agent unless a task was lost.
- Stopping a working task stops its thread; tasks waiting `after` it become ordinary waiting tasks.
- Tasks ended over seven days ago are deleted, and a thread bot with no task left is deleted with its transcript.
- Chains: tasks added from a report's turn carry its hops; the ninth is refused.

## Analytics

`work_added {outcome: added | elsewhere | invalid | full | hops | failed, after, source}`, `work_finished {outcome,
agent, start, forkFailed, minutes, error, source, worktree, kept, worktreeFailed}` (`source`: `bot`, `routine`, `request`), `work_steered {how, by: bot |
user}`, replacing the `task_*` events. Screen `thread`
(sent as `task` before the rename) is the task's thread view (a thread's transcript before stage 11). A thread's `turn_finished` carries `work`
(its task's source), which is where a task's failure gets its reason (the agent's error, a sign-in, a rate limit);
the box's own failures that should never happen (a box that won't save, a thread that is gone) go to
[error tracking](../features/analytics.md#error-tracking).

## As built (host, stage 1)

Where the host differs from, or settles, the text above ([what is built](../features/work.md)):

- **A thread ends as soon as its task does** when no waiting task follows one of its tasks. So `after` joins a session
  only while what it follows is working or waiting; `add_task` refuses an `after` that names an ended task.
- **Tasks that follow go first** in a pass, before the ordinary ones in `order`: their thread already runs,
  so the ceiling holds.
- A task whose `after` task failed, was stopped or was dropped is an ordinary task, as when its thread is gone.
- **Replace on a working task** puts it at the front without setting `cutIn` (nothing cut in on it).
- **`stopWork` without a task** stops the waiting tasks too, or they would start on the freed threads.
- `workCutIn`'s `insteadOf` is optional, as in the tool: the working task that started last gives way.
- Stopping the bot's turn drops the tasks that turn added, as it dropped the task it started in 2.48.0.
- The restart rule keeps its own mark on the task (`restarted`), so a task the user cut in on still gets its one
  restart.
- `work_steered`'s `how` also takes `drop` and `retry`, from the user's `dropWork` and `retryWork`.
- `threadEntries` answers `thread: {id, title, status, startedAt, endedAt, start}` for the task the thread works on, else
  the last it worked on.
- `wait` is null on a task that isn't waiting. `maxThreads` is on every bot row, groups included (unused there).

## As built (host, stages 3 and 5)

What is built is in [work](../features/work.md#other-sources); where it settles or differs
from the text below:

- **Wire**: `work[].source = {kind: routine | request, id?}`, absent on the bot's own task. `id` is the routine's
  id, or the asking bot's (absent for a request from another computer or the cloud assistant). A routine row's
  title is the routine's name; a request row's is the request's first line (80 characters). No new API method.
- **A bot with computer use or Manage the team on keeps its routine runs and requests as turns in its chat**
  (`work::boxed`): a thread has neither server, so a run that drives the screen or sets up a bot would break.
- **A run or a request is never started twice**: it doesn't give way to a cut in (`workCutIn` with it as
  `insteadOf` is refused, and it isn't picked when none is named), `steer_task replace` refuses it, and the
  restart rule and the hourly look end it at once (a run `interrupted`, so its routine's retry rules decide; a
  request failed) where the bot's own task waits again once.
- **`retryWork`** on a routine's task runs the routine again (a new run, a new task, the old row gone); on a
  request's it is refused. **`stopWork` / `dropWork`** cancel the run or the request. `workFirst` is as for any task.
- **Stop in the chat** stops neither: they are the box's now.
- **A routine's run in a thread starts in a new session with only its own prompt**, as a run in the chat does: no
  fork and no chat so far, so it costs and sees what it did before (corrected after stage 3 first forked it). A
  request starts like any task.
- **A request answered with exactly `(pass)`** gives the asking bot those words, as a turn in the chat did.
- **What those turns could do in the chat holds in their thread**: `request_decision`, `send_message`, `send_file`
  and a request's artifact cards refused (a run's artifact card goes into the bot's chat); the `routines` tools
  (and, from a run, `message_bot`) work as the bot. Stage 5's
  asking and sharing is for the bot's own tasks.
- **A thread's choice card is answered by a press only**: a message the user types is the coordinator's, so it neither
  answers the card nor ends it; the chat's Stop doesn't end it either (stopping the task does). The cases are
  listed in [asking and sharing from a thread](../features/work.md#asking-and-sharing-from-a-thread).
- **A thread's file and artifact are the bot's** (its files, its Library); only the card or message carries the task.

## As built (host, stages 6 to 9)

What is built is in [work](../features/work.md) ([memory](../features/work.md#memory),
[worktrees](../features/work.md#worktrees), [outside the chat](../features/work.md#outside-the-chat))
and [voice calls](../features/voice-call.md#how-the-voice-talks); where it settles or differs from the text below:

- **6.** A progress cue for a task reads `<bot>, in the background on "<title>": <activity>`; the chat's own turn
  and each task at work are said when what they do changed, one line every 15 s. A choice card (`request_decision`)
  is a cue too, the chat's own as well as a thread's; a thread's says it takes a press in the chat.
- **7.** Wire: `bot.threadWorktrees` (true unless set, `updateBot`), `bot.gitRepo`, `work[].worktree = {branch,
  path}` while it is there. **A thread in a worktree starts from a brief, not a fork** (no agent is checked to honor
  another folder on a fork). The worktree is made by the thread as its first turn starts, not by the dispatcher, so
  no git runs under the box's lock. A task that follows another stays in that thread's folder; one that starts again
  reuses its worktree. A stopped task's worktree is looked over when its thread's turn has ended, with no report.
  Git runs through `tokio::process` with an argument list and a one-minute limit per command, not on a blocking
  thread: the limit can stop it. `work_finished` gains `worktree`, `kept` and `worktreeFailed`.
- **8.** The host builds no line: the Live Activity and the ongoing notification get the status and a start time
  only (the relay passes nothing else), so the host's part is the lifecycle. Working means the box holds a task
  that is working or waiting, or one whose report the chat hasn't taken. The same status is `bot.outside =
  {status, startedAt}` on the wire. No alert was added or removed.
- **9.** Keyed by the acting bot or thread, not by session id: each has one turn at a time, and the turn records its
  session. A thread's turn is never the user's own, so a thread adds to memory and never takes out, for the bot's own
  tasks too.

## As built (clients, stages 3 to 8)

The same on iPhone, Android, desktop and terminal unless said; where it settles or differs from the text below:

- **3.** Before a row's title: the routine symbol (`clock.arrow.circlepath`; terminal `↻`) for a routine's run, the
  asking bot's small avatar (terminal: its name and `›`) for a request. A request from a bot the client doesn't
  have (another computer, the cloud assistant, a deleted bot) has no mark. **Try again** isn't offered on a
  request's row. **Cut in** shows only when some live task can give way (the bot's own), and the list it asks with
  holds only those.
- **4.** The roster header's button (`tray.full`, titled **Work**, with the count of working tasks, no number at
  none) shows only while a bot has a live or waiting task: a toolbar button on iPhone, a glass button on Android,
  an icon button in the desktop's roster header. It opens a sheet titled **Work**: the bots in the roster's
  on-screen order, each a heading (avatar, name, with the computer's name
  when there are several computers) that opens that bot's Details, over its Threads rows then its numbered Event
  box rows, with the rows' own actions. No `Free` rows, Later, Routines or Recent there. A row that needs you opens
  the chat at its card (terminal: the chat, scrolled to its end); the sheet closes when nothing is live or waiting.
  Terminal: `E` from the roster (listed in the keys sheet only while there is work; with none it says `No
  work`), `↵` on a name opens Details, the task keys are the Details page's, `esc`, `q` or `E` closes.
  Screen `all_work`.
- **5.** The `Work · <title>` line is on a thread's choice card, file message, voice note and artifact
  card as on its permission card. A thread's choice card has no **Something else** row: only a press answers it.
- **7.** **Worktree per thread** is a switch in Settings > Workspace, shown only when the host says the bot's folder
  is in a git repository and sends the setting, with the caption `Threads that work at once in this repository each
  get their own branch.` The task's branch is in its thread's transcript header (terminal: `⎇ <branch>` after the
  status; the desktop shows the folder on hover), not on its Details row. Stage 11 put it on the row too
  ([as built](#as-built-iphone-android-and-desktop-stage-11)).
- **8.** Each client reads `bot.outside` (the chat's own `status` for a host that doesn't send it). On every
  client a bot's avatar follows it, its mood and its needs-you badge alike (terminal: the roster row's mark), so a
  bot whose chat is idle shows working while its tasks work and needing you while one waits on a card.
  Desktop: the island's face, badge and dot and the tray's face, dot and working mark follow it; they show no
  counts line. iPhone: the Live Activity (the updates the app makes while it runs) and the widgets follow it, and
  the activity and a widget's bot row show the roster's line (`Needs you`, the chat's activity,
  `2 working · 3 waiting`). Android: the ongoing notification and the widgets follow it and show the same line.
  A widget says `Failed` only when that status is `error`: a chat that failed while tasks still work reads as
  working on both phones. An update that
  arrives by push carries the status only, on both phones.

## Stages

- [x] **1. Host**: tasks and the dispatcher in `chat/work/` (the thread bot machinery stays), the tools, the API, the
  wire, `maxThreads`, analytics, tests (`tests/work_e2e.rs` rewritten for tasks: the ceiling, order, first, cut in,
  replace, after, stop, retry, restart), a real-agent check.
- [x] **2. Clients**: capsule, Details, Settings, cut in, roster, on iPhone, Android, desktop and terminal.
- [x] **3. Other sources**: routines and bot requests as tasks, and on the four clients the row's mark (the
  routine's symbol or the asking bot's avatar before the title).
- [x] **4. One view of all bots' work**, on the four clients.
- [x] **5. A thread asks and shares through the chat**: `request_decision`, `send_file` and artifacts from a thread, as
  cards and messages in the bot's chat; the four clients show the task's line on a choice card, a file message, a
  voice note and an artifact card as on a permission card (`data.thread`, `data.task`).
- [x] **6. Voice calls know the box**: tasks in `bot_status` and the call's opening context, `stop_bot` stops
  them too, a thread's activity in the call's progress cues, approval and choice cues that name the task.
- [x] **7. Worktrees** for threads that run side by side in one git repository; the four clients have the switch
  (`threadWorktrees`, shown where `gitRepo`) and show the task's branch in its thread's transcript header
  (`work[].worktree`).
- [x] **8. The same line elsewhere**: the iPhone Live Activity, widgets and avatar mood, Android's ongoing
  notification and the desktop island and tray follow `bot.outside`; the two phones show the roster's line while
  the app runs. Checked by each client's unit tests only: a push-updated activity or notification (status only, no
  line) and a real iPhone's Lock Screen were not seen.
- [x] **9. Memory credit per session**: turn tracking keyed by the actor (the bot or a thread), so a thread's memory
  writes are its own turn's.
- [x] **10. Fork for other agents**: the fork spikes
  ([results](../archive/background-tasks-plan-2026-11-15.md#spikes), and each agent's page under
  `docs/upstream-agents/`) were run for the agents installed on the computer. Codex, Hermes, oh-my-pi and OpenCode
  advertise fork and passed, so they fork; Antigravity, Cursor, DeepSeek Harness, Grok Build and pi don't advertise
  it and start from a brief (Grok Build wasn't signed in, so only its `initialize` was read). The spikes ran
  without Codync's MCP servers: a fork with them attached was not checked for these agents.
- [x] **11. Talking to a thread** ([below](#stage-11-talking-to-a-thread)), on the host and the four clients: the
  host and the terminal ([as built](#as-built-host-and-terminal-stage-11)), then the iPhone, Android and desktop
  thread views, the where line and the git state in Settings
  ([as built](#as-built-iphone-android-and-desktop-stage-11), with what no test covers).

## Stages 3 to 10: what each builds

Decided 2026-11-15 ("do all of them"). Each stage keeps the rules above: the chat shows text and what the user must
act on, the user never types into a thread, four clients behave the same, nothing wakes an agent that has nothing to do.

**3. Other sources.** A task says where it came from: `source` = `bot` (today's `add_task`), `routine` (with the
routine's id) or `request` (with the asking bot's id). A routine run and another bot's request (`message_bot`)
become tasks in the bot's box instead of turns in its chat, so the chat stays free and they count under `maxThreads`.
Their result does not go through a turn of the coordinator: a routine task's last message is posted in the chat as
the routine's message exactly as a routine run posts today (title, progress note, follow-ups and alerts unchanged);
a request task's last message is the reply the asking bot receives. They start like any task (fork where the agent
can, else a brief that carries the routine's instructions or the request). A bot whose `maxThreads` threads are all
busy makes them wait in the box like anything else. Rows show the source as the existing symbol of a routine or
the asking bot's avatar before the title; no new words. `Try again` on a failed routine task runs the routine again.
Rooms (group chats) keep their own path.

**4. One view of all bots' work.** Each client gets one list of every bot that has live or waiting tasks: the bot's
name and avatar as a heading, under it the same rows as its Details (Threads rows then Event box rows, same actions).
It opens from one icon in the roster's header, shown only while any bot has such tasks, with the count of working
tasks; a row's heading opens that bot's Details. Terminal: `E` from the roster (`W` is taken: rename / remove section). Tray and island keep their own lines.

**5. A thread asks and shares through the chat.** `request_decision`, `send_file` and artifacts called by a thread are
no longer refused: the card or message appears in its bot's chat with `data.task` (the same "Work ·
<title>" line as a permission card), the user answers there, and the answer goes back to the thread (`card_actor`
already routes by task). A task waiting on such a card reads `needsYou`.

**6. Voice calls.** A thread's activity joins the call's progress cues (said as the task's title and what it does),
and an approval cue from a thread names its task.

**7. Worktrees.** Per bot setting `threadWorktrees` (Settings > Workspace, "Worktree per thread", on by default, shown
only when the workspace is a git repository). With it on, a thread that starts while another thread of the bot works in
the same repository gets its own git worktree (`~/.codync/worktrees/<bot>/<task>`, branch `codync/<title-slug>` from
the workspace's HEAD) as its working directory; the first thread alone keeps the workspace. Its report names the
branch. When the task ends: a worktree with no changes and no commits of its own is removed with its branch; anything
else is kept and the report says where it is. Codync never merges, rebases or deletes a branch that has work on it.

**8. The same line elsewhere.** The iPhone Live Activity, Android's ongoing notification and the desktop island
(face and status dot included) treat a bot with working tasks as working and show the roster's line
(`2 working · 3 waiting`, or `Needs you`).

**9. Memory credit per session.** Turn tracking is keyed by session, so a thread's `edit_memory` and its file edits to
memory are credited to the thread's own session, not the chat's.

**10. Fork for other agents.** The fork spikes run for each installed agent other than Claude; one that passes
forks, the others keep starting from a brief. Results go in the agent's page under `docs/upstream-agents/`.

## Stage 11: talking to a thread

Decided 2026-11-16 (the user: "右邊的 panel 變成 threads panel … 可以聊天 queue 和 steering", then "好幫我處理好" to
the proposal below). It replaces the earlier rule that the user never writes into a thread: the user writes to a
thread directly, as to a chat. Demo of the desktop panel: https://claude.ai/artifact/ETxnJuT8uNLCLqWy6nkK5H.

- **Opening a thread.** A row of the bot's own task opens the thread **in place of** what listed it, not in a dialog:
  on the desktop the inspector's panel slides to the thread (a back button returns to Details; the chat stays beside
  it); on iPhone and Android the row pushes a thread page inside the details sheet; in the terminal the transcript
  page gains a composer. The Work view of all bots opens the same thread view.
- **Its header.** Title, status and elapsed time, where it works (the folder, or the branch when it has a worktree:
  the folder symbol or the branch symbol before it), and one Stop button. What it was told (the brief) is first in
  the transcript, folded.
- **Writing to it.** A composer like the chat's (text; files and voice are not in this stage). While the thread's
  turn runs, a message waits in its queue and shows Send now and Remove, exactly as in the chat; Send now steers the
  running turn where the agent can, else stops it for the message. With no turn running the message starts one.
- **After it ended.** A done, failed or stopped task of the bot's own whose thread still exists (kept seven days)
  takes messages too: the first one reopens the task (`working` again, back under Threads, counted against Threads at
  once even past the ceiling, since the user asked), in the same session where the agent can resume it, else in a new
  one told what was done.
- **The bot hears of it.** The task's report to the bot ends with what the user told the thread in between (their
  words, shortened), so the chat's account of the work stays whole. A reopened task reports again when it ends.
- **Read-only threads.** A routine's run and another bot's request can be read and stopped, not written to: their
  result has an owner already. The composer is absent there, with one quiet line saying why.
- **Wording.** A waiting task held back by usage reads `Near the usage limit: one at a time` instead of
  `Waiting for usage`.

**Where it works, shown** (decided 2026-11-16: "顯示資料夾並且顯示這是不是 git project … information button … 打開我的 docs
網站"; review page https://claude.ai/artifact/AeeXyBAeQFpSB4MR1wSnhd):

- Every row of a task a thread has (Threads, and Recent while its thread is kept) has one more quiet line on iPhone,
  Android and desktop: the folder symbol and the folder (home as `~`) when the thread works in the bot's folder,
  the branch symbol and the branch when it works in a worktree of its own. The row's line copies nothing: the row
  is one button that opens the thread (the desktop shows the full path on hover). The terminal's rows don't have
  the line.
- The thread's header repeats the line, and there a click or tap copies the full path (`Copy path`; the line reads
  `Path copied` for a moment). The terminal's heading shows it and copies nothing.
- Settings > Workspace says under the folder whether it is a git repository: `Git repository`, `Not a git
  repository`, or `Git isn't installed on this computer`, with one information button (`info.circle`, `About
  working folders and git`) that opens the manual's page on working folders and git
  (https://codync.dev/docs/working-folders: what threads working at once do with and without git, and how to install
  git on macOS, Linux and Windows). The terminal has the words in dim text and no button. Only for a bot whose host
  sends `threadWorktrees`, and for the folder the host has saved. `Worktree per thread` stays where it is for a
  repository.
- The host says which of the three it is (`bot.gitRepo`, `bot.gitInstalled`).

Not in this version: a task given to a running thread by the dispatcher (only `after`); priorities the user sets;
preempting without being asked.

## As built (host and terminal, stage 11)

What is built is in [work](../features/work.md#talking-to-a-thread); where it settles or
differs from the text above:

- **API**: no new method. `send`, `steer` and `unqueue` take `taskId` beside `botId`: the message is for the thread
  that has or had that task of that bot, and a thread's own id as `botId` is refused. `send` answers `{entry,
  reopened, queued}`. The thread's messages are user entries of the thread's transcript (`threadEntries`, `threadEntry`),
  with the chat's `status` (`queued`, `sent`, `cancelled`, `failed`) and `removed`; what the host told the thread has
  `from: "host"`.
- **Wire**: `work[].talk` (the bot's own task whose thread bot exists) and `work[].folder` (where its thread works: the
  thread's own folder, which is the worktree's for one in a worktree; the bot's `cwd` alone would be wrong once the
  bot's folder changed, and `worktree.path` is the worktree's top, not where the thread is in it). A worktree removed
  with nothing in it names no folder: the thread bot's row still points into the deleted checkout (`tasks::folder_of`).
- **One report, not two.** A message that waits while the task's turn ends doesn't let the task end: it stays
  `working` through the message's turn, whose last message is the result and the one report. Only a message to an
  task that already ended reopens it and makes it report again.
- **Send now** on an agent that can't take a message into its turn stops that turn; the task isn't `stopped` by
  it, since the message's turn follows, and the thread gets no `Stopped.` line: drawn under the user's message, it
  read as if their message was stopped (e2e). A Stop meanwhile, or a Remove that leaves nothing waiting, keeps it.
- **Reopening** uses the thread bot's saved session as a restart does: `session/load` or `session/resume` where the
  agent has one, else a new session told the task, the user's words and notes and what it reported. The thread's
  agent process is started again either way (it stopped when the task ended).
- **Cut in and replace**: the messages that waited in the thread go with the task as `notes` (the session that
  starts it again reads them); the closed thread's entries end `cancelled`. **Stop** drops them.
- **A host restart** fails a waiting message, as in a chat. It is not sent on its own afterwards.
- **Memory**: a thread still only adds, also in a turn the user's message started.
- **A message to an ended task whose thread now works a task that followed it** goes to that session and waits
  behind its turn; the ended task isn't reopened.
- **Analytics**: `work_messaged {outcome, reopened, queued}`, or `{outcome: failed, error}` with the refusal's
  category (`thread::Refused`: `read_only`, `not_started`, `gone`, `text_only`, else `other`); Send now and Remove
  in a thread are `work_steered` with `how` `sendNow` / `remove` (`by: user`), and the agent-level
  `message_steered` still says what became of a Send now.
- **Wording**: the new key is in the ten catalogs, and all four clients read it. `Waiting for usage`, read by none
  any more, left the catalogs.
- **Terminal**: the transcript page has the composer under the steps. `i` or `↵` gives it the keys (so `q`, `T`,
  `X` type), `↵` sends, `⇧↵` / `^j` is a new line, `esc` gives the keys back. What waits shows over the composer
  as in the chat, with the chat's keys: `S` sends the next one now, `D` takes back the last. The heading has `⎇
  <branch>` or the folder (home as `~`) after the status. A routine's run and another bot's request have no
  composer and one dim line: `Read only: a routine's run`, `Read only: another bot's request`. The composer's
  placeholder is `Message this thread`. What the thread put in the bot's chat is one dim line among the steps where
  it came (`app::shared_line`: `◆` while it waits on the user, `✓`, `×` or `·` once answered or not, `▤` a file or
  an artifact, `♪` a voice note; a sign-in card `◆`, `✓` once signed in, `×` cancelled). `tab` picks the newest line and goes up, `⇧tab` down, `esc` lets go; `↵` on a
  picked line closes the page for the bot's chat at that entry: a card still waiting is at the chat's end, where
  the chat's keys answer it, and anything else is picked there (an answered card counts as a message for that),
  which brings it on screen. The page (`view/thread.rs`, `manage/threads.rs`) still opens over Details as before, not
  in its place.
- **Where it works, shown (host, terminal, manual)**: `bot.gitInstalled` (bool, beside `bot.gitRepo`; absent =
  installed) from a cached look for git that never holds a snapshot up, refreshed at most every five minutes, so
  a git installed meanwhile shows after a restart or within that time. The terminal's Settings form has the state
  in dim text under Workspace, for the folder the bot has (not one picked and not saved yet, and nothing from a
  host before worktrees); it has no information button and names no URL. The nine words are in the ten catalogs.
  The manual's page is `/docs/working-folders`, with `#without-git` for the two cases without copies.

## As built (iPhone, Android and desktop, stage 11)

The rules are one set, written three times: `apps/desktop/src/shared/work.ts` (`readOnlyReason`, `whereLine`,
`wherePath`, `gitState`, `threadEntries`, `isThreadMessage`, `sharedLine`), the iPhone's
`CodyncKit/Models/WorkThread.swift` and Android's `core/Work.kt`; whether a thread takes a message is the host's
`talk`, read as it is. Where the three settle or differ from the text above:

- **Opening.** Desktop: the inspector's panel slides from its pages to the thread (`DetailsPanel` → `WorkThread`);
  the back button (`Back to details`), or Escape while the keyboard is in the panel, returns to the page as it was
  left, and the keyboard goes back to the row; a thread whose task left the box closes. The dialog that showed a
  thread's transcript is gone. iPhone: the row
  pushes `WorkThreadView` inside the details sheet; the system bar is hidden (`hidesSystemNavigationBar`, which keeps
  the edge swipe back) and the page draws its own header, as Android's: a `Back to details` icon button, the title
  over the status line, Stop. Android: `WorkThreadView` is pushed inside the sheet by the design module's
  `NavStack`, with the same header. A row of the Work sheet opens the bot's chat with its
  details on that thread.
- **A row that needs you still opens the chat at its card**, not the thread. In the task's thread the card is one
  line (below), which opens the chat at it; the thread has no card to answer.
- **Header**: title, the status word with its mark and the time, the where line, Stop while the task is live (it
  asks first).
- **Transcript**: `What it was told` folded on top (the host's first words to the thread); what the host told it
  later (a task picked up again) is a quiet quote where it happened; the user's messages are the chat's bubbles;
  the thread's final replies (`isThreadMessage`: kind `agent` and `final`) are the chat's agent message, read only
  (Copy; no reactions, reply or trace); the rest are the chat's working steps, the running turn's unfolded. A thread's
  own transcript holds no file or artifact (what it shares goes to the chat), so its replies are text on every
  client. What the thread put in the bot's chat (the entries whose `data.thread.id` is the thread's id: approval and
  choice cards, answered ones too, its agent's sign-in card, files, voice notes, artifact cards) joins the transcript in time order
  (`createdAt`, each side kept in its order, the thread's own first at the same moment) as one quiet line with a
  symbol (`sharedLine`: `hand.raised` while it waits on the user, `checkmark`, `xmark` or `clock` once answered or
  not, `doc`, `waveform`, `doc.richtext`; a sign-in card reads as the card stands: `Needs <agent> sign-in`, `Signed
  in`, `Cancelled`, not `Asked in the chat to sign in to <agent>.`, which the thread's own transcript already has
  beside it). The full card, file or artifact stays in the chat, where the coordinator
  shows it. A tap or click on the line opens the chat at that entry: the desktop scrolls the chat beside the panel
  to it (`showInChat`, by `data-entry-id`); the phones close the details sheet and bring it into view, as a
  needs-you row does (iPhone `BotDetailsView.openInChat`, Android `openCard`). Only the chat's loaded messages are
  looked at, as for a needs-you row.
- **Composer**: the chat's, text only (`Message this thread`): no files, dictation, commands, reply or call. Each
  client keeps a draft per task, under the bot's (desktop `<bot>/thread/<task>`, phones `<bot>/task/<task>`), so it
  goes with the bot. Off while the computer is offline.
- **Messages that wait.** Desktop: the bubble stays in the transcript with `Queued`, **Send now** and **Remove**
  under it. iPhone and Android: they leave the transcript for the chat's tray above the composer, with the same
  two buttons. On the desktop a message that never arrived has Resend and Delete, and one dropped by Stop reads
  `Not sent — stopped`; the phones show the chat's own lines for both.
- **Read-only threads**: no composer and one quiet line, `Read only: a routine's run` or `Read only: another bot's
  request`. An own task whose thread is gone (`talk` false) has neither.
- **Where line.** Every client writes the computer's home (`hello.home`) as `~`, with one rule (`homeRelative`,
  the terminal's `tilde`): at a separator boundary only, `/` or `\`; any other path shows whole and is cut in the
  middle.
- **Git state** in Settings > Workspace as [above](#stage-11-talking-to-a-thread): `GitStateLine.tsx`,
  `GitStateRow.swift`, `GitLine` in `BotSettingsForm.kt`. The button reports `link_opened` with `link: docs`.
- **Analytics**: screen `thread` is the thread view; the rest is the host's (`work_messaged`, `work_steered`). The
  tap on a shared line is navigation and reports nothing of its own.

**What no test covers.** The desktop's Playwright spec (`e2e/work.spec.ts`) and the phones' Maestro flow
(`apps/maestro/tasks.yaml`) walk opening a thread from its row, writing to a working thread (the message waits,
Send now and Remove show) and, on the desktop, reopening a done task, a card's line in an open thread that shows
the card in the chat and turns `Answered` once it is answered there, and a routine run's read-only line. Neither
has a step for: copying the path from the header, the git state line and its button, Escape, Resend of a failed
message, a shared line on the phones, or a Windows path in the where line.
Whether a real iPhone, a real Android phone, and the desktop on Linux and Windows were looked at is not recorded
here.
