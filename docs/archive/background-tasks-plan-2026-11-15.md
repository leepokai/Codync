# Background tasks: the first design

Archived 2026-11-15. This was the plan for background tasks as merged in 2.48.0: one task at a time, started and
steered by the bot with four task tools. Before any release it became a queue of items
([the event box](../plans/event-box.md)), which replaced those tools, the `stopTask` method, the bot's `tasks` list,
the `task_*` analytics events and the limits by bot and by computer. **Current:**
[the event box](../plans/event-box.md) (the design and what is left) and
[background tasks](../features/work.md) (what is built). What stays here is the reasoning that still
holds: why a task is a bot of its own, fork or brief, the report, and the spikes.

## Decisions made with the owner

1. The chat shows text and what needs the user. No tabs, no task cards, no suggested prompts.
2. One capsule above the composer shows the work.
3. Approval cards from a task appear in the main chat. The user never types into a task: they tell the bot, and the
   bot passes it on with the user's own words.
4. A task starts by forking the main session where the agent can; otherwise from a new session and a brief.

## Why a task is a bot of its own inside the host

A second actor under the same bot id can't work: the handle map, the runtime (status, activity, `started_at`,
`message`, `session`, `lane`), team turns, routine and follow-up state, memory's turn tracking, the restart mark,
`bots.session_id`, fallback keys and compaction windows are all single slots keyed by bot id
(`hub.rs`, `agent/bot/`, `chat/team/requests.rs`, `routines/`, `chat/memory/turns.rs`). A second session inside the
one actor doesn't work either: it drops every `session/update` that isn't the running turn's, routes permission
requests without checking the session, and tool calls carry only `botId` (`agent/bot/updates.rs`, `mcp.rs`).

So a task is a **task bot**: a `bots` row whose config has `taskOf = <bot id>` (id `task-<uuid>`; no schema
change, the config is JSON), copied from its bot when it starts. It gets its own actor, runtime, chat (its
transcript), session, agent process, restart mark, fallback state and compaction record, all by the existing code.
What it must not have of its own points back to its bot:

