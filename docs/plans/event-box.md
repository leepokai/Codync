# Event box

Status: **being built**, 2026-11-15, on `feat/background-tasks`. Builds on background tasks as
merged in 2.48.0 ([its first design](../archive/background-tasks-plan-2026-11-15.md)): what was "one task, a second
refused" becomes a queue.

Everything a bot should work on away from its chat is an **item** in a durable box. A bounded number of the bot's
agents take items from it. The owner's decisions are marked *(owner)*.

## Words

- **Coordinator**: the bot's main conversation. It talks to the user, decides what is worth an item, and tells the
  user what came of it.
- **Item**: one piece of work in the box: a title, a brief, the user's own words, a status.
- **Task**: an agent session working items, one at a time: a task bot
  ([background tasks](../features/background-tasks.md)). An item names the task that has it.
- **Box**: the bot's items. It is the source of truth: a task that dies leaves its item there.

## An item

| Status | Meaning |
| --- | --- |
| `waiting` | In the box; no task works on it |
| `working` | A task works on it |
| `needsYou` | Its task waits on an approval |
| `done` | Finished; `result` is what its task reported |
| `failed` | It could not be finished; `result` says why |
| `stopped` | The user or the bot stopped it |

These six are the only words for work anywhere: the wire, every client, the bot's tools.

Record (kv `work.<bot>`, one JSON list per bot, replacing `task.<id>`): `id, title, brief, words, notes (what the
user said about it since, with their words), status, order, after (an item it should follow in the same session),
task (the task bot that has or had it), cutIn (it gave way once), hops, createdAt, startedAt, endedAt, result,
report (until the coordinator's turn for it has ended)`.

## What goes in

*(owner)* The user talks to the coordinator; when it judges the request worth it, it adds an item. Short things it
answers itself. A run of one of the bot's routines and another bot's request are items too (stage 3): the host adds
them, and their result goes to the chat as the routine's message or back to the asking bot, not through the
coordinator.

## The dispatcher

*(owner)* The user sets one number per bot, **the most agents at once** (`maxAgents`, 1 to 6, 3 unless changed).
*(owner, 2026-11-15)* There is no limit for the computer as a whole: each bot's own number is the only ceiling. Within it the dispatcher decides how many run: the number is a ceiling, not a target.

One function, `tasks::dispatch(hub, bot)`, run whenever the bot's box or one of its tasks changes (an item added,
moved, dropped or retried; a task's turn over; the coordinator's turn over; the setting changed; host start). For
each waiting item, lowest `order` first:

1. **It follows an item** (`after`) whose task is alive and idle (its last item ended): that task takes it, in the
   same session. If that task is working, the item waits for it. If that task is gone, the item is an ordinary one.
2. **Otherwise a new task starts for it** when the bot is under `maxAgents` and the agent is
   not near its usage limit (near it, only when none of the bot's tasks runs).
3. **Otherwise it waits.** Nothing is refused for being over the ceiling.

An item added during the coordinator's turn starts when that turn ends (a fork taken mid-turn would carry an
unanswered tool call), as tasks do today. A task with no item left ends: its session is closed and its process
stopped; its transcript stays for its items' details. The dispatcher never starts a task to fill a place.

Why an item waits is on the wire (`wait`): `next` (it starts when a place is free), `behind` with how many are
ahead, `after` (it follows a working item), `usage`.

## Steering

*(owner)* Three places to cut in; the box's own is only about order.

| Where | What it does |
| --- | --- |
| The chat with the coordinator | As today: a message sent while it answers waits above the composer, and can be sent now or removed. |
| The box | **First**: a waiting item moves to the front. Order only. |
| A running task | The user writes to the thread of the bot's own item as to a chat (stage 11; before it, a task's session was never written into). To put an item on a busy bot **now**, a task gives way: its work is closed and a new session starts for the item. |

**Cutting in** (`now`), when every place is taken: the user picks which working item gives way (or the coordinator
names it; with none named, the one that started last). That task is stopped and deleted, its item returns to the
front of the box with `cutIn` set, and starts again later in a new session from its brief and notes. The freed place
starts the item that cut in. With a place free, `now` is just `first`; so it is for an item that couldn't take the
freed place (it follows unfinished work, or waits for usage): no session is closed for it, and the clients don't
offer Cut in on such a row.

**Changing an item** (`replace`): its brief is replaced. Waiting: nothing else. Working: its task's work is closed
the same way and the item starts again from the new brief.

