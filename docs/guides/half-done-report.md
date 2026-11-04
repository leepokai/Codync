# Topic 2: closing what was left half done

Branch `deska/Implementar-Motor-de-Flujos-de-Rutina-5fa5448a`, on top of the base update (`base-update-report.md`).

## What changed, by part

### A. Approvals
- `host/src/chat/takeover.rs` (new): the card of `request_handoff` now carries `requestedAt`, `expiresAt` (30 min) and
  `notified`; the host pushes `NeedsInput` once as it is posted; a 60 s sweep (`background.rs`) turns a waiting request
  into `expired` and rewrites the chat line. `chat/open_items.rs` builds the card through it.
- `host/src/agent/bot/approval_age.rs`: the 4 h reminder writes `reminded: true` into the card, so it is sent once per
  episode even across restarts. Timers use `Duration::from_hours`.
- Clients: desktop (`ChatRows.tsx`, `bdots-models.ts`) and Android (`BotNoticeCards.kt`, `BdotsModels.kt`) show an expired
  request closed and drop it from the pending decisions; the terminal shows the rewritten line; iOS has no takeover card.
- Tests: injected clock (`lapsed(data, now)`, `Waiting::due(now)`), the sweep against a real store, the flag, and the
  desktop and Android pending-decision tests with an `expired` card.

### B. `verify_command`
- Exit code decides (`verify.rs`); a 5 minute limit (`run_within` makes it testable), output cut to its last 1,200
  characters; the check runs only for a turn that ended `end_turn`.
- Only the user writes it: a test in `chat/manage/tests.rs` fails if a manage tool or field can set `verifyCommand`,
  `backgroundModel`, `personality`, `approvalRules` or `actionRules`.
- Field added to the terminal (`app/forms.rs`, `ui.rs`, `view/forms.rs`) and iOS (not compiled).

### C. Background model and saver
- **Finding:** a session's model is fixed when the session starts (`finish_session_model` is the only place that sets
  it), and scheduled check-ins ran in the main chat session, so they could never use another model. Routine runs
  already have a session of their own. Now a check-in on a bot with a background model gets its own `check-in` session
  (`session.rs`, `slot()`), and `finish_session_model` starts routine and check-in sessions on the background model.
- Selector instead of free text: desktop and Android list the agent's models (`agentModels`); iOS likewise (not
  compiled); the terminal cycles with the arrow keys (`cycle_model_for`). An unlisted value stays selectable.
- Saver: `BotConfig.saver` plus `Budgets::saving` (the bot's or the fleet's budget at the warning level today). A bot with
  the saver on and a background model starts new sessions on it until the next day; the 80 % warning says so. The switch
  is in all four clients and is shown only against a host that sends `saver`.
- **Not done, on purpose:** switching the model of a session that is already running, and "less context". Both would change
  what a live session is told and break the prompt-cache and snapshot invariants of `chat/context.rs`. Recorded as a
  limit in `docs/features/bot-character.md`.

### D. Weekly recap
- `host/src/chat/recap.rs`: Monday 09:00 to 12:00 by the computer's clock, once per ISO week, a push with the counts of
  `Store::week`; the snapshot is kept in `recap:<week>` before sending (a crash loses a recap, never repeats one).
- iOS card in Usage (not compiled), a "Your week" line on the terminal's usage sheet; desktop and Android had the card.

### E. Soul, persona, heartbeat
- Fields added to the terminal (`character` editors) and iOS (`Personality.swift`, `BotEditorView.swift`, not compiled).

### Docs
- `docs/features/approvals.md` (unanswered cards, takeover expiry), `docs/features/bot-character.md` (new).

## Decisions where there were two options

- Takeover lives in `chat/open_items.rs`, not `chat/handoff.rs` (that file is the conversation handoff). The request does not
  hold the bot's computer in this build, so expiry only changes the card.
- Personality stays in config fields only the user edits, not in memory files the bot can write.
- The saver is opt-in (default off) and applies to new sessions: the least surprising behaviour that needs no mid-session
  model switch.
- `saver` is always serialized so clients can tell a host that has it from one that does not.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | passes |
| `cargo clippy --all-targets -- -D warnings` | passes |
| `cargo test -j 1 --bin codync-host -- --test-threads=1` | 822 passed, 0 failed, 1 ignored |
| Desktop `npm run typecheck`, `npm test` | clean; 385 passed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, 7m 30s) |

## Not tested

iOS and its Swift tests were written, never compiled. No agent ran against these changes: expiry, reminders, the
check-in session and the saver were tested with injected clocks and unit tests only. The Monday push was not sent for
real. The `e2e.rs` fake-agent harness was not extended: the 4 h, 24 h and 30 min thresholds are not reachable in it
without a configurable clock.
