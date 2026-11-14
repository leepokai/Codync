# Background tasks

Status: **stage 1 (host) built**, 2026-11-15, on `feat/background-tasks`; clients not yet. Answers stage A of the
[personal agent plan](personal-agent-experience.md) ("whether and how conversations and ACP sessions are reused,
separated or resumed") and is its Task model.

A bot keeps answering in its chat while a second session of the same bot does long work. The chat shows only
messages and what the user must act on; the work shows as one capsule above the composer.

## Words

- **Main conversation**: the bot's chat and its main session, as today. It triages: short things it answers,
  long work it hands to a task.
- **Task**: one piece of long work the bot does away from the chat. A task has a title, its own session, its own
  agent process, its own transcript and a status. The bot is the same bot: same name, instructions, memory, folder.
- **Subagent**: what any session delegates through its harness's own tools. Codync displays them; it doesn't run them.

In code the feature is `tasks` (`background_work.rs` and `background.rs` already mean detached processes and host
services). On screen it is "in the background".

## Decisions made with the owner

1. The chat shows text and what needs the user. No tabs, no task cards, no suggested prompts.
2. One capsule above the composer shows the task; a tap opens a read-only detail sheet.
3. Approval and question cards from a task appear in the main chat. The user never types into a task: they tell
   the bot, and the bot passes it on with the user's own words.
4. One task at a time unless a second is really needed. Parallel parts of a task are the task's own subagents.
5. A task starts by forking the main session where the agent can; otherwise from a new session and a brief.

## Shape

```
user ── chat ──▶ main conversation (actor: the bot)
                   │  start_task / tell_task          ▲ report (a turn in the chat)
                   ▼                                  │
                 task (actor: a task bot, own process, own session, own transcript)
                   │ harness subagents
                   ▼
                 shared: memory folders, workspace, connectors, the user's approval setting
```

### Why a task is a bot of its own inside the host

A second actor under the same bot id can't work: the handle map, the runtime (status, activity, `started_at`,
`message`, `session`, `lane`), team turns, routine and follow-up state, memory's turn tracking, the restart mark,
`bots.session_id`, fallback keys and compaction windows are all single slots keyed by bot id
(`hub.rs`, `agent/bot/`, `chat/team/requests.rs`, `routines/`, `chat/memory/turns.rs`). A second session inside the
one actor doesn't work either: it drops every `session/update` that isn't the running turn's, routes permission
requests without checking the session, and tool calls carry only `botId` (`agent/bot/updates.rs`, `mcp.rs`).

So a task is a **task bot**: a `bots` row whose config has `taskOf = <bot id>` (id `task-<uuid>`; no schema
change, the config is JSON), copied from its bot when it starts. It gets its own actor, runtime, chat (its transcript), session, agent process, restart mark, fallback
state and compaction record, all by the existing code. What it must not have of its own points back to its bot:

