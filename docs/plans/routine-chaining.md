# Routine chaining: one routine runs after another

Status: stage 1 (after a routine finishes) built on `feat/routine-chaining` (2026-10-29). Current behavior:
[routines](../features/routines.md#after-another-routine-routineschainrs). Stage 2 (after a bot's reply) is open,
see the end.

## Why

A routine starts on a clock or on a webhook; nothing starts it when another routine is done. Two chains in real use
are held together from outside today:

| Chain | Today | Problem |
|---|---|---|
| Server check → repair assessment | System crontab: the check at 0/6/12/18, the repair 10 minutes later, passing the result through a state file | Fixed offsets: a slow check hands the repair a stale result; neither run shows in Codync |
| Post draft → review by another bot → publish | The draft routine asks the reviewer with message_bot; the lead bot picks up the reply by hand | Depends on the lead bot being there (stage 2) |

Goals:

1. A routine can run after another one succeeds or fails, optionally only when that run's result contains some
   words, or only when it had something to report.
2. The finished run's result is the next run's trigger data.
3. No client changes: the apps show and keep such a trigger as they are.
4. No endless loops and no flood of runs.

Not in scope: general workflows (branches, joins, a graph editor) and chains across computers.

## What already exists

- `Trigger::Event { source, event, filters }` (`routines/schedule.rs`) with `matches(payload)`: `source` and `event`
  (`*` for any) must match, each filter must equal the payload's field, and `messageContains` matches a substring of
  `text`. Only webhooks deliver events today.
- `Routines::enqueue` / `receive` (`routines/mod.rs`) queue a run with its event; `busy()` refuses a second one.
- `Routines::complete` (`routines/runner.rs`) records the outcome and reply inside one `change()`; a failure that
  `retry::after_failure` schedules again gets `retry_at`.
- Every client decodes `source/event/filters`, keeps a trigger it can't edit as it is, and shows the host's
  `triggerDescriptions` (iOS `RoutineTrigger` and `RoutineEditorView`, Android `Routine.kt`, desktop `routine.ts`).

So the plan reuses the event trigger with a reserved source, `routine`, and has the host raise the event itself when a
run ends.

## Design

### The trigger

```json
{"type": "event", "source": "routine", "event": "succeeded",
 "filters": {"routineId": "<upstream id>", "messageContains": "CRIT", "passed": "false"}}
```

- `event`: `succeeded`, `failed` (final failures only) or `*`.
- `filters.routineId` is required: no routine listens to every other.
- `filters.messageContains` (optional) matches the upstream's result text.
- `filters.passed` (optional): `"true"` when the upstream had nothing to report (`(pass)` or no reply), `"false"`
  otherwise. `"false"` is the usual "only when the check found something".
- Other filter keys are refused, so a typo can't make a trigger that silently never fires.

### The event

```json
{"source": "routine", "event": "succeeded", "routineId": "…", "routineName": "Server check", "botId": "…",
 "runId": "…", "passed": "false", "text": "the reply, up to 16 KB", "finishedAt": 1793188800000, "depth": 1}
```

Filtered fields are strings, as `matches` compares strings. The run prompt already marks trigger data as untrusted.

### When it fires

In `complete`, in the same `change()`, after the outcome is recorded and before the run drops its event:

1. `Succeeded` → `succeeded`; `Failed`/`Interrupted` with no `retry_at` → `failed`; otherwise (a retry is coming, or the
   run was cancelled) nothing.
2. Every enabled, undeleted routine other than the upstream whose trigger matches gets a new pending run with the
   event. Runs are pushed onto the state directly: calling `enqueue` would take the state lock again.

The code lives in `routines/chain.rs`; `complete` gains one call.

### Loops and load

| Rule | How |
|---|---|
| Not after itself | Saving refuses `routineId` equal to the routine's own id |
| No loop | Saving walks the upstreams' own upstreams and refuses a loop back to the routine |
| Depth | The event carries `depth` (upstream's depth + 1 when it was itself chained, else 1); past 5 nothing is queued |
| Busy | A routine already running or queued is skipped, not queued twice |
| Unknown upstream | Saving refuses it; one deleted later just never fires and reads "a deleted routine" |

Skips are written to the downstream routine's `lastError`.

### Across bots

A routine can run after any bot's routine on the same host: all of them are the same user's. The run belongs to the
downstream bot and reports in its chat.

### Webhooks

`hook_set` registers a cloud webhook for every routine with an event trigger. `routine` events never leave the host,
so only `Webhook` and events of other sources count (`Trigger::external`).

### Descriptions

`After "Server check" succeeds` / `fails` / `finishes`, plus ` with something to report` / ` with nothing to report`
for `passed`, plus ` · result contains "CRIT"`. `public()` names the upstream from the state; `Trigger::description`
alone says "another routine".

### Tools

`save_routine`'s description documents the trigger, so a bot can set up a chain in conversation.

## Clients

None must change: each decodes the trigger, keeps it through an edit of the name or instruction, and shows its
description. A later option: an "After another routine" choice in the editors.

## Tests

- Unit (`routines/tests.rs`): the result reaches the next run; outcome, `passed` and `messageContains` filters; only
  final failures, never cancelled runs; busy, paused and depth skips; unknown upstream and loop refusals; no webhook;
  the description.
- End to end (`tests/routine_chain_e2e.rs`): a run on one bot starts the routine after it on another with the trigger
  data; saving refuses an unknown upstream and a loop.

## Toward workflows (later)

Chaining gives workflows their edges: sequence, conditions (filters), fan-out and data passing, across bots. What a
workflow still needs, in the order it would come:

1. **One run of the whole chain**: a `workflowRunId`, set by the first run and carried in each event, so the apps can
   show how far one pass got and where it stopped. Small: one more event field and a grouped run listing.
2. **Joins**: "after A and B both finish", a trigger that waits for all its upstreams within the same workflow run.
   The largest piece: it needs durable waiting state.
3. **A workflow as one object**: a definition (nodes and edges) to pause, edit and delete as a whole, instead of
   links spread over routines; the routines become its nodes.
4. **A view of it**: the graph with each node's state, in every client.

Step 1 follows once this has run in real use; steps 2–4 wait for a chain that needs a join, so the framework is shaped
by real cases.

## Stage 2: after a bot's reply (open)

For "publish once the reviewer says approved": the chat would raise `{source:"chat", event:"botMessage", botId,
chatId, replyTo, text}` when a bot's message lands, filtered by `botId` and `messageContains`. Open questions: tying a
reply to the request it answers (message_bot's request id would have to reach the reply), whether group messages
count, and consent, since chat text would reach another bot's prompt.