**More for the same session** is not steering: it is a new item with `after`, which that task takes when it has
finished what it is on.

## Tools (on the `chat` MCP server, replacing the task tools)

- `add_work {title, brief, after?}` → `{itemId, status}`. Refused outside the bot's own chat, in a task, past 20
  waiting items, and for the ninth item added in a row with no message from the user.
- `steer_work {item, how: "first" | "now" | "replace" | "stop", brief?, insteadOf?}`. The user's own words of the
  running turn are added to the item's `notes`.
- `work_status {}` → every item that is live or ended in the last day: status, what it does now, seconds, its
  result.

`tasks::EACH_TURN` and `tasks::BRIEF` name `add_work`. Each of the coordinator's turns in its own chat also carries
the box in a few lines (counts, then the live and waiting titles with their ids), so it can answer "what is it
doing?" and place an `after` without a tool call.

A task's first message is the item (title, brief, words, notes). A task's last message of a turn is its item's
result and report: `Cmd::TaskReport` as today, one per item, reports that arrive together taken in one turn.

## API (clients)

`workFirst {botId, itemId}`, `workCutIn {botId, itemId, insteadOf}`, `dropWork {botId, itemId}` (a waiting item, or
one that ended), `stopWork {botId, itemId?}` (one working item, or all; replaces `stopTask`), `retryWork {botId,
itemId}` (a stopped or failed item goes back to waiting, last). `taskEntries {botId, taskId}` and `taskEntry` events
stay: a task's transcript, by the item's `task`.

## Wire