| Belongs to the task bot | Read from its bot |
| --- | --- |
| actor, queue, agent process, session | name, instructions, avatar (copied at start) |
| transcript (`entries.bot_id = task id`) | memory layers, their kv keys and the instruction snapshot (`BotConfig::as_owner()`) |
| runtime (status, activity, `started_at`) | approval setting, read at each request |
| restart mark, fallback state, compaction | `search_history` (searches its bot's chat) |

A task bot never reaches a client as a bot. `Store::bots()` leaves task bots out (one filter; `Store::task_bots()`
for the hub), `emit_bot` for a task bot emits its bot's event instead, and its entries leave as `taskEntry` events
(below). Every caller of `bots()` is checked when this lands: roster, `list_bots`, `list_team`, groups, usage, order.

One process per task, always. Two sessions prompting in one process is not built: the actor's routing above makes it
unsafe, and a task's deadline or cancel must not kill the main session's agent.

## Starting a task

The main session calls `start_task {title, brief, parallel?}`, one of four tools added to the built-in `chat` MCP
server (no new server: the agents' allow lists and server sets stay as they are). The host records the
task (`starting`), the capsule appears, and the tool returns at once. The task launches **when that turn ends**: a
fork taken mid-turn would carry an unanswered tool call. A Stop on that turn cancels a task that hasn't launched.

The task bot's first turn opens its session one of two ways:

- **Fork** (`session/fork` of the main session, from the task's own process). Conditions: the agent advertises
  `sessionCapabilities.fork`; the bot is on its own agent (not a fallback); the main session's saved usage
  (`compaction.<bot>.<session>`) is under half its window. The request carries the main session's frozen
  instructions (`context::snapshot(main).system`, Claude `_meta` or ACP `systemPrompt`) and the same MCP server list,
  so the prompt cache is shared; then `context::adopt` and `memory::opened` with the parent's memory revision. This is
  the removed thread fork (`ecc955018da2`, [threads](threads-for-teams.md#session-fork-as-it-was-implemented)) with a
  different caller.
- **Brief** (`session/new`). Used when a condition fails, or the fork errors or times out (logged, counted, never
  shown as a failure). Instructions are a fresh `context::render`; the first prompt carries what a fork would have
  known, built by the code that already does it: the rollover handoff (`handoff::build`: summary, last user messages,
  last reply, plan) sized by `carry::room`.

Either way the first prompt ends with the task itself:

```
[Task] You are continuing as the same bot, away from the chat, on: "<title>".
<brief>
[The user's own words: «…»]
Work until it's done. Your last message is the report the chat gets. Progress you send with send_message shows in
the task's details, not in the chat. Approvals and questions reach the user in the chat.
```

The user's own words are the user messages of the turn that called `start_task` (`turn_entries`), word for word, as
voice calls do (`calls::tools::relay`).

The task session gets the same MCP servers as the main one (a different tool list would break the fork's cache),
except `computer` and `manage`, which a task never has. The host knows a call comes from a task by its bot id and
refuses what a task may not do, the way a request turn is refused today (`tasks::refuse_in_task`): the task tools,
`follow_up` and the other `routines` tools, `message_bot`, `request_decision`, `send_file`, artifacts, and changes to
the bot's profile or fallbacks.

### One at a time

`start_task` while a task is live fails with "a task is running: add to it with tell_task", unless `parallel: true`.
Hard limits: 3 live tasks per bot, 6 per computer; refused while the agent is near its usage limit
(`usage::near_limit`). The bot's instructions (`tasks::BRIEF`, in the context snapshot, plus a line in each turn's
prompt while a task is live) say: start a task for work of more than about a minute; give several things to one task;
ask for `parallel` only when the user wants two unrelated things at once.

A parallel task in a git repository works in its own worktree (`<data>/bots/<bot>/tasks/<task>/`, branch
`codync/<slug>`); the first task works in place. Outside a repository both are told another task works there.
Worktrees ship after the single task (stage 3).

## Talking to a task, and its report

- **`tell_task {message, task?}`**: the host posts the message in the task's transcript as a user entry with
  `from: "host"` and the user's own words of this turn attached, through `messages::post`. The task bot's queue does
  the rest as for any message: folded into its next turn, or steered into the running one where the agent supports
  steering. A finished task takes it as a new turn (its session is reopened) and reports again.
- **`task_status {task?}`**: status, activity, elapsed time, its last three messages. The bot answers "what is it
  doing?" from this; a voice call gets the same through `bot_status`.
- **`stop_task {task?}`**: as the user's Stop.
- **Report**: when a task's turn ends, its final reply goes to its bot as `Cmd::TaskReport` and becomes a host turn in
  the main session (`HostTurn::TaskReport`, queued like a follow-up, reports arriving together taken in one turn):
  `[Codync: your task "<title>" finished. Its report: …]`, or `failed: <reason>`, or `was stopped by the user` (no
  turn: the user did it). That turn's reply is an ordinary chat message with the usual *done* alert; `(pass)` keeps it
  silent, and then the host sends the alert itself. The report is kept on the task until its turn has started
  (`report_pending`, as the routine outbox), so a restart loses none.
- Reports are not the user speaking: `from: "host"`, so memory and dreaming don't take them as the user's words.
- A report turn may start a task. Tasks started without a user turn in between are counted (`hops`, as bot requests);
  the eighth is refused.

## Approvals and questions

- A task's permission request becomes a card **in its bot's chat** (`Lane::main(bot)`), authored by the bot, with
  `data.task = {id, title}`. `respondPermission` and `respondDecision` route by `data.task` first, then `data.author`,
  so the answer reaches the task's actor (the group precedent: a card sits in one lane and is answered by another
  actor). Push, unread count and a voice call's `answer_approval` work unchanged: the card is an entry in the chat.
- The approval setting is the bot's, copied when the task starts. A task never has more than its bot;
  `start_task` has no permission argument.
- **Allow all for this task** (not built; owner decision below): the host adds one option to a task's cards. It
  answers this card and sets the task to approve automatically until it ends. Only the user can press it.
- A waiting card stops only its task. Cards wait without a timeout; a stopped or failed task expires its cards.
- **Not in the first version**: a task can't ask a question (`request_decision`) or share a file or artifact. Both
  are refused with "say in your report what you need", so the question or the path reaches the user through the
  bot's own message, and the answer goes back with `tell_task`. Cards for them in the chat come later.

## Memory and context

- **Same memory.** A task reads and writes its bot's layers (`memory_bot()` in `files::folder`, `layers`, the
  `memory.revision` / `announced` / `opening` / `putback` keys, `memoryCall`). Writes are already safe across sessions
  (the global `SCANNING` lock, compare-and-swap on content, one transaction per write).
- **How it reaches memory.** The task's `memory` MCP server is started for its bot (`--bot <bot id>`), and the
  actor's memory and snapshot calls use `as_owner()`, so the memory module itself is unchanged.
- **Who wrote it (first version's limit).** Because the tools run as the bot, a task's `edit_memory` is credited to
  the bot's runtime: the chat's session, and "the user asked" only while the user's own message is being answered
  there. A task's edits through its agent's file tools aren't tracked as a turn's (the tracker keeps one turn per
  bot); the bot's next scan takes them as changes made outside a turn. Per-session turn tracking fixes both.
- **What a task may change.** With no user message being answered in the chat, the existing rule holds: it may add
  lines and notes, not rewrite or bring back forgotten facts.
- **Hearing each other.** Change notices are per session (`memory.announced.<bot>.<session>`): each session gets a
  `<memory_update>` at its next turn after the other wrote.
- **Dreaming** stays the bot's: a task bot spawns no dreamer, and the bot's dreamer reads the bot's chat, where the
  user's words already are. A task's transcript is the bot's own work and is not read.
- **The harness's own memory** switch applies to the task's process as to the bot's (`own_memory::apply` at launch).
- **Compaction and rollover** work per session as today: the task's transcript is its own main chat, so its
  compaction record is saved by the existing check; a rollover of the main session doesn't touch a task.
- **`search_history`** in a task searches the bot's chat. Task transcripts are not searched in this version.
- **Per-task progress notes** are not in this version; the transcript and the report carry the state.

## How the rest of Codync meets a task

| Existing feature | With tasks |
| --- | --- |
| `message_bot` to this bot | Unchanged: the request waits for and runs in the main session. A later stage may run requests as tasks so the chat isn't held. |
| `message_bot` from a task | Refused in this version; a task that needs another bot says so in its report. |
| Groups | A room turn can't start a task (as it can't `message_bot`). Tasks don't speak in rooms. |
| Routines | Unchanged in this version: a run still takes the main actor. Stage 4 runs them as tasks, so the chat isn't held during a run. A routine run can't start a task. |
| Follow-ups | Main conversation only. A follow-up turn may call `task_status`. |
| Lead bot, Check in | Unchanged. |
| Voice calls | `send_to_bot` reaches the main conversation, which may start a task. The call's watcher adds a task's activity to its progress cues and names the task in approval cues; `bot_status` includes tasks; `stop_bot` stops the task too. |
| Agent fallbacks | A task has its own chain state; rests are the computer's, so shared. A task on a fallback agent starts from a brief. |
| Login profiles, usage | The task's process uses the bot's login; its turns count toward the same usage. |
| Dev servers | Servers a task starts are owned by its bot, so they outlive the task. |
| New session, model or folder change | A live task keeps the config it started with. The next task starts from the new one. |
| Deleting the bot | Stops and deletes its tasks. |
| Cloud computers | A working task keeps the computer awake (its runtime is `working`). |
| Other computers (network) | Not involved: a task runs on its bot's computer. |

## Lifecycle

`starting → working ⇄ needs you → done | failed | stopped`

- The task's session is closed and its process stopped when its turn ends (`release_spent`, as a routine run).
  `tell_task` reopens it.
- Host restart: a task bot is spawned like any bot, and its restart mark resumes its turn (`Inflight`); a task still
  `starting` launches; a pending report is delivered.
- No deadline: the user sees it and can stop it.
- A finished task and its transcript are kept 7 days, then deleted at the next host start.

## Wire

All additive; no `MIN_APP` change. A client shows the feature when the bot carries `tasks`.

- Bot: `tasks: [{id, title, status, activity, startedAt, start: "fork" | "brief"}]`: live tasks only, in start order.
  The bot's own `status` stays the main conversation's (the composer's send/stop depends on it).
- Entries of a task: `{"type": "taskEntry", rev, task, entry}` events, not `entry` (older apps ignore unknown types and
  would otherwise mix them into rosters and chats); not in the catch-up.
- `taskEntries {botId, taskId}` → the task's newest 500 entries (as `callEntries`). `stopTask {botId, taskId?}`.
- Chat entries from a task (cards, files, the report's reply) carry `data.task = {id, title}`.

## Screens

One change on every client, in the same release (iPhone and Android checked together).

- **Capsule**, in the slot above the composer (where queued messages sit): spinner, `In the background · <activity>`,
  elapsed time; `Needs you` while a card waits; `Stop` (asks once). Two tasks: `In the background · 2 tasks`. Gone when
  no task is live. iPhone and Android: the call bar's glass capsule with the working line's content; desktop: in
  `.composer-dock`; terminal: a line like the queue band, one key to open, one to stop.
- **Details**: a read-only sheet (desktop: the side panel, a sheet when narrow) built like the call conversation
  (fetch by id, merge live events, loading / error / empty) with the trace's rows (`TraceRow`): the task's messages,
  thinking, tool calls. Header: title, status, elapsed, Stop. No composer. Two tasks: a list first.
- **Cards in the chat** say where they come from, in the line that says "Runs on …": `In the background · <title>`.
- **A report's message** offers *Show what it did*, which opens the task's details instead of the turn's trace.
- **Roster row**: `In the background: <activity>` when the chat itself is idle. iPhone Live Activity (while the app is
  open; pushes carry no free text), Android's ongoing notification and the desktop island show the same line.
- **Subagents** in details: a group of rows (title, status, elapsed, last line) where the harness's tool calls can be
  recognized (spike 5); otherwise they show as the tool calls they are, as in the trace today.

## Client spec (stage 2)

The same on iPhone, Android, desktop and the terminal. Reads only the [wire](../features/background-tasks.md#wire).

**Data.** `bot.tasks` is absent on a host from before tasks: read it as empty, and the feature doesn't show. Each
task: `{id, title, status: "starting" | "working" | "needsInput", activity, startedAt, start}`. `taskEntry` events
carry a task's transcript entries (`{task, entry}`): keep them out of every chat, in a store of their own per task.
`taskEntries {botId, taskId}` answers `{task: {id, title, status: starting | working | done | failed | stopped,
startedAt, endedAt, start}, entries}` (oldest first). `stopTask {botId, taskId?}` stops one task, or all of the bot's.

**Capsule.** In a bot's own chat (never a group), in the slot directly above the composer, shown while `tasks` isn't
empty; it animates in and out like the other things that appear there.

- One task: a spinner, then `In the background · <what>`, the elapsed time since `startedAt` (the working line's
  timer format), and a Stop button. `<what>`: `Needs you` when `needsInput`; `Starting…` when `starting`; otherwise
  `activity`, or the task's `title` when `activity` is empty. One line, truncated in the middle part.
- Several: `In the background · N tasks running`, and ` · needs you` when any is `needsInput`. The time is the oldest's.
- Tapping the text opens the details. Stop asks once with the client's own dialog (`Stop the background work?` /
  `What it already changed stays.` / `Stop`, `Cancel`), then calls `stopTask` (with `taskId` for one task, without
  for several).
- Black and white, the client's existing capsule and spinner components; no new colors, no border on a filled shape.

**Details.** Read only: no composer. Header: the task's title; under it its status in words (`Working`, `Needs you`,
`Starting…`, `Done`, `Failed`, `Stopped`) and elapsed time; a Stop button while it is live; the client's usual close.
Body: the task's entries, fetched with `taskEntries` when it opens and kept current from `taskEntry` events, each
entry once (the higher `rev` wins), rendered with the rows the Full conversation / trace view already has (messages,
thinking, tool calls with their output one tap away). The task's own user entries (`data.from == "host"`) are what it
was told: show them as plain quoted text, not as the user's bubbles. Loading, error (with retry) and empty states as
the call conversation sheet has. With several tasks the capsule opens a list first (title, status, time); a row opens
that task. iPhone and Android: the system sheet (`.codyncSheet` / `CodyncSheet`); desktop: `Sheet`; terminal: a
full view like the bot conversation view, closed with `Esc`.

**Cards.** A permission card whose entry has `data.task` shows one quiet line, `In the background · <data.task.title>`,
where the card says where it runs. Nothing else about the card changes; it is answered as any card.

**Roster.** When a bot has tasks and its own `status` is idle, its row's preview reads `In the background: <what>`
(`<what>` as in the capsule). Otherwise the row is as today.

**Words.** Every string goes through the client's own localization, English and Traditional Chinese at least:
`In the background` = `背景`, `Needs you` = `需要你`, `Starting…` = `開始中…`, `N tasks running` = `N 件事`,
`Stop the background work?` = `停止背景的工作？`, `What it already changed stays.` = `已經改的內容會留著。`,
`Working` = `進行中`, `Done` = `完成`, `Failed` = `失敗`, `Stopped` = `已停止`.

**Analytics.** Opening the details reports `screen_viewed` with screen `task`.

**Not in this stage.** Live Activity, Android's ongoing notification, the desktop island, voice cues, a report's
*Show what it did*, subagent rows.

## Analytics

Host events, no content: `task_started {start: fork | brief, parallel}`, `task_fork_failed {error}`,
`task_finished {outcome: done | failed | stopped, duration}`, `task_told`, `task_refused {reason: live | limit | usage |
hops}`; `stopTask` in `observe_call`; `task` in the apps' `Screen` list. Listed in
`docs/reference/analytics-events.md` in the same change.

## Stages

The owner wants the whole reorganization, not only the first slice (2026-11-15: "要完成整個完整所有的架構重組").
Each stage is its own change through `after-change`, on all four clients where it touches a screen, and merges into
`dev` when it is green. Tick a stage only with its evidence.

- [x] **0. Spikes** 1 to 4 (below), with Claude.
- [x] **1. Host**: `BotConfig::task_of`, `chat/tasks.rs`, the actor's hooks, the sync changes, `taskEntries` /
  `stopTask`, analytics. `tests/tasks_e2e.rs`; a real Claude bot started a task by fork, worked, and reported.
- [ ] **2. Clients**: capsule, details, card line, roster line on iPhone, Android, desktop and terminal. Built and
  unit-tested on all four; desktop walked with Playwright and with a real Claude bot. Still owed before merge: the
  walk on the iOS Simulator and the Android emulator (Maestro `tasks.yaml`).
- [ ] **3. What a task still can't do from where it is**: `request_decision` and `send_file` / artifacts from a task,
  as cards and messages in the chat (today refused); *Allow all for this task* on its cards.
- [ ] **4. Voice calls**: a task's activity in the call's progress cues, tasks in `bot_status`, `stop_bot` stops
  tasks, approval cues name the task.
- [ ] **5. Routines run away from the chat**: a run takes a task bot of the routine's bot instead of the chat's
  actor, so the chat isn't held during a run; its result still lands in the chat, and the bot's main session hears
  of it at its next turn. Keeps the run's own machinery (deadline, retries, progress note, recovery, run history).
  Open points to settle first: the run's MCP servers act as the bot (`save_progress`, `follow_up`), entries are
  authored by the bot, `Routines.active` stays one run per bot, the capsule shows a running routine.
- [ ] **6. Requests from other bots** (`message_bot`) run as tasks of the recipient, so its chat isn't held; the
  reply goes back as today.
- [ ] **7. Parallel tasks' worktrees** (the second live task in a git repository).
- [ ] **8. What shows where**: subagent rows in details (spike 5), a report's *Show what it did*, the iPhone Live
  Activity, Android's ongoing notification and the desktop island.
- [ ] **9. Memory credit per session**: turn tracking keyed by session, so a task's memory writes are its own.
- [ ] **10. Other agents**: spikes 1 to 3 for Codex, Cursor and the rest; each gets fork where it passes.

## Spikes

| # | Question | If yes | If no |
| --- | --- | --- | --- |
| 1 | Which agents advertise `sessionCapabilities.fork`? **Claude: yes** (`claude-agent-acp` 0.107.0, 2026-11-15). Others untested. | Fork for those | Brief |
| 2 | Does `session/fork` from a second process work on a session another process holds, right after its turn? **Claude: yes**: the fork knew the chat, and the chat's session never saw what the fork did after. | Fork as designed | Brief for that agent |
| 3 | Does the fork read the prompt cache (same instructions and tools)? **Claude: yes** (41,453 of 42,425 input tokens read from cache on the fork's first turn). | Keep the half-window rule | Lower it, or prefer a brief |
| 4 | With `tasks::BRIEF`, does the agent call `start_task` rather than do the work in the chat's turn? **Claude (haiku): not with the instructions alone** (two tries, 85 to 95 s of reading in the chat's turn); **yes with one line on each message** (`tasks::EACH_TURN`), and a quick question was still answered in the chat. | Ship | Reword; add the per-turn line |
| 5 | Can a subagent's start, end and result be read from the adapter's tool calls? | Subagent rows for that agent | Plain tool rows |

If 1–3 all fail everywhere, every task starts from a brief: the feature is whole, a task just starts from a summary.

## Owner decisions still open

- **Allow all for this task** on a task's cards: proposed yes.
- **Parallel tasks**: the second gets a worktree (proposed), or parallel tasks are never allowed.
- **Limits**: one by default, 3 per bot and 6 per computer when asked (proposed).

## Not in this version

Tasks inside tasks; a task on another bot or computer; a model per task; a merge button or diff view for worktrees;
computer use in a task; read-only tasks; typing into a task; per-task progress notes; searching task transcripts.

## Docs to change when it ships

[conversation runtime](../architecture/conversation-runtime.md), [chat messages](../features/chat-messages.md),
[context and memory](../features/context-and-memory.md) (its `context.<bot>` keys are per session already; the fork
paragraph is stale), [bot collaboration](../features/bot-collaboration.md), [voice call](../features/voice-call.md),
[routines](../features/routines.md) (stage 4), a new `features/background-tasks.md`, the user manual page, and this
plan moved to the archive.
