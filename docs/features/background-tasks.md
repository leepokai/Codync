# Background tasks

A bot hands long work to a **task**: another session of the same bot that works away from the chat, so the chat
keeps answering. The chat shows only messages and what needs the user (a task's approval cards); the task shows as
one capsule above the composer, and its report comes back as the bot's own message.

| | The chat | A task |
|---|---|---|
| Session | The bot's main one | Its own, forked from the main one where the agent can |
| Agent process | The bot's | Its own |
| Transcript | The chat | Its own, read from the capsule's details |
| Who talks to it | The user | The bot (`tell_task`) |
| Its result | A message | A report the bot turns into a message |

Design and what is still to build: [the plan](../plans/background-tasks.md). Code: `host/src/chat/tasks.rs`; the
actor's side is in `host/src/agent/bot/` (`session.rs` for the fork, `turn.rs` for the launch and the report,
`updates.rs` for cards).

## A task is a bot of its own inside the host

Everything a running turn touches is keyed by bot id, so a task is a **task bot**: a `bots` row with
`taskOf = <bot id>` and id `task-<uuid>`, its config copied from its bot when it starts (without `computer` and
`manage`). It has its own actor, agent process, session, transcript, restart mark, fallback state and compaction
record. Its memory and instruction snapshot are its bot's (`BotConfig::as_owner`; its `memory` MCP server is started
for its bot).

No client lists a task bot. `Store::bots()` leaves them out, `emit_bot` for one emits its bot instead, and its
entries leave as `taskEntry` events, not `entry`.

## Tools (on the `chat` MCP server)

- `start_task {title, brief, parallel?}`: records the task (the capsule appears) and answers at once. The task
  launches **when that turn ends**: a fork taken mid-turn would carry an unanswered tool call. Stopping that turn
  cancels it.
- `tell_task {message, task?}`: a message in the task's transcript, with the user's own words of the running turn
  attached. The task's queue takes it like any message (folded into its next turn, or steered into the running one).
  A finished task takes it as more work and reports again.
- `task_status {}`: each task's status, current step, seconds, latest three messages.
- `stop_task {task?}`.

Refused in a task (`tasks::refuse_in_task`): these four, the `routines`, `team` and `manage` tools,
`request_decision`, `send_file`, artifacts, and changes to the bot's profile or fallbacks. The refusal says to put
what it needs in its report.

`start_task` is refused outside the bot's own chat (a room, another bot's request), while a task is live unless
`parallel: true`, past 3 live tasks for the bot or 6 on the computer, while the agent is near its usage limit, and
for the ninth task started in a row with no message from the user.

The bot's instructions carry `tasks::BRIEF`, and each message in its own chat ends with `tasks::EACH_TURN`: with the
instructions alone a real agent did 90 seconds of reading in the chat's turn (checked with Claude on 2026-11-15; with
the line it started a task, and answered a quick question itself).

## How a task's session begins

- **Fork**: `session/fork` of the bot's main session, sent from the task's own process with the main session's
  frozen instructions, so the prompt cache is the chat's. Conditions: the agent advertises
  `sessionCapabilities.fork`, the bot is on its own agent (not a fallback), and the main session has used under half
  its window.
- **Brief**: `session/new`, with the chat so far before the task's first message (`handoff::build`: summary, last
  requests, last reply, plan). Used when a condition fails or the fork errors.

The task's first message says what it is, gives the brief and the user's own words, and how its words reach the
user (`tasks::first_prompt`).

## Report

When a task's turn ends, its last message goes to its bot as `Cmd::TaskReport` and becomes a turn in the chat
(`HostTurn::TaskReport`; reports that arrive together are one turn). The bot's reply there is an ordinary message
with the usual alert; `(pass)` stays silent. A report is kept on the task until that turn has started, so a host
restart sends it again. A task the user stopped reports nothing. Reports are `from: host`: memory and dreaming don't
take them as the user's words.

## Approvals

A task's permission request becomes a card in **its bot's chat**, with `data.task = {id, title}` and the task as
`data.author`, so `respondPermission` reaches the task's actor. The approval setting is the bot's (copied when the
task starts). A waiting card stops only its task; the chat goes on.

## Wire

- Bot: `tasks: [{id, title, status: starting | working | needsInput, activity, startedAt, start: fork | brief}]`,
  the ones still going. The bot's own `status` stays the chat's.
- `taskEntry` events `{rev, task, entry}` for a task's transcript; none in the catch-up.
- `taskEntries {botId, taskId}` → `{task, entries}` (newest 500). `stopTask {botId, taskId?}`.

## In the apps

The same on iPhone, Android, desktop and the terminal ([client spec](../plans/background-tasks.md#client-spec-stage-2)).

- **Capsule** above the composer of the bot's own chat while it has tasks: `In the background · <what>`, the time,
  Stop (asks once, then `stopTask`). `<what>` is `Needs you`, `Starting…`, the task's activity, or its title; with
  several tasks, `N tasks running`. Terminal: a line over the composer; `B` opens, `X` stops (after `esc` while typing).
- **Details**, from the capsule: the task's own transcript, read only, fetched with `taskEntries` and kept current
  from `taskEntry` events while it is open. A sheet on the phones and the desktop, a full view in the terminal.
- **A task's approval card** in the chat says `In the background · <title>`.
- **Roster row**: `In the background: <what>` while the chat itself is idle.
- Opening the details reports `screen_viewed` with screen `task`.

Code: desktop `src/shared/tasks.ts`, `views/thread/TaskCapsule.tsx`, `TaskDetails.tsx`; iPhone
`CodyncKit/Models/BotTask.swift`, `CodyncUI/Thread/TaskCapsule.swift`, `TaskDetailsView.swift`; Android
`core/Tasks.kt`, `TaskCapsule.kt`, `TaskDetailsView.kt`, `AppStoreTasks.kt`; terminal `tui/app/tasks.rs`,
`manage/tasks.rs`, `view/tasks.rs`.

## Restart and cleanup

A task bot is spawned at host start when its turn was cut off (its restart mark resumes it); one still `starting`
launches; one that was working with nothing left to resume fails with a report. A finished task is kept 7 days.
Deleting a bot deletes its tasks.

## Limits of this version

- A task's memory tools run as its bot, so a task's `edit_memory` is credited to the chat's session, and its edits
  through file tools are taken at the bot's next scan as changes outside a turn.
- No worktree for a parallel task; no subagent rows.

Tests: desktop `e2e/tasks.spec.ts` (capsule, details, stop, a task's card) and each client's unit tests of the
label, the decoding and the transcript merge; `host/tests/tasks_e2e.rs` (fork, brief, a card answered from the chat, refusals in a task, one at a time,
tell, stop, a host restart mid-task) with `tests/fixtures/task_agent.py`.
