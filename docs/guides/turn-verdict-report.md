# Topic 4: an honest end of turn

## What changed, by file

- `host/src/agent/bot/verdict.rs` (new): `TurnVerdict { Completed, Paused, Truncated, Cancelled, Failed }` and `of(stop_reason)`.
  The stop reasons that reach the host (`host/src/agent/acp.rs`, `agent/bot/turn.rs`) are ACP's `end_turn`,
  `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`, plus ours: `error`, `start_failed`, `host_restart`,
  `sign_in`, `verification_failed`. Mapping: `end_turn` is Completed; `max_turn_requests` is Paused (out of steps while
  still working); `max_tokens` is Truncated (answer cut); `cancelled` is Cancelled; everything else, including a refusal
  and any reason an adapter invents, is Failed. Reference: Mercury's `completion-verdict.ts` (steps exhausted is a pause).
- `host/src/agent/bot/turn.rs`: the verdict drives the chat notice, `turnResult.state` (`paused`, `truncated`, `done`,
  `stopped`, `failed`, `incomplete` as before for a refusal) and `turn_alert`.
- `host/src/remote/push.rs`: `AlertKind::Paused` ("Paused halfway", generic text "A task paused before it was done").
  It is additive: a client that does not know the category shows the generic line.
- `host/src/agent/bot/pause.rs` (new) and `api/bots.rs`: a turn that ends Paused or Truncated posts a notice with
  `pausedTurn {verdict, status, round, canContinue}`. `continueTurn {entryId}` sends the visible message "Continue where
  you left off. Finish what remains, then stop." (`from: "continue"`). A card works once, and at most two continuations
  follow each other (`MAX_CONTINUATIONS`); then the card says "Say what remains in a message". A scheduled check-in that
  stops short leaves no card.
- `host/src/agent/bot/verify.rs`: the check command runs only for a Completed verdict.
- Clients: desktop (`ChatRows.tsx` card, `bot-store.ts` `continueTurn`, `models.ts`), Android (`BotNoticeCards.kt`
  `PausedTurnCard`, `ChatRows.kt`, `AppStoreChat.kt`), terminal (`/continue` in the slash menu while a card can be used,
  and a line on the card).
- Tests: `verdict` (every reason, only a pause or a cut answer can be picked up again, the old state words),
  `pause` (once per card, limit, texts), `turn_alert` (Paused instead of Failed, a queued message does not hide it, a
  refusal still Failed), the terminal's `/continue`, and `host/tests/pause_e2e.rs` with `fixtures/pause_agent.py`: a real
  host and a scripted agent that runs out of steps get a paused result and card, one tap continues, a second is refused.

## Decisions where there were two options

- **Where the pause is shown.** A new notice type (`pausedTurn`) beside the existing takeover and passkey cards, not a
  new entry kind, so older clients show it as a plain notice with the explanatory text.
- **"Bounded" continuation.** ACP has no per-turn step limit to pass, so the bound is the number of continuations in a
  row (two) and one use per card.
- **A queued message does not hide the pause alert**, unlike a normal "done": the person has to know the work stopped.
- **Routines and team requests** still treat any non-`end_turn` ending as not finished, as before.

## Out of scope by the user's instruction

iOS, iPhone and Mac were not touched for this topic. (Earlier topics still carry some iOS code, written but never
compiled; see `progress-branch.md`.)

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test -j 1 --bin codync-host -- --test-threads=1` and `--test pause_e2e` | 835 passed, 0 failed, 1 ignored; the e2e passes |
| Desktop `npm run typecheck`, `npm test` | clean; 385 passed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, BUILD SUCCESSFUL in 9m 51s) |

## Not tested

No real agent was run to a step limit. The Paused push was not sent for real. The Android card was compiled, not seen.
