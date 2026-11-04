# Routine flows, host: report

Branch `feat/routine-flows-host` (from `integrate/upstream-2.29`, v2.29.0 merged). Design: [routine flows](../features/routine-flows.md).
No version bump, no tag, no PR upstream. `minApp`/`minHost` unchanged (only methods, an event type and tools were added).

## Decisions and differences from Openstark

No Openstark file was copied; the design and rules were ported, so no notices were carried over.

| Openstark | Codync | Why |
|---|---|---|
| Sweeps over deliveries and turns, driven by agent events | The host runs the flow as one task per routine run, level by level | The host owns the run's end (`Routines::complete`); no polling |
| Agent message queue | The `ask_bot` road, without a sender turn (`chat/team/flow.rs`) | `message_bot`/`ask_bot` need a sender in a turn; a flow starts after the routine's run has ended |
| Tables with foreign keys | Explicit cascade in one transaction (`flow_forget_routine`, `flow_forget_bot`) | Routines live in the `routines.v1` JSON blob, so no FK is possible |
| Canvas change event with data | `{"type":"routineFlow","rev","botIds"}` only; clients refetch | A client that doesn't know new fields can't break; catch-up needs no payload |
| Node keys `agent:<id>` | `bot:<id>` | Codync vocabulary |
| Times as ISO strings | Unix milliseconds | Codync convention |
| A step "ran from the routine's own run delivery" | Root step recorded from the run's reply | Same result, no delivery to track |
| `testRoutineFlow` n/a | Starts the flow with a sample (or given) reply, without running the routine | Try a link without spending a run |
| Depth configurable | Env `CODYNC_ROUTINE_FLOW_MAX_DEPTH` (1–16, default 5) | The host has no generic settings API |

Other choices: a join runs with the inputs that replied (skipped only if none did); a `(pass)` reply hands nothing on; a routine run replying `(pass)` starts no flow; disconnecting a link doesn't delete links below it; the owner bot's *Stop* cancels its running flows; only the newest 50 runs per routine keep steps; a failed routine attempt that is retried leaves skipped steps for the failed attempt.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| Unit tests `routines::flow` (23: graph, engine success / middle failure / cancel / recovery / idempotency / loop / depth, API validation, permissions, cascade, events, tools) | 23 passed |
| `tui::manage::flow` tests | included in the unit binary run (824 passed, the 1 failure was fixed and re-run) |
| `routine_flows_e2e` (real hosts, restart mid-run) | 4 / 4 |
| `team_e2e` | 5 / 5 |
| `routines_e2e` | 13 / 14, the failure (fixture expecting exactly six tools) fixed; that test re-run alone: passes |
| `routine_retry_e2e` | 4 / 7 under load; two of the three pass alone |
| Dev host, port 19223, own data (`tests/routine-flows-dev-e2e.mjs`) | OK: A→B→C succeeded; failure in B left C skipped; a restart mid-run failed B ("The host restarted while this step was running…") and skipped C |

## Not verified

- `a_session_cut_short_by_the_time_limit_is_not_left_open` (`routine_retry_e2e`) fails here reading a bot log file that doesn't exist; it doesn't touch flows, but it was **not compared against the base branch**.
- A single full `cargo test -j 1 -- --test-threads=1` pass did not complete (it was cut off); the pieces above were run separately. Other e2e binaries were not re-run.
- No real provider was used: all agents are the deterministic fixture. Windows only; Linux and macOS not run.
- TUI: logic and row building tested; the drawing was not driven in a terminal.
- Codync Dev desktop/phone apps: not part of this prompt.

## Contract for prompt 2

Times are Unix ms. Ids 1–200 chars. Errors: HTTP 400 `{"error":"…"}`; 403 for a device without the `control` scope or for pairing; 404 on a host without the method (older host: hide the canvas).

| Method | Request | Response |
|---|---|---|
| `routineFlowCanvas` | `{botId}` | Canvas |
| `saveRoutineFlowPosition` | `{botId, nodeKey, x, y}` (`routine:<id>`/`bot:<id>` that exists; \|x\|,\|y\| ≤ 1 000 000) | `{ok:true}` |
| `removeRoutineFlowPosition` | `{botId, nodeKey}` (a `bot:` key also unplaces) | `{ok:true}` |
| `placeRoutineFlowBot` | `{botId, placedBotId, x, y}` | `{ok:true}` |
| `connectRoutineFlow` | `{routineId, fromBotId, toBotId, instruction?}` (≤ 8000 bytes) | `{link}` |
| `disconnectRoutineFlow` | `{linkId}` | `{ok:true}` |
| `updateRoutineFlowLink` | `{linkId, instruction}` | `{link}` |
| `testRoutineFlow` | `{botId (owner), routineId, input?}` | `{runId}` |

```
Link   {id, routineId, fromBotId, toBotId, instruction, createdAt}
Step   {id, runId, botId, deliveryId|null, input, output|null, status, error|null, createdAt, updatedAt}
       status: running | succeeded | failed | skipped | cancelled
Pos    {nodeKey, x, y}
Canvas {botId, rev, routines:[{routine, recentRuns:[Run], upcomingRuns:[ms], steps:[Step]}],
        links:[Link], positions:[Pos], placedBotIds:[botId]}
        routine = the `routines` method's routine object (id, botId, name, instruction, triggers, enabled,
        timeoutSeconds, lastError, triggerDescriptions, nextRunAt, createdAt, updatedAt)
Event  {type:"routineFlow", rev, botIds:[…]}   // refetch routineFlowCanvas for each bot; never skip it
```

Connect refusals (text in `error`): same bot, into the routine's bot, not reached yet, duplicate, loop, too deep, group chat,
terminal session, unknown or hidden bot. A canvas holds routines the bot owns plus those with a link naming it.
Bot tools (MCP `routines`): `link_routine_bots`, `unlink_routine_bots`, `list_routine_flow`.
