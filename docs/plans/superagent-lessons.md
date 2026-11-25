# Learning from DeepSeek's Super-agent

Status: building (2026-11-21), branch `feat/superagent-lessons`. Sources: DeepSeek Harness, "Exploring Super-agent
Design" (2026-10-31; English translation at offloop.org/learn/exploring-deepseek-super-agent-design) and its research
prototype `deepseek-ai/dsh-superagent-prototype` (`docs/003-execution.md` for worker slots). Comparison page:
https://claude.ai/artifact/987JgRa2UKyn26UoVrBCwz

Codync already has the two layers ([work](../features/work.md)): the bot's chat is the Router, its threads are the
Workers, reports arriving together are one turn (Router batching), `add_task` answers at once (asynchronous calls),
a restart resumes with a do-not-repeat note, and rate limits rest on fallbacks. Five lessons are taken over:

| # | Lesson | Super-agent | Codync before | Codync now |
|---|---|---|---|---|
| 1 | Watchdog | An external control loop judges whether Workers make progress or drift | An hourly look at whether a thread's process is still there | Three layers, below |
| 2 | The Router doesn't do work | The Router has routing tools only (no Bash); prompt-only delegation was slow and unstable (p95 29.5 vs 15.7 min) | One line each turn (`work::EACH_TURN`) asks the chat to hand work on | While the box holds work, the chat's own edits and commands are refused and it is told to hand them on |
| 3 | Direct delivery | Later events of a Thread go straight to the subscribed Worker | The chat could only replace a working task (its session closes) or queue more | `tell_task`: the chat passes words to a working thread, at its next step |
| 4 | A seat is given up while a person decides | A handoff to a person frees the worker slot; a person's message has one extra slot | A thread waiting on an approval held its seat | It gives its seat up; at most one thread past the ceiling |
| 5 | Chain of Recall | Objects carry the ids of related ones, read one hop at a time | Ended tasks were deleted after 7 days; the chat couldn't read a thread | `recall_task` reads a task, its related ids and its thread a page at a time; ended tasks are kept 30 days |

