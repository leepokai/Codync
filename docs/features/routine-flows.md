# Routine flows

A routine can pass its result on. When the routine's bot answers, a **link** hands that answer to
another bot, with an instruction of its own ("summarize this", "translate it"), and that bot's
answer can go on to a third. A flow lives *inside one routine*: the routine's own bot starts it,
and each bot that takes part in a run leaves a **step**. The host runs it (`host/src/routines/flow/`);
the clients only show it and send intents. The design is ported from Openstark's routine flows
(`routine-flows-v1`) with Codync's vocabulary (bot, not agent); no code was copied.

Flows are built by talking to the bot ("when the digest is ready, give it to Ada to translate"),
from the graphical canvas of the desktop and phone apps, or in the terminal UI (`f` on a routine).

## Model

| Thing | Fields |
|---|---|
| **Link** | `id`, `routineId`, `fromBotId`, `toBotId`, `instruction` (≤ 8000 bytes, may be empty), `createdAt` (ms) |
| **Step** | `id`, `runId`, `botId`, `deliveryId` (the bot request that carried the input; `null` for the routine's own bot), `input`, `output` (`null` until it replies), `status`, `error`, `createdAt`, `updatedAt` |
| **Step status** | `running`, `succeeded`, `failed`, `skipped`, `cancelled` |
| **Position** | `nodeKey` (`routine:<id>` or `bot:<id>`), `x`, `y` (numbers, at most ±1 000 000) |

A **canvas** belongs to a bot and holds every routine it owns or that passes through it, the links of
those routines, where the nodes sit, and the bots **placed** on it before any link reaches them.

## Running a flow

1. The routine's run ends (`Routines::complete`). A reply other than `(pass)` becomes the root step
   (`output` = the reply, `botId` = the routine's bot). A failed, interrupted or cancelled run records
   the root step that way and **skips** every bot the flow reaches.
2. Level by level (depth 1, 2, …) each bot of the flow gets a request through the **same road as
   `ask_bot`** (`chat/team/flow.rs`): the bot's actor queue, an exchange notice in its chat ("Routine flow ·
   name · from Ada"), the assignment ledger and the reply. The prompt carries the replies of the bots linked to
   it and the links' instructions. A flow needs no turn of the sending bot (the host sends it).
3. A bot with several inputs (a join) waits for all of them and gets them in one request. If some inputs
   failed, it runs with the ones that replied; if none replied, its step is `skipped`. A bot that replies
   `(pass)` succeeds with no output, so what hangs below it is skipped.
4. Each step has the **routine's run timeout**. A step that fails leaves its bot's step `failed` with the
   error, and everything that only depended on it is `skipped`.

Rules the host enforces:

- **No cycles**: connecting refuses a link that would close a loop, and running checks again (a loop in
  stored links skips the whole flow and logs an error).
- **Depth**: at most 5 hops from the routine's own bot; set `CODYNC_ROUTINE_FLOW_MAX_DEPTH` (1 to 16) in the
  host's environment to change it. Connecting counts what already hangs below the new link; running skips bots
  deeper than the limit.
- **Targets**: only a bot that takes requests like any other: not a group, not a terminal session
  (`external::is_external`), not hidden or deleted. The same check applies when connecting, when placing a bot and when running.
- **Connecting rules** (`graph.rs`): not to itself, never into the routine's own bot, only from a bot the flow
  already reaches, no duplicate link between the same two bots, no loop, no deeper than the limit.
- **A link applies to runs that began after it was made.**
- **Idempotent**: a bot has at most one step per run (`UNIQUE(run_id, bot_id)`), so running the same run again
  adds no steps and sends nothing twice. Only the newest 50 runs of a routine keep their steps.
- **Cancelling**: deleting the routine, deleting the bot, disconnecting a link or the owner bot's *Stop* cancels the
  routine's running flows. The step in flight ends `cancelled` and the rest are skipped.
- **Restart**: steps still `running` when the host starts fail with "The host restarted while this step was running",
  and the bots of a cut-short flow that never got a step are `skipped` (`recover.rs`). The flow is not resumed.
- **Disconnecting** a link does not remove links that hang below it; they stay but are not reached until the
  flow reaches them again.

## Host API

All methods use the usual authenticated API (`POST /api/<method>`) and the E2E channel with a device that has the
`control` scope: the same access as `routines`. Changes are `rev`-stamped and reach every client through events.
Request bodies are JSON objects; ids are 1 to 200 characters; unknown or invalid input answers `400` with
`{"error": "..."}`.

| Method | Body | Answer |
|---|---|---|
| `routineFlowCanvas` | `botId` | Canvas (below) |
| `saveRoutineFlowPosition` | `botId`, `nodeKey`, `x`, `y` (node must exist) | `{"ok": true}` |
| `removeRoutineFlowPosition` | `botId`, `nodeKey` (a `bot:` key also unplaces the bot) | `{"ok": true}` |
| `placeRoutineFlowBot` | `botId`, `placedBotId`, `x`, `y` | `{"ok": true}` |
| `connectRoutineFlow` | `routineId`, `fromBotId`, `toBotId`, optional `instruction` | `{"link": Link}` |
| `disconnectRoutineFlow` | `linkId` | `{"ok": true}` |
| `updateRoutineFlowLink` | `linkId`, `instruction` | `{"link": Link}` |
| `testRoutineFlow` | `botId` (the routine's owner), `routineId`, optional `input` | `{"runId": "flow-test-…"}` |

`testRoutineFlow` doesn't run the routine: it starts the flow as if the routine's bot had just replied with `input`
(a sample sentence by default), so a link can be tried without spending a run. Its steps carry the returned
`runId`. Run the routine itself with `runRoutine`.

**Canvas** (`routineFlowCanvas`):

```json
{
  "botId": "…", "rev": 123,
  "routines": [{
    "routine": { "id": "…", "botId": "…", "name": "…", "instruction": "…", "triggers": [], "enabled": true,
                 "timeoutSeconds": 3600, "lastError": null, "triggerDescriptions": [], "nextRunAt": 1730000000000,
                 "createdAt": 0, "updatedAt": 0 },
    "recentRuns": [ /* the newest 10 routine runs, newest first, as in `routines` */ ],
    "upcomingRuns": [1730000000000],
    "steps": [ /* the steps of the routine's newest run that has any, oldest first */ ]
  }],
  "links": [ { "id": "…", "routineId": "…", "fromBotId": "…", "toBotId": "…", "instruction": "", "createdAt": 0 } ],
  "positions": [ { "nodeKey": "routine:…", "x": 0.0, "y": 0.0 } ],
  "placedBotIds": ["…"]
}
```

`upcomingRuns` are the next fire times in the next 7 days (at most 400; empty while paused or for a webhook or
watch routine). All times are Unix milliseconds.

**Event**: `{"type": "routineFlow", "rev": N, "botIds": ["…"]}` — the canvases of these bots changed (a link, a
step, a position, a placement); fetch `routineFlowCanvas` again. The event carries no data on purpose: a client
that doesn't know it can't be broken by a new field, and one that does always reloads. Catch-up (`/events?since=`)
holds one such event for every bot whose canvas changed after `since`, in rev order. Clients must never skip it.

## Bot tools

The built-in `routines` MCP server has three more tools, scoped to the bot's own routines:
`link_routine_bots` (`routineId`, `toBotId`, optional `fromBotId` and `instruction`), `unlink_routine_bots` (`linkId`,
or `routineId` + `toBotId`) and `list_routine_flow` (optional `routineId`). The server's instructions tell the bot
when to use them (see [Routines](routines.md#conversational-setup-and-tools)).

## Storage

SQLite tables `routine_flow_links`, `routine_flow_steps`, `routine_flow_positions` and `routine_flow_placed`
(`host/src/store/flows.rs`, `flow_steps.rs`), with indexes by routine, owner bot, target bot and step status.
Routines themselves live in the `routines.v1` JSON blob, so the cascade is explicit: deleting a routine
(`flow::forget_routine`) or a bot (`flow::forget_bot`, also at startup for deleted bots) removes its links,
steps, positions and placements in one transaction. Each change stamps a global `rev` and records it per bot in
`kv routineflow.rev.<botId>`.

## Compatibility

Only methods, an event type and fields were added: `minApp` and `minHost` do not change, and older apps and hosts
ignore the new method names and `routineFlow` events (the TUI and apps skip unknown event types by design).

## Screens

In each client, a bot's routines screen has a **List | Flow** selector. Flow shows the canvas of that bot: routine and bot nodes, a fixed dashed edge from each routine to its own bot, link edges, saved positions (nodes never moved sit in automatic columns by depth), and each bot's latest step status (icon and words, never colour alone). A **list** of the same flow is the keyboard and screen-reader equivalent (and the phone's default with TalkBack on or on narrow screens). Both open the same inspector: a routine (pause/resume, test, latest steps, run history), a bot (hand its reply on to…, remove if placed by hand) or a link (instruction, disconnect). States: loading, no routines, not connected, no runs yet, error with retry, offline (read-only) and a host without the methods (the canvas says to update Codync there).

- **Desktop** (`apps/desktop/src/renderer/views/routines/flow/`, model `lib/routine-flow.ts`, state `store/routine-flow-store.ts`): hand-written pan/zoom/drag; connect by dragging from a bot's dot onto another bot, or from the inspector.
- **Android** (`RoutineFlow*.kt`, model `core/RoutineFlow.kt`): pan, pinch and node dragging; connect from the bot's detail sheet.
- **Terminal** (`f` on a routine): the chain as rows; same words (Running, Succeeded, Failed, Skipped, Cancelled).
- **iPhone**: not built yet.

The clients pre-check the connect rules (`connectProblem`, same wording as the host) and the host stays the authority.

## Trying it

See [Testing routine flows](../guides/routine-flows-testing.md).
