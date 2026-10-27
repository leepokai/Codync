# Session context meter — implementation and validation

Date: 2026-10-26, America/Los_Angeles. Branch: `codex/session-context-meter`.
Base: `origin/dev` at `b000ad983fcab98c631cf1a0c1cb87d527d2b0dd`.

## Result

The rightmost composer control is a context ring, matching Live's visual height: 25px on desktop and 34pt/dp on phones, with 44pt/dp phone hit targets. Clicking opens a small bubble above the ring with percentage and used/total tokens. Outside clicks and Escape/Back dismiss it. Phone bubbles label the reading as the agent's last report. The terminal shows a compact panel above the composer's right edge, opened by click or `W` and closed with Escape.

Unknown usage is a dashed ring and a dash, rather than zero. Amber starts at 80%, red at 95%. Groups hide the control because they have no single session window. The optional wire field supports older clients and hosts.

## Data ownership and limits

The host uses ACP `usage_update.used` and `size`, without estimating from chat text or cumulative billing tokens. It calculates integer percentage/severity, persists by bot, effective backend and session, and publishes bot events. Missing/zero capacity is unknown. This measures the coding agent's reported context, independently of subscription usage and the realtime voice model. It updates only when an agent reports usage; adapters without a reported capacity cannot provide a percentage.

Fresh `session/new` clears the previous record even if an adapter reuses a process-local session ID. Resumed sessions keep their last reading across host restart. Routine sessions cannot replace the main reading. Parked fallback agents keep separate measurements even with equal session IDs.

## Review and cleanup

Three read-only reviewers covered correctness, completeness and refactoring. A separate verifier checked the findings.

- Fixed stale readings after non-resumable agent restarts and collisions between fallback agents: backend namespacing, explicit new-session clearing and regression tests.
- Updated the additive wire-shape snapshot and passed its test; compatibility floors remain unchanged.
- Reused the desktop `ProgressRing`. Extracted the existing anchored panel unchanged into a shared component in a separate refactor commit, preserving the avatar picker.
- Reused Android menu placement; consumed internal bubble taps so they do not dismiss it, and added accessibility pane/dismiss semantics.
- No unresolved confirmed correctness findings remain. An existing fallback switch sequence may publish a transient snapshot before its synchronous final backend-marker update. Coalescing that sequence is outside this feature.
- Version 2.29.0 already exceeds the latest local v2.28.0 tag, so no extra version bump is required. Both phone release notes, shared translations, developer docs and the user manual are updated.

## Automated verification

| Check | Result |
| --- | --- |
| Desktop TypeScript and Node tests | Passed; 272 tests |
| Host unit suite before review fixes | Passed; 538 tests, one unrelated ignored test |
| Final focused usage unit tests | Passed; 3, including backend isolation and overflow/thresholds |
| Host usage integration tests | Passed; 4 real-host tests covering SSE/full sync, restart/reset, compaction, zero/unknown, ID reuse and routine isolation |
| Wire-shape snapshot test | Passed after recording the optional field |
| Final Rust formatting, all-target Clippy with warnings denied, build | Passed |
| Final TUI keyboard/mouse regression | Passed |
| iOS package suite | Passed; 158 Swift Testing tests, including optional/known/future-level decoding |
| iOS final bubble build | Passed; signed Dev app installed in an isolated simulator |
| Android core/app unit tests, Debug build and lint | Passed; final app and instrumentation APK installed |
| Android final live UI test | Passed; AndroidJUnitRunner `OK (1 test)` |
| User manual check | Passed; 18 pages and 93 links |

Host and app acceptance use the repository's ACP fixture with controlled context readings. They verify transport, lifecycle and rendering; they do not establish whether any external agent adapter reports its provider's context accurately.

## App acceptance

| Platform | Flow | Result |
| --- | --- | --- |
| macOS Electron sandbox | Unknown ring; send fixture message; receive 25%; bubble shows 50,000 / 200,000; outside/Escape dismissal; same size/alignment as Live | Pass, Computer Use |
| iOS Simulator | Paired real host; 25%; compact anchored bubble with exact counts; outside dismissal; same height as Live | Pass, Device Hub |
| Android emulator | Paired real host; 25%; compact bubble and exact counts; inside tap retains it; Back dismisses and retains reading | Pass, live Compose instrumentation; screenshots inspected |
| Terminal UI | Real host; 25% at composer edge; `W` opens compact bottom-right counts; Escape closes | Pass, actual PTY and regression test |
| Linux/Windows desktop | Platform UI acceptance | Not run; repository minimal macOS E2E scope, with both affected phone clients separately checked |

Android test setup initially hit animated-UI clock idling and a shell-truncated pairing URL. Explicit frame advances and a correctly quoted fresh pairing intent produced the passing final run.

## Screenshots

- [Desktop ring beside Live](context-meter/desktop-composer.png)
- [Desktop anchored bubble](context-meter/desktop-detail.png)
- [Android ring beside Live](context-meter/android-composer.png)
- [Android anchored bubble](context-meter/android-detail.png)

## Handoff

Ready for a PR into upstream `dev`; not merged or deployed. The task worktree and isolated desktop app are retained for review. Only task-owned terminal/emulator processes are stopped after acceptance; production Codync and other worktrees remain untouched.
