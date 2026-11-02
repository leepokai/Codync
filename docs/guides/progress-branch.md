# Progress of the branch `deska/Implementar-Motor-de-Flujos-de-Rutina-5fa5448a`

Control file. Update it when a topic closes; resume from here if the context is compacted.
Rules in force: one topic at a time, small commits, push to `origin` without `--force`, no version bump, no tags,
iOS written but **not compiled** on this PC. Gates: host `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test -j 1 -- --test-threads=1`; desktop `npm run typecheck`, `npm test`; Android
`./gradlew.bat assembleDebug testDebugUnitTest :core:test lintDebug`.

## Before this plan (already on the branch)

Approval reminder at 4 h and close at 24 h (`approval_age.rs`), `verify_command` (`verify.rs`; desktop and Android fields),
background model (`launch.rs`; free text), soul/persona/heartbeat (desktop and Android), `weekSummary` and the "Your week"
card (desktop and Android), `SETUP_PROMPT.md`.

## Topics

| # | Topic | State | Closing commit | Not tested |
|---|---|---|---|---|
| 1 | Put the branch on the current base (`origin/integrate/upstream-2.29`) | done | merge `6c20c14c` (+ two `fix:` commits) | iOS untouched; no agent run on the merged build; see `base-update-report.md` |
| 2 | Close what was left half done (A-E) | done | see `git log` (`feat: Your week`, saver, takeover commits) | iOS not compiled; no agent run; thresholds only with injected clocks; see `half-done-report.md` |
| 3 | Forbidden commands | done | see `git log` (`feat(host): forbidden commands`) | no UI; real agents not run; obfuscated commands not caught; see `forbidden-commands-report.md` |
| 4 | Honest end of turn | done | see `git log` (`feat(host,tui): an honest end of turn`) | no real agent run to a step limit; Paused push not sent for real; Android card compiled, not seen; iOS skipped by instruction; see `turn-verdict-report.md` |
| 5 | Approval rules editor | done | see `git log` (`feat(host): one decision for an approval`) | seen on no screen; no real agent asked for a permission; iOS skipped by instruction; see `approval-rules-editor-report.md` |
| 6 | "Always" as a rule and `actionRules` | done | see `git log` (`feat(host): 'always allow' keeps a rule`, `quick rules by kind of action`) | seen on no screen; no real agent offered allow_always; browser, apps, bots and routines classes left out on purpose; iOS skipped by instruction; see `always-and-quick-rules-report.md` |
| 7 | Security profiles | done | see `git log` (`feat(host): security profiles`) | seen on no screen; no real agent ran under a profile; daily limits not exercised; path scopes do not apply over ACP; iOS skipped by instruction; see `security-profiles-report.md` |
| 8 | Mini-app and check-in fixes | done | see `git log` (`fix(host): a half-done scheduled check-in stays private`) | items 1-5 were already done by the merged base (evidence in the report); the one-hour real-host e2e not run; the Android WebView callbacks not seen; see `miniapps-and-check-ins-report.md` |
| 9 | Questions and plans from external sessions | done | see `git log` (`feat(host): answer a terminal session's questions and plans from any client`) | no real Claude Code session was run; plan approval output (`allow` + echoed input) is undocumented by Claude Code; multi-select questions stay in the terminal; see `external-questions-report.md` |
| 10 | Messaging channels (3 phases) | done | see `git log` (`feat(host): Slack Socket Mode`, `feat(host): Telegram channel`, `feat(...): Channels screen`) | nothing tested against real Slack or Telegram (see `slack-manual-checklist.md`, `telegram-manual-checklist.md`); Android and terminal cannot add a Telegram bot or set up Slack (tokens stay on the computer); no Slack form in any client; iOS skipped; see `channels-report.md` |
| 11 | Computer health | done | see `git log` (`feat(host): computer health`) | real sampler and runaway-agent stop only tested with simulated readings; no screen seen; "ask before an expensive turn" not done (documented); iOS skipped; see `computer-health-report.md` |
| 12 | Import agents | pending | | |
| 13 | Memory as beliefs | pending | | |
| 14 | Proposed skills inbox | pending | | |
| 15 | Demo mode | pending | | |
| 16 | Branch close | pending | | |

## Not touched on purpose

Routines by nodes (`feat/routine-flows-*`), Android widgets (`feat/android-widgets-v2`), syncing with `upstream/main`.

## Where to resume (written when the work was stopped)

- **Rule from the user, after the plan was written: no iOS, iPhone or Mac work.** Skip `apps/ios` in every topic from
  now on. iOS code written earlier in topics 2 (commits `e3de78f9`, `ff044681`, `b0903e8f`, never compiled) is still in
  the branch; decide whether to revert it before the final push.
- **Topic 4 left to do:** run `./gradlew.bat assembleDebug testDebugUnitTest :core:test lintDebug --no-daemon
  -Dorg.gradle.jvmargs=-Xmx2g` in `apps/android` (nothing else compiling), then the full host suite
  (`cargo test -j 1 --bin codync-host -- --test-threads=1` with `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1`), fill the Android
  row of `turn-verdict-report.md`, mark topic 4 done, push without `--force`.
- **Then topic 5** (approval rules editor), skipping iOS.
- Machine notes: 8 GB; kill the idle `opencode.exe` processes (the user allowed it) if rustc dies with
  "memory allocation failed"; Gradle and cargo never at the same time.
