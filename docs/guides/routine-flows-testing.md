# Testing routine flows

How to check [routine flows](../features/routine-flows.md) on a development host. Use the `dev` host (port 19223,
its own data directory) and never the production host on 19222.

## Automated

From `host/` (this machine: `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1`, `-j 1`):

```sh
cargo test -j 1 --bin codync-host routines::flow -- --test-threads=1   # graph, engine, API, recovery
cargo test -j 1 --bin codync-host tui::manage::flow -- --test-threads=1
cargo test -j 1 --test routine_flows_e2e -- --test-threads=1            # real host process, restart mid-run
```

The unit tests drive a host with the deterministic `tests/fixtures/team_agent.py` agent: `FAIL` in a link's
instruction makes that bot fail, `BLOCK` holds it until a `release` file exists in its folder.

## By hand (API)

```sh
export CODYNC_HOME=/tmp/flow-dev CODYNC_PORT=19223       # a data directory of its own
codync-host serve &
TOKEN=$(cat $CODYNC_HOME/token)
call() { curl -s -H "Authorization: Bearer $TOKEN" -d "$2" http://127.0.0.1:19223/api/$1; }
```

1. Create three bots (`createBot`) A, B and C and a routine on A (`saveRoutine`).
2. `call connectRoutineFlow '{"routineId":"R","fromBotId":"A","toBotId":"B","instruction":"Summarize"}'`, then B → C.
3. `call testRoutineFlow '{"botId":"A","routineId":"R","input":"hello"}'` and read the steps from
   `routineFlowCanvas` (`routines[0].steps`). Every bot shows `succeeded`, and B and C chats show a "Routine flow" notice.
4. Make B fail (an instruction that makes its agent fail) and repeat: B is `failed`, C is `skipped`.
5. Restart the host while a step runs: after startup the step is `failed` ("The host restarted…") and the rest `skipped`.
6. `curl -N -H "Authorization: Bearer $TOKEN" 'http://127.0.0.1:19223/events?since=0'` shows `routineFlow` events.

In the terminal UI (`codync-host tui`), `R` opens a bot's routines and `f` the flow of the selected routine:
`a` connects a bot (then type the instruction, `↵`), `e` edits the selected link's instruction, `d` disconnects it,
`t` runs a test.
