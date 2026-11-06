# Base update: this branch on `origin/integrate/upstream-2.29`

The branch started at `eded5f5f` (9 commits of its own) and was 262 commits behind
`origin/integrate/upstream-2.29` (Codync 2.29.0). `git merge --no-ff origin/integrate/upstream-2.29`, no squash, no rebase,
`rerere` on for this worktree only. Eight files conflicted; every other file merged by itself.

## Conflicts, by layer

| File | What collided | Criterion |
|---|---|---|
| `host/src/hub.rs` | My bot validation (personality, check command, background model) against upstream moving `validate` out of `hub.rs` | Took upstream's layout and re-added my three checks in `host/src/hub/validate.rs` |
| `host/src/store/model.rs` | My `verify_command`, `background_model`, `personality` against upstream's `action_rules`, `proactive` in `BotConfig` | Kept both sets of fields |
| `host/src/agent/bot/mod.rs` | `mod approval_age;` / `mod verify;` against upstream's new `mod cmd;` and its split of the actor | Kept all modules; my hooks (`age_approvals`, `verified`, `waiting`) merged by themselves and were checked with `git grep` |
| `apps/android/core/.../BotDraft.kt` | My three draft fields against upstream's `offeredToNetwork`, `reachableByAssistants` (and its `UNSET`/template lists) | Kept both; `from()` passes arguments in the declaration order; my keys joined `UNSET` |
| `apps/android/core/.../HostClient.kt` | Same fields on `Bot` | Kept both |
| `apps/android/app/.../BotEditor.kt` | Upstream moved the whole settings form into `BotSettingsForm.kt` | Took upstream's file; my rows now live in a new `BotCharacterRows.kt`, called from `instructionsRow` |
| `apps/android/app/.../UsageScreen.kt` | "Your week" import and card against upstream's `UsageWindow` import and new empty text | Kept both |
| `apps/desktop/src/shared/models.ts` | My `Bot` fields and `WeekSummary` against upstream's `offeredToNetwork`, `DoctorReport`, `TokenBudget`, `BotBundle` | Kept both; the duplicated `computerPermissions` line is upstream's only |

## Nothing lost

Mine, present after the merge (`git grep`): `approval_age.rs`, `verify.rs`, `for_background` in `launch.rs`,
`Personality.kt` and the soul/persona/heartbeat fields, `week.rs`, `WeekCard`, `SETUP_PROMPT.md`, the fields of
`BotDraft`, `models.ts` and the editors.

Incoming, present: `api/apps_bridge.rs`, `proactive.rs`, `api/apps_runtime.rs` (`dispatch_bot_turn`), `MiniAppBridge`,
`MiniAppWebClient`, `MiniAppBridgeScript`, `DecisionActions`, `collectPendingDecisions` with `allowOptionId`,
`takeoverRequest`, `ProactiveCheckIns`, the bot catalog and teams.

## Fixes after the merge (separate `fix:` commits)

- `host/src/agent/bot/approval_age.rs`: the newer Rust 1.99 clippy asks for `Duration::from_hours` (`duration_suboptimal_units`).
- `apps/android/core/.../ChannelIntegrationTest.kt`: "a slow consumer receives catch-up…" waited 8 s for 600 events with
  `delay(2)`; where the clock ticks every 15 ms (Windows) that is about 9 s, so it failed in the full run and passed alone.
  Budget raised to 30 s. Not caused by the merge.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | passes (exit 0) |
| `cargo clippy --all-targets -- -D warnings` | passes (exit 0) after the `from_hours` fix; the first run died when rustc ran out of memory (about 1 GB free), the retry passed |
| `cargo test -j 1 --bin codync-host -- --test-threads=1` | 809 passed, 0 failed, 1 ignored |
| Desktop `npm run typecheck`, `npm test` | clean; 385 passed, 0 failed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, BUILD SUCCESSFUL in 5m 27s) after the test-budget fix; the two earlier runs failed only on that test |

## Not tested

No agent was run against the merged build; nothing was exercised on screen. iOS was not touched or compiled.
