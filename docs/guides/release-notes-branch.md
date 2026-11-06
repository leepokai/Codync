# Release notes of this branch

Branch `deska/Implementar-Motor-de-Flujos-de-Rutina-5fa5448a` (fork `Thndrr00/Codync`). Progress by topic, with the closing commit
and what was not tested: [progress-branch.md](progress-branch.md). One report per topic is in this folder.

**Proposed version: 2.30.0** (minor: new features and protocol additions, no removal). Not applied: no bump, no tag, no
`MARKETING_VERSION` change was made on this branch. The fork's base is 2.29.0; upstream is at 2.37.0 and was not merged
(by instruction), so the version has to be picked again after that merge.

## What is new, by area

**Host**
- Approvals: forbidden commands refused for every bot; approval rules (first match decides) with a tester; "Always allow"
  proposes a rule; security profiles (read-only, builder, operator, full) that write the rules and the daily token limit.
- A turn that stops early says why and can be continued; the bot's background model for check-ins, verification and drafts;
  the weekly summary; mini-app and check-in fixes (a half-done check-in stays private).
- Terminal sessions: questions and plans of Claude Code are answered from any client (`hooks install` adds the waiting hook).
- Channels: Slack Socket Mode (no public endpoint), a channel contract, and Telegram (pairing codes, allowed list, streamed
  replies, inline approval buttons, `/estado /rutinas /detener`).
- Computer health (memory, CPU, disk, per-agent memory) with a policy that stops new turns when memory is full.
- Import an agent's memory (`importAgentMemory`, credentials refused); memory as beliefs (confirm, correct, forget, undo, one
  contradiction question a day); proposed skills inbox.

**Desktop (Windows, Linux, macOS)**: approval rules editor and security profile picker, Channels, Computer health (settings
and island), Import an agent, "What I know" tab, proposed skills card, demo mode.

**Android**: the same screens ported (rules, profiles, Channels sheet, health rows, "What I know", proposals, demo mode).

**Terminal**: sheets for rules, channels, beliefs and proposals; a health word in the header; demo mode.

**iPhone**: nothing new, by instruction. Earlier iOS code of topic 2 (commits `e3de78f9`, `ff044681`, `b0903e8f`) was never
compiled and is still in the branch; decide whether to revert it.

## What was NOT tested

- **No screen was seen** on any client, and nothing ran against a real agent, a real Slack or a real Telegram (there are
  fake servers in the tests and a manual checklist for each: `slack-manual-checklist.md`, `telegram-manual-checklist.md`).
- **Topics 12 to 15 were written but not compiled or run** (the gates were left for one pass): agent import, memory as
  beliefs, proposed skills, demo mode. Their tests exist and have never been executed. Expect compile errors on the first
  build, mostly small. The first thing to run is: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test -j 1 -- --test-threads=1` in `host/`; `npm run typecheck` and `npm test` in `apps/desktop`; and the Android
  gate in `apps/android` (`./gradlew.bat assembleDebug testDebugUnitTest :core:test lintDebug --no-daemon -Dorg.gradle.jvmargs=-Xmx2g`).
- Gates last passed at the end of topic 11 (host 902 tests, desktop 401, Android build). Topics 8 to 10 and 11 each passed
  theirs when closed.
- **iOS** is not built and was not written to. **No signed APK or Windows installer was built**, and nothing was installed on a
  phone (the MIUI prompt step was not reached).
- Real memory pressure and the real `sysinfo` sampler were never exercised (simulated readings only).
- The expensive-turn question of topic 11, archive reading of topic 12, and per-fact metadata of topic 13 are not done.

## Files likely to conflict with the other branches

- With `feat/routine-flows-*` (routines by nodes): `host/src/api/bots.rs`, `host/src/hub.rs`, `host/src/store/mod.rs`,
  `host/src/tui/app/{events,overlay_keys,ui}.rs`, `host/src/tui/view/overlay.rs` (all hold the same kind of list: new methods,
  new overlays, new events). The screens of that branch's desktop and Android code also add to `RoutinesView` and the stores,
  which this branch does not touch.
- With `feat/android-widgets-v2`: `apps/android/app/src/main/kotlin/com/codync/android/AppStore.kt` (the demo's refusal in `ready()`)
  and `MainActivity.kt` (the demo swap of the state). Also likely: `SettingsView.kt`, `ChatRows.kt`, `MemoryCard.kt`,
  `BotDetailsView.kt` (small insertions).
- With upstream (`upstream/main`, not merged): `host/Cargo.toml` and `Cargo.lock` (`sysinfo`), `THIRD-PARTY-NOTICES.md`, `docs/guides/*`.