A sixth came from the same idea (the Router and the Workers needn't be one model): **Advisor mode**, below.

Not taken (yet): event triggers from outside sources (GitHub, email) with code-then-model filtering, acting in
advance, and moving the agent loop off the user's computer.

## 1. Watchdog

All three layers are built ([watchdog](../features/work.md#watchdog)).

- **Layer 0, the host itself** (`host/src/watchdog.rs`): an OS thread outside the async runtime expects a heartbeat
  that goes through the runtime and the hub's emit lock. One missing for 5 minutes (a deadlock, a starved runtime)
  logs, leaves a mark, and aborts the process; the service starts it again (launchd `KeepAlive`, systemd
  `Restart=always`, `codync-hostw` on Windows), and the next start reports it (`host_restarted_stuck`). Only under
  the service (`service::started_by_service`). A stall within 30 minutes of the last such restart only logs, so a hang
  that comes back at start doesn't restart the host every few minutes. A sleeping computer is not a hung host: the
  thread's wall-clock gap resets the wait.
- **Layer 1, code checks** (`work/watch.rs`): every 5 minutes while any thread of the bot works, a pure function looks
  at each working thread's current turn: the same tool call again and again (`looping`), tool calls that keep failing
  (`failing`), a turn with no new step for an hour (`quiet`), and a card left unanswered for two hours
  (`card`). Silence alone never acts: a quiet turn only asks the bot to look.
- **Layer 2, the bot looks** (only on a flag): `card` reminds the user once (a push, no agent). The others go
  to the bot's own chat as a host turn, with the task and what shows it stuck (the repeated call, the failed calls,
  the minutes without a step); the bot reads its steps with `recall_task` and decides with the tools it
  has (`tell_task` to nudge, `steer_task` to start again or stop) and answers `(pass)` when it is fine. Each thread
  is looked at by the bot at most once an hour.

## 2. The chat hands work on

While the bot's box holds a working or waiting task, a turn in its own chat runs its agent in a mode that asks before
editing or running commands (`modes::under`, as a network ceiling does), and Codync refuses those requests itself
(edit, delete, move, execute) with no card. Reading, searching and Codync's own tools work. The bot's instructions say
so. An agent without such a mode is not held. **Not built**: on real Claude and Codex the refusal reaches the user as a
question or a card instead of a task ([spike](#spike)).

## 3. `tell_task`

`tell_task {task, text, now?}` on the `chat` server. A waiting task takes the words as a note its thread reads when it
starts. A working one gets them in its thread, marked as passed on from the chat (`from: host`, `relay`): after its
current step's turn, or with `now` at its next step where the agent takes messages mid-turn (otherwise its current
turn stops for them). Like the user's, a passed-on message that waits keeps the task going until a turn takes it, so
the chat gets one report, the last turn's. Kept with the task (`relayed`), the words reach a session that starts it
again as the chat's, never as the user's. Refused for an ended task, a routine's run and another bot's request.

## 4. Seats

`dispatch::free_seats`: a working task whose thread waits on the user (its runtime `needsInput`) takes no seat, but every
live thread counts toward a hard bound of `maxThreads + 1`, so only one seat is ever lent. A thread that starts to
wait on the user looks the box over again (`Work::seat_freed`). Each task row carries `seat` on the wire (the one
waiting task that lent its seat says `false`); clients show `in use / maxThreads`, Free rows and Cut in from it
(older hosts: every live task takes one). Near the usage limit a waiting thread still counts as running. Where the
next thread gets no worktree (no repository, Worktree per thread off, the bot's own folder) it shares the waiting one's
folder, even at `maxThreads` 1, which the manual says ([working folders](../../web/content/docs/working-folders.mdx)).

## 5. `recall_task`

`recall_task {task?, query?, before?}`: with `task`, its brief, the user's words and notes, its full result, the ids
it relates to (`after`, what follows it, its thread) and a page of its thread's steps (`before`: an entry id for the
page before); with `query`, the tasks of the last 30 days whose title, brief or result has it. Read only, so allowed
under a network ceiling. Ended tasks stay 30 days (at most 200 a bot); a thread and its transcript still go after 7.
Built: at host start a task ended over a week ago leaves the box (which the wire and every change read whole) for kv
`work-past.<bot>`, which only `recall_task` reads; `work_recalled` counts its use.

## 6. Advisor mode

Bot settings → Agent → Advisor mode: the bot's threads run on an agent of their own, with their own fallbacks, while
the chat stays on the bot's agent (a strong model that plans and routes, a cheaper or faster one that does the work, or
another vendor's). `BotConfig.thread_agent` (none: off) and `thread_fallbacks`; `agent::fallback::threads` is the bot
as its threads run it (that agent, model and login, its fallbacks, Auto for a mode the chat's agent named). A thread
starts as a fork of the chat's session only on the chat's own agent and login (a model of its own is fine); on
another it starts from a brief that tells it the chat. A thread keeps the agent it started on: a change of the chain
reaches running threads whose first agent it is. Routine runs and other bots' requests are threads too, so they run
on the threads' agent. Usage, rests and login profiles count both chains. `bot_updated` says `advisorMode`.

## Spike

2026-11-21, real agents on an isolated host (claude-acp 0.109.0 on Sonnet 5.5, codex-acp on 6.2 Sol), a git repo with a
README, a task working in the box (checked right before the turn), then "Edit README.md: add a section …, then show
me git diff" in the chat, its edit and shell cards refused:

- **Claude, a fresh bot whose instructions say Codync refuses its edits and commands while the box has work, and to
  hand the request on with `add_task` when that happens.** It did the edit itself ("Small job; do it directly"),
  tried again with its Edit tool after the first refusal, then asked the user to allow writing. No `add_task`.
  Without that rule in its instructions it also asked the user; only a note steered in after the refusal got a task
  added, after a confused "refused" reply.
- **Codex.** In its `read-only` mode, the mode that would hold it, Codync's own tools (`add_task`, `send_message`) each
  raise a card; in `auto` they don't. A refused card ends its turn, and a message steered into that turn was lost:
  a Codex bug, now patched for every chat (a re-queue, [codex-acp](../upstream-agents/codex-acp.md)).
- **What the chat did is what `EACH_TURN` asks.** A two-call edit is "work of a few tool calls", which the chat may do
  itself; the failure that prompt was written for (2026-11-15: 90 seconds of reading in the chat's turn) is reading,
  which a lowered mode allows anyway.
- **Advisor mode, same agent and a lighter model** (Sonnet chat, Haiku threads): the task's thread forked from the
  chat's session (`start: fork`), switched to Haiku (it wrote `claude-haiku-5-5` when asked for its model id) and
  reported. Not tried: a threads' model with a smaller context window than the chat's.

Recommendation: don't hold the chat by mode. It blocks the small edits the user wants done now, not the long reading
it was meant for, and on both agents a refusal reaches the user as a question or a card. If the chat should still be
held, the hook that fits the measured failure is the watchdog seeing the chat's own long turn while its box has work.
