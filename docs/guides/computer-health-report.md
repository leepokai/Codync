# Topic 11: computer health

Reference: Mercury Agent's memory governor (MIT, `THIRD-PARTY-NOTICES.md`): the idea of levels and of freeing memory under
pressure; the policy and the code are Codync's. Feature doc: [computer health](../features/computer-health.md).

## What changed

- `host/Cargo.toml`: `sysinfo` 0.38 with only `system` and `disk`.
- `host/src/health.rs` (new): `Source` (the real `System`, or simulated readings in tests), `Thresholds`/`Level`, `Monitor`
  (last snapshot, thresholds saved in the key-value store, agent pids), `Track` (abnormal growth), the sampling task, the
  chat notices and the `hostHealth` value.
- `agent/bot/turn.rs`: `start_turn` asks `health.admit()` first; `queue.rs` words the refusal alone (not "Couldn't start the
  agent").
- `agent/bot/lifecycle.rs`: the idle release takes how long counts as idle; `Cmd::FreeMemory` asks for zero. `session.rs`
  registers each agent's pid when it starts.
- API: `hostHealth`, `setHealthThresholds` (a pair that makes no sense is refused and changes nothing).
- Clients: desktop `shared/health.ts`, `store/health` fields on the bot store, `HealthSection.tsx`, the island line;
  Android `core/Health.kt`, `ComputerHealthRows.kt`; terminal `tui/app/health.rs` and the header.

## Acceptance

"With memory simulated at 90 % no new turn starts and it says why":
`health::tests::at_ninety_percent_a_message_starts_no_turn_and_the_chat_says_why` runs a real bot with a scripted agent
and a simulated computer. At 90 % the message gets the notice "Not started… 90 % … 85 %" and no agent reply; `hostHealth`
says `critical` and `refusing`; at 40 % the next message is answered.

## Tests

Levels and threshold validation, the 90 % refusal and its recovery, a level announced once, the wording, growth (steady,
doubled, persisting, big but steady, small but fast, recovering), forgetting a gone agent, thresholds saved/refused,
the idle plan under pressure; desktop `shared/health.test.ts`; Android `HealthTest`; terminal `app::health`.

## Not done, and not tested

- The expensive-turn question (see the feature doc).
- Stopping a runaway agent was tested with simulated readings only; the real sampler (`sysinfo`) ran only as far as
  compiling and the one `hostHealth` call in the 90 % test, which uses the simulated source. No real memory pressure was
  produced.
- The island line and the Android and terminal rows were not seen.
- iOS: not done.