| Belongs to the task bot | Read from its bot |
| --- | --- |
| actor, queue, agent process, session | name, instructions, avatar (copied at start) |
| transcript (`entries.bot_id = task id`) | memory layers, their kv keys and the instruction snapshot (`BotConfig::as_owner()`) |
| runtime (status, activity, `started_at`) | approval setting (copied at start) |
| restart mark, fallback state, compaction | `search_history` (searches its bot's chat) |

A task bot never reaches a client as a bot: `Store::bots()` leaves task bots out, `emit_bot` for a task bot emits
its bot's event instead, and its entries leave as `taskEntry` events.

One process per task, always. Two sessions prompting in one process is not built: the actor's routing above makes it
unsafe, and a task's cancel must not kill the main session's agent.

## Fork or brief

A task launches when the turn that asked for it ends: a fork taken mid-turn would carry an unanswered tool call.
Its first turn opens its session one of two ways:

- **Fork** (`session/fork` of the main session, from the task's own process). Conditions: the agent advertises
  `sessionCapabilities.fork`; the bot is on its own agent (not a fallback); the main session's saved usage
  (`compaction.<bot>.<session>`) is under half its window. The request carries the main session's frozen
  instructions (`context::snapshot(main).system`, Claude `_meta` or ACP `systemPrompt`) and the same MCP server list,
  so the prompt cache is shared; then `context::adopt` and `memory::opened` with the parent's memory revision. This
  is the fork reply threads used ([threads](../plans/threads-for-teams.md#session-fork-as-it-was-implemented)) with a
  different caller.
- **Brief** (`session/new`). Used when a condition fails, or the fork errors or times out (logged, counted, never
  shown as a failure). Instructions are a fresh `context::render`; the first prompt carries what a fork would have
  known, built by the code that already does it: the rollover handoff (`handoff::build`: summary, last user messages,
  last reply, plan) sized by `carry::room`.

The task session gets the same MCP servers as the main one (a different tool list would break the fork's cache),
except `computer` and `manage`. The host knows a call comes from a task by its bot id and refuses what a task may not
do, the way a request turn is refused.

## The report

When a task's turn ends, its final reply goes to its bot as `Cmd::TaskReport` and becomes a host turn in the main
session (`HostTurn::TaskReport`, queued like a follow-up; reports arriving together are taken in one turn). That
turn's reply is an ordinary chat message with the usual *done* alert; `(pass)` keeps it silent. The report is kept
until its turn is over, so a restart loses none. Reports are not the user speaking (`from: "host"`), so memory and
dreaming don't take them as the user's words. Work started from a report's turn, with no user turn in between, is
counted (`hops`, as bot requests) and refused past the limit.

## Memory

- **Same memory.** A task reads and writes its bot's layers. Writes are already safe across sessions (the global
  `SCANNING` lock, compare-and-swap on content, one transaction per write). Its `memory` MCP server is started for
  its bot (`--bot <bot id>`), and the actor's memory and snapshot calls use `as_owner()`, so the memory module is
  unchanged.
- **Who wrote it.** Because the tools run as the bot, a task's `edit_memory` is credited to the bot's runtime: the
  chat's session. A task's edits through its agent's file tools aren't tracked as a turn's (the tracker keeps one
  turn per bot); the bot's next scan takes them as changes made outside a turn. Turn tracking per session fixes both
  ([still open](../plans/event-box.md#stages)).
- **Hearing each other.** Change notices are per session (`memory.announced.<bot>.<session>`): each session gets a
  `<memory_update>` at its next turn after the other wrote.
- **Dreaming** stays the bot's: a task bot spawns no dreamer, and a task's transcript is not read.

## Spikes

| # | Question | Result |
| --- | --- | --- |
| 1 | Which agents advertise `sessionCapabilities.fork`? | **Claude: yes** (`claude-agent-acp` 0.107.0, 2026-11-15). Others: the table below. |
| 2 | Does `session/fork` from a second process work on a session another process holds, right after its turn? | **Claude: yes**: the fork knew the chat, and the chat's session never saw what the fork did after. |
| 3 | Does the fork read the prompt cache (same instructions and tools)? | **Claude: yes** (41,453 of 42,425 input tokens read from cache on the fork's first turn). |
| 4 | With the instructions alone, does the agent hand long work on rather than do it in the chat's turn? | **Claude (haiku): not with the instructions alone** (two tries, 85 to 95 s of reading in the chat's turn); **yes with one line on each message** (`tasks::EACH_TURN`), and a quick question was still answered in the chat. |
| 5 | Can a subagent's start, end and result be read from the adapter's tool calls? | Not run. |

Where an agent fails 1 to 3, its tasks start from a brief: the feature is whole, a task just starts from a summary.

Spikes 1 to 3 for the other agents on the same computer (2026-11-15, each agent started the way the host starts it,
on its default login, with no MCP servers and no instructions; details on each agent's page in
[upstream agents](../upstream-agents/README.md)):

| Agent (version) | 1. Advertises `sessionCapabilities.fork` | 2. Fork from a second process | Original untouched | 3. Cache read on the fork's first turn |
| --- | --- | --- | --- | --- |
| Codex (codex-acp 2.6.0, CLI 0.167.0) | yes | yes, knew the chat | yes | partly: 7,168 of 25,728 input tokens (the chat's only turn had written none yet); 25,600 of 25,751 on its second |
| Hermes 0.22.5 | yes | yes, knew the chat | yes | yes: 18,048 of 18,231 |
| oh-my-pi 18.14.0 | yes | yes, knew the chat | yes | yes: 12,416 of 12,482 |
| OpenCode 1.19.5 (installed; registry 2.3.0 not tried) | yes | yes, knew the chat (replays the history first) | yes | not shown: 1,792 of 82,200, the same as the original's own next turn on this free model |
| Cursor (cursor-agent 2026.10.22) | no | `Method not found` | | |
| Pi (pi-acp 0.0.34, pi 1.4.0) | no | `Method not found` | | |
| GitHub Copilot 1.0.105 | no | `Method not found` | | |
| DeepSeek Harness (dsh 0.2.1-rc.5) | no | `Method not found` | | |
| Antigravity (antigravity-acp 1.4.3) | no | answers `{}` with no `sessionId` | | |
| Grok Build 1.0.77 | no | not run: not signed in | | |
| Gemini CLI 0.68.0 (registry build; not installed) | no (no `sessionCapabilities` at all) | not run: no login | | |

The four that pass advertise the field the host already reads (`SessionCaps.fork`, `host/src/agent/bot/launch.rs`),
so their tasks fork with no change. Not tried: a fork that also sends the instructions (`systemPrompt`, which Codex
advertises) and Codync's MCP servers, as the host's does.

## Left out then, and still

Tasks inside tasks; a task on another bot or computer; a model per task; a merge button or diff view for worktrees;
computer use in a task; typing into a task; searching task transcripts.