- Bot: `work: [item]`, replacing `tasks`: every live or waiting item in dispatch order, then items ended in the last
  24 hours (newest first, at most 20). Item: `{id, title, status, activity, task, wait: {kind, ahead?}, cutIn,
  createdAt, startedAt, endedAt, result}` (`result` at most 280 characters; `activity` is its task's).
- Bot: `maxAgents`. Set with `updateBot`.
- Absent on an older host: the feature doesn't show.

## In the apps

*(owner)* The chat stays as it is: messages and what needs the user. The capsule is the one sign of work. The bot's
inspector changes so that **Details is what the bot does** and **Settings is how it is set up**.

**Capsule** (above the composer of the bot's own chat, while any item is live or waiting):
`In the background · 2 working · 3 waiting`; `In the background · Needs you` first when any item needs the user,
with the counts after it. One working item and nothing waiting: `In the background · <its activity or title>` and
the time, as today. It opens Details. Its Stop (one item working) asks once, then `stopWork`.

**Details** (the inspector's first tab; the phones' bot details), top to bottom, empty sections left out:

| Section | Rows |
| --- | --- |
| **Threads** | *(owner: the running sessions and the box are told apart.)* The agent sessions that run now. The heading carries `2 / 3`: how many run, of the most allowed. Items that need you first (the attention mark, what the card asks; opens the chat at the card), then working items (a moving dot, the task's activity, elapsed time). Both kinds of row have Stop. A working row opens its thread (stage 11; the task's transcript before it) and says where it works. A place not in use is one faint row, `Free`, so the ceiling is visible. |
| **Event box** | What waits for a thread, numbered 1, 2, 3 in the order it will be taken, with why it waits. The heading carries how many. **First** on every row but the first; **Cut in** on the first row when every place of the bot is taken (not on a row that follows unfinished work or waits for usage), which asks which working item gives way; **Drop**. Left out when nothing waits. |
| **Later** | Follow-ups the bot set (the dashed card, as today). |
| **Routines** | As today, with **+**. |
| **Dev servers** | As today, when the bot has some. |
| **Recent** | Items ended in the last day: done (a ring, time taken; opens its thread), stopped and failed (the danger mark, why; **Try again**). |

**Settings** (the inspector's Settings tab; the phones' bot settings) gains what Details held: **Instructions** and
**Agent** (the agent, model, fast mode, fallbacks) go first, then Profile, Workspace (with **Agents at once**, a
1 to 6 stepper), Activity, Connectors, Skills.

**Cut in** asks with the client's dialog: `Let "<title>" cut in`, `Every place is taken. Pick what gives way: its
session closes and it starts again later.`, one row per working item (title, what it does, how long), `Cancel`.

**Roster row**, the most pressing thing first: needs you; the chat's own activity; `2 working · 3 waiting` (either
part alone when the other is zero); else the last message.

**The same on every client**: an item whose status the client doesn't know is left out; a waiting item with no
reason reads `Waiting`; a host that sends no `maxAgents` gets no stepper, and the client counts 3 for whether every
place is taken; the Threads section shows only while the bot has a live or waiting item (an idle bot's Details has
no row of free places), and its heading says what is true (`2 / 1` right after the ceiling was lowered).

**Terminal**: the line over the composer says the same as the capsule; `B` opens the bot's Details page, which lists
the same sections; on a row `↵` opens the transcript (the terminal's thread view), `X` stops (asks), `F` moves first, `C` cuts in (asks which),
`D` drops, `A` tries again. Agent and Instructions are on its Settings page. `E` from the roster opens the page of
every bot's work (stage 4).

**Words** (English key = Traditional Chinese): `Threads` Threads, `Event box` Event Box, `Free` 空位,
`Later` 之後, `Recent` 最近,
`{count} working` {count} 進行中, `{count} waiting` {count} 等待中, `Next up` 下一個, `{count} ahead` 前面還有
{count} 件, `Follows what it works on` 接在進行中的事後面, `Near the usage limit: one at a time` 用量接近上限，一次只跑一個
(stage 11; `Waiting for usage` 等用量恢復 before it), `Was cut in on; starts
again` 被插隊，會重新開始, `First` 先做, `Cut in` 插隊, `Drop` 丟掉, `Try again` 再試, `Agents at once` 同時幾個
agent, `Let "{title}" cut in` 讓「{title}」插隊, `Every place is taken. Pick what gives way: its session closes and
it starts again later.` 名額滿了。選一個讓位：它的 session 會關掉，之後重新開始。 The strings of 2.48.0 that still
apply are reused; `{count} tasks running` goes.

## Restart, failure, limits

- At host start: a working item whose task resumes stays working; one whose task can't goes back to waiting once
  (as an item that was cut in on, from its brief), then fails.
- A task that fails fails its item with the reason; the coordinator hears it in a report.
- Every hour while a box holds waiting or working items, the host looks it over again (`tasks/sweep.rs`): the
  dispatcher takes a pass, and a working item whose task runs no turn waits again once, then fails. A turn that
  runs, however long and quiet, is left alone. The hour counts from the box's last change; an empty box sets no
  look. Nothing here wakes an agent unless an item was lost.
- Stopping a working item stops its task; items waiting `after` it become ordinary waiting items.
- Items ended over seven days ago are deleted, and a task bot with no item left is deleted with its transcript.
- Chains: items added from a report's turn carry its hops; the ninth is refused.

## Analytics

`work_added {outcome: added | elsewhere | invalid | full | hops | failed, after, source}`, `work_finished {outcome,
start, forkFailed, minutes, error, source, worktree, kept, worktreeFailed}` (`source`: `bot`, `routine`, `request`), `work_steered {how, by: bot |
user}`, replacing the `task_*` events. Screen `task`
stays: it is the item's thread view (a task's transcript before stage 11).

## As built (host, stage 1)

Where the host differs from, or settles, the text above ([what is built](../features/background-tasks.md)):

- **A task ends as soon as its item does** when no waiting item follows one of its items. So `after` joins a session
  only while what it follows is working or waiting; `add_work` refuses an `after` that names an ended item.
- **Items that follow go first** in a pass, before the ordinary ones in `order`: their task already has its place,
  so the ceiling holds.
- An item whose `after` item failed, was stopped or was dropped is an ordinary item, as when its task is gone.
- **Replace on a working item** puts it at the front without setting `cutIn` (nothing cut in on it).
- **`stopWork` without an item** stops the waiting items too, or they would start in the freed places.
- `workCutIn`'s `insteadOf` is optional, as in the tool: the working item that started last gives way.
- Stopping the bot's turn drops the items that turn added, as it dropped the task it started in 2.48.0.
- The restart rule keeps its own mark on the item (`restarted`), so an item the user cut in on still gets its one
  restart.
- `work_steered`'s `how` also takes `drop` and `retry`, from the user's `dropWork` and `retryWork`.
- `taskEntries` answers `task: {id, title, status, startedAt, endedAt, start}` for the item the task works on, else
  the last it worked on.
- `wait` is null on an item that isn't waiting. `maxAgents` is on every bot row, groups included (unused there).

## As built (host, stages 3 and 5)

What is built is in [background tasks](../features/background-tasks.md#other-sources); where it settles or differs
from the text below:

- **Wire**: `work[].source = {kind: routine | request, id?}`, absent on the bot's own item. `id` is the routine's
  id, or the asking bot's (absent for a request from another computer or the cloud assistant). A routine row's
  title is the routine's name; a request row's is the request's first line (80 characters). No new API method.
- **A bot with computer use or Manage the team on keeps its routine runs and requests as turns in its chat**
  (`tasks::boxed`): a task has neither server, so a run that drives the screen or sets up a bot would break.
- **A run or a request is never started twice**: it doesn't give way to a cut in (`workCutIn` with it as
  `insteadOf` is refused, and it isn't picked when none is named), `steer_work replace` refuses it, and the
  restart rule and the hourly look end it at once (a run `interrupted`, so its routine's retry rules decide; a
  request failed) where the bot's own item waits again once.
- **`retryWork`** on a routine's item runs the routine again (a new run, a new item, the old row gone); on a
  request's it is refused. **`stopWork` / `dropWork`** cancel the run or the request. `workFirst` is as for any item.
- **Stop in the chat** stops neither: they are the box's now.
- **A routine's run in a task starts in a new session with only its own prompt**, as a run in the chat does: no
  fork and no chat so far, so it costs and sees what it did before (corrected after stage 3 first forked it). A
  request starts like any item.
- **A request answered with exactly `(pass)`** gives the asking bot those words, as a turn in the chat did.
- **What those turns could do in the chat holds in their task**: `request_decision`, `send_message`, `send_file`
  and artifact cards refused; the `routines` tools (and, from a run, `message_bot`) work as the bot. Stage 5's
  asking and sharing is for the bot's own items.
- **A task's choice card is answered by a press only**: a message the user types is the coordinator's, so it neither
  answers the card nor ends it; the chat's Stop doesn't end it either (stopping the item does). The cases are
  listed in [asking and sharing from a task](../features/background-tasks.md#asking-and-sharing-from-a-task).
- **A task's file and artifact are the bot's** (its files, its Library); only the card or message carries the item.

## As built (host, stages 6 to 9)

What is built is in [background tasks](../features/background-tasks.md) ([memory](../features/background-tasks.md#memory),
[worktrees](../features/background-tasks.md#worktrees), [outside the chat](../features/background-tasks.md#outside-the-chat))
and [voice calls](../features/voice-call.md#how-the-voice-talks); where it settles or differs from the text below:

- **6.** A progress cue for an item reads `<bot>, in the background on "<title>": <activity>`; the chat's own turn
  and each item at work are said when what they do changed, one line every 15 s. A choice card (`request_decision`)
  is a cue too, the chat's own as well as a task's; a task's says it takes a press in the chat.
- **7.** Wire: `bot.taskWorktrees` (true unless set, `updateBot`), `bot.gitRepo`, `work[].worktree = {branch,
  path}` while it is there. **A task in a worktree starts from a brief, not a fork** (no agent is checked to honor
  another folder on a fork). The worktree is made by the task as its first turn starts, not by the dispatcher, so
  no git runs under the box's lock. An item that follows another stays in that task's folder; one that starts again
  reuses its worktree. A stopped item's worktree is looked over when its task's turn has ended, with no report.
  Git runs through `tokio::process` with an argument list and a one-minute limit per command, not on a blocking
  thread: the limit can stop it. `work_finished` gains `worktree`, `kept` and `worktreeFailed`.
- **8.** The host builds no line: the Live Activity and the ongoing notification get the status and a start time
  only (the relay passes nothing else), so the host's part is the lifecycle. Working means the box holds an item
  that is working or waiting, or one whose report the chat hasn't taken. The same status is `bot.outside =
  {status, startedAt}` on the wire. No alert was added or removed.
- **9.** Keyed by the acting bot or task, not by session id: each has one turn at a time, and the turn records its
  session. A task's turn is never the user's own, so a task adds to memory and never takes out, for the bot's own
  items too.

## As built (clients, stages 3 to 8)

The same on iPhone, Android, desktop and terminal unless said; where it settles or differs from the text below:

- **3.** Before a row's title: the routine symbol (`clock.arrow.circlepath`; terminal `↻`) for a routine's run, the
  asking bot's small avatar (terminal: its name and `›`) for a request. A request from a bot the client doesn't
  have (another computer, the cloud assistant, a deleted bot) has no mark. **Try again** isn't offered on a
  request's row. **Cut in** shows only when some live item can give way (the bot's own), and the list it asks with
  holds only those.
- **4.** The roster header's button (`tray.full`, titled **Work**, with the count of working items, no number at
  none) shows only while a bot has a live or waiting item: a toolbar button on iPhone, a glass button on Android,
  an icon button in the desktop's roster header. It opens a sheet titled **Work**: the bots in the roster's
  on-screen order, each a heading (avatar, name, with the computer's name
  when there are several computers) that opens that bot's Details, over its Threads rows then its numbered Event
  box rows, with the rows' own actions. No `Free` rows, Later, Routines or Recent there. A row that needs you opens
  the chat at its card (terminal: the chat, scrolled to its end); the sheet closes when nothing is live or waiting.
  Terminal: `E` from the roster (listed in the keys sheet only while there is work; with none it says `Nothing in
  the background`), `↵` on a name opens Details, the item keys are the Details page's, `esc`, `q` or `E` closes.
  Screen `all_work`.
- **5.** The `In the background · <title>` line is on a task's choice card, file message, voice note and artifact
  card as on its permission card. A task's choice card has no **Something else** row: only a press answers it.
- **7.** **Worktree per agent** is a switch in Settings > Workspace, shown only when the host says the bot's folder
  is in a git repository and sends the setting, with the caption `Agents that work at once in this repository each
  get their own branch.` The item's branch is in its task's transcript header (terminal: `⎇ <branch>` after the
  status; the desktop shows the folder on hover), not on its Details row. Stage 11 put it on the row too
  ([as built](#as-built-iphone-android-and-desktop-stage-11)).
- **8.** Each client reads `bot.outside` (the chat's own `status` for a host that doesn't send it). On every
  client a bot's avatar follows it, its mood and its needs-you badge alike (terminal: the roster row's mark), so a
  bot whose chat is idle shows working while its items work and needing you while one waits on a card.
  Desktop: the island's face, badge and dot and the tray's face, dot and working mark follow it; they show no
  counts line. iPhone: the Live Activity (the updates the app makes while it runs) and the widgets follow it, and
  the activity and a widget's bot row show the roster's line (`Needs you`, the chat's activity,
  `2 working · 3 waiting`). Android: the ongoing notification and the widgets follow it and show the same line.
  A widget says `Failed` only when that status is `error`: a chat that failed while items still work reads as
  working on both phones. An update that
  arrives by push carries the status only, on both phones.

## Stages

- [x] **1. Host**: items and the dispatcher in `chat/tasks/` (the task bot machinery stays), the tools, the API, the
  wire, `maxAgents`, analytics, tests (`tests/tasks_e2e.rs` rewritten for items: the ceiling, order, first, cut in,
  replace, after, stop, retry, restart), a real-agent check.
- [x] **2. Clients**: capsule, Details, Settings, cut in, roster, on iPhone, Android, desktop and terminal.
- [x] **3. Other sources**: routines and bot requests as items, and on the four clients the row's mark (the
  routine's symbol or the asking bot's avatar before the title).
- [x] **4. One view of all bots' work**, on the four clients.
- [x] **5. A task asks and shares through the chat**: `request_decision`, `send_file` and artifacts from a task, as
  cards and messages in the bot's chat; the four clients show the item's line on a choice card, a file message, a
  voice note and an artifact card as on a permission card (`data.task`, `data.item`).
- [x] **6. Voice calls know the box**: items in `bot_status` and the call's opening context, `stop_bot` stops
  them too, a task's activity in the call's progress cues, approval and choice cues that name the item.
- [x] **7. Worktrees** for tasks that run side by side in one git repository; the four clients have the switch
  (`taskWorktrees`, shown where `gitRepo`) and show the item's branch in its task's transcript header
  (`work[].worktree`).
- [x] **8. The same line elsewhere**: the iPhone Live Activity, widgets and avatar mood, Android's ongoing
  notification and the desktop island and tray follow `bot.outside`; the two phones show the roster's line while
  the app runs. Checked by each client's unit tests only: a push-updated activity or notification (status only, no
  line) and a real iPhone's Lock Screen were not seen.
- [x] **9. Memory credit per session**: turn tracking keyed by the actor (the bot or a task), so a task's memory
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
act on, the user never types into a task, four clients behave the same, nothing wakes an agent that has nothing to do.

**3. Other sources.** An item says where it came from: `source` = `bot` (today's `add_work`), `routine` (with the
routine's id) or `request` (with the asking bot's id). A routine run and another bot's request (`message_bot`)
become items in the bot's box instead of turns in its chat, so the chat stays free and they count under `maxAgents`.
Their result does not go through a turn of the coordinator: a routine item's last message is posted in the chat as
the routine's message exactly as a routine run posts today (title, progress note, follow-ups and alerts unchanged);
a request item's last message is the reply the asking bot receives. They start like any item (fork where the agent
can, else a brief that carries the routine's instructions or the request). A bot whose `maxAgents` places are all
taken makes them wait in the box like anything else. Rows show the source as the existing symbol of a routine or
the asking bot's avatar before the title; no new words. `Try again` on a failed routine item runs the routine again.
Rooms (group chats) keep their own path.

**4. One view of all bots' work.** Each client gets one list of every bot that has live or waiting items: the bot's
name and avatar as a heading, under it the same rows as its Details (Threads rows then Event box rows, same actions).
It opens from one icon in the roster's header, shown only while any bot has such items, with the count of working
items; a row's heading opens that bot's Details. Terminal: `E` from the roster (`W` is taken: rename / remove section). Tray and island keep their own lines.

**5. A task asks and shares through the chat.** `request_decision`, `send_file` and artifacts called by a task are
no longer refused: the card or message appears in its bot's chat with `data.item` (the same "In the background ·
<title>" line as a permission card), the user answers there, and the answer goes back to the task (`card_actor`
already routes by item). An item waiting on such a card reads `needsYou`.

**6. Voice calls.** A task's activity joins the call's progress cues (said as the item's title and what it does),
and an approval cue from a task names its item.

**7. Worktrees.** Per bot setting `taskWorktrees` (Settings > Workspace, "Worktree per agent", on by default, shown
only when the workspace is a git repository). With it on, a task that starts while another task of the bot works in
the same repository gets its own git worktree (`~/.codync/worktrees/<bot>/<item>`, branch `codync/<title-slug>` from
the workspace's HEAD) as its working directory; the first task alone keeps the workspace. Its report names the
branch. When the item ends: a worktree with no changes and no commits of its own is removed with its branch; anything
else is kept and the report says where it is. Codync never merges, rebases or deletes a branch that has work on it.

**8. The same line elsewhere.** The iPhone Live Activity, Android's ongoing notification and the desktop island
(face and status dot included) treat a bot with working items as working and show the roster's line
(`2 working · 3 waiting`, or `Needs you`).

**9. Memory credit per session.** Turn tracking is keyed by session, so a task's `edit_memory` and its file edits to
memory are credited to the task's own session, not the chat's.

**10. Fork for other agents.** The fork spikes run for each installed agent other than Claude; one that passes
forks, the others keep starting from a brief. Results go in the agent's page under `docs/upstream-agents/`.

## Stage 11: talking to a thread

Decided 2026-11-16 (the user: "右邊的 panel 變成 threads panel … 可以聊天 queue 和 steering", then "好幫我處理好" to
the proposal below). It replaces the earlier rule that the user never writes into a task: the user writes to a
thread directly, as to a chat. Demo of the desktop panel: https://claude.ai/artifact/ETxnJuT8uNLCLqWy6nkK5H.

- **Opening a thread.** A row of the bot's own item opens the thread **in place of** what listed it, not in a dialog:
  on the desktop the inspector's panel slides to the thread (a back button returns to Details; the chat stays beside
  it); on iPhone and Android the row pushes a thread page inside the details sheet; in the terminal the transcript
  page gains a composer. The Work view of all bots opens the same thread view.
- **Its header.** Title, status and elapsed time, where it works (the folder, or the branch when it has a worktree:
  the folder symbol or the branch symbol before it), and one Stop button. What it was told (the brief) is first in
  the transcript, folded.
- **Writing to it.** A composer like the chat's (text; files and voice are not in this stage). While the thread's
  turn runs, a message waits in its queue and shows Send now and Remove, exactly as in the chat; Send now steers the
  running turn where the agent can, else stops it for the message. With no turn running the message starts one.
- **After it ended.** A done, failed or stopped item of the bot's own whose task still exists (kept seven days)
  takes messages too: the first one reopens the item (`working` again, back under Threads, counted against Agents at
  once even past the ceiling, since the user asked), in the same session where the agent can resume it, else in a new
  one told what was done.
- **The bot hears of it.** The item's report to the bot ends with what the user told the thread in between (their
  words, shortened), so the chat's account of the work stays whole. A reopened item reports again when it ends.
- **Read-only threads.** A routine's run and another bot's request can be read and stopped, not written to: their
  result has an owner already. The composer is absent there, with one quiet line saying why.
- **Wording.** A waiting item held back by usage reads `Near the usage limit: one at a time` instead of
  `Waiting for usage`.

**Where it works, shown** (decided 2026-11-16: "顯示資料夾並且顯示這是不是 git project … information button … 打開我的 docs
網站"; review page https://claude.ai/artifact/AeeXyBAeQFpSB4MR1wSnhd):

- Every row of an item a task has (Threads, and Recent while its task is kept) has one more quiet line on iPhone,
  Android and desktop: the folder symbol and the folder (home as `~`) when the thread works in the bot's folder,
  the branch symbol and the branch when it works in a worktree of its own. The row's line copies nothing: the row
  is one button that opens the thread (the desktop shows the full path on hover). The terminal's rows don't have
  the line.
- The thread's header repeats the line, and there a click or tap copies the full path (`Copy path`; the line reads
  `Path copied` for a moment). The terminal's heading shows it and copies nothing.
- Settings > Workspace says under the folder whether it is a git repository: `Git repository`, `Not a git
  repository`, or `Git isn't installed on this computer`, with one information button (`info.circle`, `About
  working folders and git`) that opens the manual's page on working folders and git
  (https://codync.dev/docs/working-folders: what agents working at once do with and without git, and how to install
  git on macOS, Linux and Windows). The terminal has the words in dim text and no button. Only for a bot whose host
  sends `taskWorktrees`, and for the folder the host has saved. `Worktree per agent` stays where it is for a
  repository.
- The host says which of the three it is (`bot.gitRepo`, `bot.gitInstalled`).

Not in this version: an item given to a running task by the dispatcher (only `after`); priorities the user sets;
preempting without being asked.

## As built (host and terminal, stage 11)

What is built is in [background tasks](../features/background-tasks.md#talking-to-a-thread); where it settles or
differs from the text above:

- **API**: no new method. `send`, `steer` and `unqueue` take `itemId` beside `botId`: the message is for the task
  that has or had that item of that bot, and a task's own id as `botId` is refused. `send` answers `{entry,
  reopened, queued}`. The thread's messages are user entries of the task's transcript (`taskEntries`, `taskEntry`),
  with the chat's `status` (`queued`, `sent`, `cancelled`, `failed`) and `removed`; what the host told the task has
  `from: "host"`.
- **Wire**: `work[].talk` (the bot's own item whose task bot exists) and `work[].folder` (where its task works: the
  task's own folder, which is the worktree's for one in a worktree; the bot's `cwd` alone would be wrong once the
  bot's folder changed, and `worktree.path` is the worktree's top, not where the task is in it). A worktree removed
  with nothing in it names no folder: the task row still points into the deleted checkout (`items::folder_of`).
- **One report, not two.** A message that waits while the item's turn ends doesn't let the item end: it stays
  `working` through the message's turn, whose last message is the result and the one report. Only a message to an
  item that already ended reopens it and makes it report again.
- **Send now** on an agent that can't take a message into its turn stops that turn; the item isn't `stopped` by
  it, since the message's turn follows, and the thread gets no `Stopped.` line: drawn under the user's message, it
  read as if their message was stopped (e2e). A Stop meanwhile, or a Remove that leaves nothing waiting, keeps it.
- **Reopening** uses the task bot's saved session as a restart does: `session/load` or `session/resume` where the
  agent has one, else a new session told the item, the user's words and notes and what it reported. The task's
  agent process is started again either way (it stopped when the item ended).
- **Cut in and replace**: the messages that waited in the thread go with the item as `notes` (the session that
  starts it again reads them); the closed task's entries end `cancelled`. **Stop** drops them.
- **A host restart** fails a waiting message, as in a chat. It is not sent on its own afterwards.
- **Memory**: a task still only adds, also in a turn the user's message started.
- **A message to an ended item whose task now works an item that followed it** goes to that session and waits
  behind its turn; the ended item isn't reopened.
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
  placeholder is `Message this thread`. What the task put in the bot's chat is one dim line among the steps where
  it came (`app::shared_line`: `◆` while it waits on the user, `✓`, `×` or `·` once answered or not, `▤` a file or
  an artifact, `♪` a voice note; a sign-in card `◆`, `✓` once signed in, `×` cancelled). `tab` picks the newest line and goes up, `⇧tab` down, `esc` lets go; `↵` on a
  picked line closes the page for the bot's chat at that entry: a card still waiting is at the chat's end, where
  the chat's keys answer it, and anything else is picked there (an answered card counts as a message for that),
  which brings it on screen. The page (`view/task.rs`, `manage/tasks.rs`) still opens over Details as before, not
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
  left, and the keyboard goes back to the row; a thread whose item left the box closes. The dialog that showed a
  task's transcript is gone. iPhone: the row
  pushes `WorkThreadView` inside the details sheet; the system bar is hidden (`hidesSystemNavigationBar`, which keeps
  the edge swipe back) and the page draws its own header, as Android's: a `Back to details` icon button, the title
  over the status line, Stop. Android: `WorkThreadView` is pushed inside the sheet by the design module's
  `NavStack`, with the same header. A row of the Work sheet opens the bot's chat with its
  details on that thread.
- **A row that needs you still opens the chat at its card**, not the thread. In the item's thread the card is one
  line (below), which opens the chat at it; the thread has no card to answer.
- **Header**: title, the status word with its mark and the time, the where line, Stop while the item is live (it
  asks first).
- **Transcript**: `What it was told` folded on top (the host's first words to the task); what the host told it
  later (an item picked up again) is a quiet quote where it happened; the user's messages are the chat's bubbles;
  the task's final replies (`isThreadMessage`: kind `agent` and `final`) are the chat's agent message, read only
  (Copy; no reactions, reply or trace); the rest are the chat's working steps, the running turn's unfolded. A task's
  own transcript holds no file or artifact (what it shares goes to the chat), so its replies are text on every
  client. What the task put in the bot's chat (the entries whose `data.task.id` is the thread's task: approval and
  choice cards, answered ones too, its agent's sign-in card, files, voice notes, artifact cards) joins the transcript in time order
  (`createdAt`, each side kept in its order, the task's own first at the same moment) as one quiet line with a
  symbol (`sharedLine`: `hand.raised` while it waits on the user, `checkmark`, `xmark` or `clock` once answered or
  not, `doc`, `waveform`, `doc.richtext`; a sign-in card reads as the card stands: `Needs <agent> sign-in`, `Signed
  in`, `Cancelled`, not `Asked in the chat to sign in to <agent>.`, which the task's own transcript already has
  beside it). The full card, file or artifact stays in the chat, where the coordinator
  shows it. A tap or click on the line opens the chat at that entry: the desktop scrolls the chat beside the panel
  to it (`showInChat`, by `data-entry-id`); the phones close the details sheet and bring it into view, as a
  needs-you row does (iPhone `BotDetailsView.openInChat`, Android `openCard`). Only the chat's loaded messages are
  looked at, as for a needs-you row.
- **Composer**: the chat's, text only (`Message this thread`): no files, dictation, commands, reply or call. Each
  client keeps a draft per item, under the bot's (desktop `<bot>/thread/<item>`, phones `<bot>/item/<item>`), so it
  goes with the bot. Off while the computer is offline.
- **Messages that wait.** Desktop: the bubble stays in the transcript with `Queued`, **Send now** and **Remove**
  under it. iPhone and Android: they leave the transcript for the chat's tray above the composer, with the same
  two buttons. On the desktop a message that never arrived has Resend and Delete, and one dropped by Stop reads
  `Not sent — stopped`; the phones show the chat's own lines for both.
- **Read-only threads**: no composer and one quiet line, `Read only: a routine's run` or `Read only: another bot's
  request`. An own item whose task is gone (`talk` false) has neither.
- **Where line.** Every client writes the computer's home (`hello.home`) as `~`, with one rule (`homeRelative`,
  the terminal's `tilde`): at a separator boundary only, `/` or `\`; any other path shows whole and is cut in the
  middle.
- **Git state** in Settings > Workspace as [above](#stage-11-talking-to-a-thread): `GitStateLine.tsx`,
  `GitStateRow.swift`, `GitLine` in `BotSettingsForm.kt`. The button reports `link_opened` with `link: docs`.
- **Analytics**: screen `task` is the thread view; the rest is the host's (`work_messaged`, `work_steered`). The
  tap on a shared line is navigation and reports nothing of its own.

**What no test covers.** The desktop's Playwright spec (`e2e/tasks.spec.ts`) and the phones' Maestro flow
(`apps/maestro/tasks.yaml`) walk opening a thread from its row, writing to a working thread (the message waits,
Send now and Remove show) and, on the desktop, reopening a done item, a card's line in an open thread that shows
the card in the chat and turns `Answered` once it is answered there, and a routine run's read-only line. Neither
has a step for: copying the path from the header, the git state line and its button, Escape, Resend of a failed
message, a shared line on the phones, or a Windows path in the where line.
Whether a real iPhone, a real Android phone, and the desktop on Linux and Windows were looked at is not recorded
here.
