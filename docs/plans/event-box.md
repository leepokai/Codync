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
answers itself. In this version only the coordinator adds items; routines and other bots' requests keep their own
paths and join the box in a later stage.

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
| A running task | A task's session is never written into by the user. To put an item on a busy bot **now**, a task gives way: its work is closed and a new session starts for the item. |

**Cutting in** (`now`), when every place is taken: the user picks which working item gives way (or the coordinator
names it; with none named, the one that started last). That task is stopped and deleted, its item returns to the
front of the box with `cutIn` set, and starts again later in a new session from its brief and notes. The freed place
starts the item that cut in. With a place free, `now` is just `first`.

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
| **Threads** | *(owner: the running sessions and the box are told apart.)* The agent sessions that run now. The heading carries `2 / 3`: how many run, of the most allowed. Items that need you first (the attention mark, what the card asks; opens the chat at the card), then working items (a moving dot, the task's activity, elapsed time, Stop). A row opens the task's transcript. A place not in use is one faint row, `Free`, so the ceiling is visible. |
| **Event box** | What waits for a thread, numbered 1, 2, 3 in the order it will be taken, with why it waits. The heading carries how many. **First** on every row but the first; **Cut in** on the first row when every place of the bot is taken, which asks which working item gives way; **Drop**. Left out when nothing waits. |
| **Later** | Follow-ups the bot set (the dashed card, as today). |
| **Routines** | As today, with **+**. |
| **Dev servers** | As today, when the bot has some. |
| **Recent** | Items ended in the last day: done (a ring, time taken; opens the transcript), stopped and failed (the danger mark, why; **Try again**). |

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
the same sections; on a row `↵` opens the transcript, `X` stops (asks), `F` moves first, `C` cuts in (asks which),
`D` drops, `A` tries again. Agent and Instructions are on its Settings page.

**Words** (English key = Traditional Chinese): `Threads` Threads, `Event box` Event Box, `Free` 空位,
`Later` 之後, `Recent` 最近,
`{count} working` {count} 進行中, `{count} waiting` {count} 等待中, `Next up` 下一個, `{count} ahead` 前面還有
{count} 件, `Follows what it works on` 接在進行中的事後面, `Waiting for usage` 等用量恢復, `Was cut in on; starts
again` 被插隊，會重新開始, `First` 先做, `Cut in` 插隊, `Drop` 丟掉, `Try again` 再試, `Agents at once` 同時幾個
agent, `Let "{title}" cut in` 讓「{title}」插隊, `Every place is taken. Pick what gives way: its session closes and
it starts again later.` 名額滿了。選一個讓位：它的 session 會關掉，之後重新開始。 The strings of 2.48.0 that still
apply are reused; `{count} tasks running` goes.

## Restart, failure, limits

- At host start: a working item whose task resumes stays working; one whose task can't goes back to waiting once
  (as an item that was cut in on, from its brief), then fails.
- A task that fails fails its item with the reason; the coordinator hears it in a report.
- Stopping a working item stops its task; items waiting `after` it become ordinary waiting items.
- Items ended over seven days ago are deleted, and a task bot with no item left is deleted with its transcript.
- Chains: items added from a report's turn carry its hops; the ninth is refused.

## Analytics

`work_added {outcome: added | elsewhere | invalid | full | hops | failed, after}`, `work_finished {outcome, start,
forkFailed, minutes, error}`, `work_steered {how, by: bot | user}`, replacing the `task_*` events. Screen `task`
stays for a task's transcript.

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

## Stages

- [x] **1. Host**: items and the dispatcher in `chat/tasks/` (the task bot machinery stays), the tools, the API, the
  wire, `maxAgents`, analytics, tests (`tests/tasks_e2e.rs` rewritten for items: the ceiling, order, first, cut in,
  replace, after, stop, retry, restart), a real-agent check.
- [x] **2. Clients**: capsule, Details, Settings, cut in, roster, on iPhone, Android, desktop and terminal.
- [ ] **3. Other sources**: routines and bot requests as items.
- [ ] **4. One view of all bots' work.**
- [ ] **5. A task asks and shares through the chat**: `request_decision`, `send_file` and artifacts from a task, as
  cards and messages in the bot's chat (today refused: the task says what it needs in its report).
- [ ] **6. Voice calls know the box**: built: items in `bot_status` and the call's opening context, `stop_bot` stops
  them too (a task's approval card was already heard and answered from a call). Left: a task's activity in the
  call's progress cues, approval cues that name the item.
- [ ] **7. Worktrees** for tasks that run side by side in one git repository.
- [ ] **8. The same line elsewhere**: the iPhone Live Activity, Android's ongoing notification and the desktop island.
- [ ] **9. Memory credit per session**: turn tracking keyed by session, so a task's memory writes are its own.
- [ ] **10. Fork for other agents**: the fork spikes
  ([run with Claude](../archive/background-tasks-plan-2026-11-15.md#spikes)) for Codex, Cursor and the rest; each
  forks where it passes, and starts from a brief until then.

Not in this version: an item given to a running task by the dispatcher (only `after`); a worktree per parallel task;
priorities the user sets; preempting without being asked.
