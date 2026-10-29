# UI and device access follow-up — 2026-10-22

Last updated: 2026-10-22 23:16 Asia/Taipei. Desktop and iPhone builds installed; final validation/handoff below.

This is the task ledger for the ongoing conversation. Later requests add to this
list unless the user explicitly cancels an earlier request.

## 1. Thinking bubble and inline preview — completed

- [x] Smooth Electron thinking bubble entrance and exit, including its occupied space.
- [x] Make the disclosure arrow show simple thinking text inline instead of Full conversation.
- [x] Preserve Starting agent, activity labels, waiting states and elapsed time.
- [x] Preserve existing iOS message insertion/removal and scrolling animations.
- [x] Fix iOS preview collapse overlapping the status bubble: opacity transition and clipping.
- [x] Apply the disclosure behavior to Android and TUI; fix Android bottom following.
- [x] Review, tests and app checks; merge and push to dev.

Source commit: `50237e48`; merge: `74637b0d`. Desktop: 214 tests;
iOS: 110 tests plus simulator walkthrough; Android checks and emulator UI test;
Rust checks and terminal walkthrough passed. Linux/Windows UI was not exercised.

## 2. Install Dev builds — partially completed

- [x] Build, install and launch desktop Codync Dev with thinking changes.
- [x] Build iOS Dev for a physical device.
- [x] Install and launch iOS Dev on the user's iPhone 18 Pro Max
  (`00008160-00060DD201900036`). Initial disconnected-device blocker is resolved.
- [x] Rebuild/reinstall desktop Dev after the clarification: the mascot itself has lowered eyes and a downturned mouth, with no emoji badge. Visual failure check and installed app connection passed.
- [x] After device-access changes, rebuild and reinstall iOS Dev on the same iPhone.
- [x] Launch the new phone build after the user unlocked it; devicectl confirmed launch on iPhone 18 Pro Max.

The phone currently has the completed thinking changes. The later device-access
changes are implemented and installed on the physical phone; launch succeeded after the user unlocked it.

## 3. Codync mascot for desktop host states — in progress

Worktree: `/Users/libokai/mycode/swiftui/Codync-host-state-mascot`.
Branch: `fix/host-state-mascot`; base: `74637b0d`.

- [x] Replace the primary system icon with the existing Codync character.
- [x] Differentiate starting, setup and failure: moving glance, idle, still frown
  with lowered eyelids and a downturned mouth, like 😞. The user clarified that this is the character's own expression, not a separate emoji badge. Retain the error message and retry action.
- [x] Review compact layout, reduced motion and accessibility in code.
- [x] Fix the light-mode badge contrast found during review; the user subsequently clarified that the character itself should look like 😞; the badge has been removed.
- [x] Identify and fix the unexpected rounded background reported by the user:
  the generic `working` class collided with the chat bubble style. The mascot now
  uses `is-working` instead.
- [x] Rerun affected checks after the background and emoji corrections: typecheck, 214 tests, build and commit hooks passed.
- [x] Visually verify the background-free failure screen and automatic recovery to chat after the test host starts; install and launch final desktop Dev.
- [x] Final facial expression visually checked in Electron: no badge or rounded background.
- [ ] Additional compact/light-mode screenshots were not captured; code was reviewed. Retry callback is unchanged; sandbox host recovery was manual.
- [x] Commit initial mascot as `d003032b` and final disappointed expression as `3fd08898`.
- [ ] Merge and push when release coordination below allows it; keep worktree and test app available.

Owned source files: desktop `components/Avatar.tsx`, `views/HostStateView.tsx`,
`views/chat-window.css`; documentation: `docs/architecture/desktop-app.md`.
The local host service management screen is desktop-only. iOS, Android and TUI
connect to the service but do not install/start it; their corresponding connection
and splash implementations were inspected.

## 4. Unreachable computer and account-device actions — implemented and installed

User requirements:

- When the phone cannot reach a computer, show a useful error and an
  **Ask for access** action.
- Devices belonging to the same account must not offer a delete/remove-device
  action in the phone's device list.
- Keep the error text readable; the supplied screenshot shows the row is crowded.

No answer to the optional scope clarification arrived. Proceeding with the stated assumption: phone lists (including older copies) cannot remove account computers; desktop account management remains available.

Implementation checklist:

- [x] Trace paired-but-unreachable, unauthorized/revoked, pending request and
  previously granted states through existing account/access APIs.
- [x] Reuse the current request-access flow; do not falsely promise that requesting
  access fixes an offline computer, and retain the accurate connection error.
- [x] Adjust the phone row layout so the recovery action and explanation fit.
- [x] Remove account-device deletion entry points according to confirmed scope;
  preserve removal for independently paired devices where applicable.
- [x] Apply corresponding changes to iOS/Android and inspect desktop/TUI parity.
- [x] Add regression coverage: own-grant-only renewal, actual request approval, preservation of direct addresses/route/color, no account Remove, replacement Ask enabled, queued approval handoff.
- [x] Run review, builds/tests and available real-app checks; install iOS Dev on the phone. Remaining validation limits are recorded below.
- [ ] Update documentation, version and iOS What's New as required; commit and push.

Likely starting points: `apps/ios/Views/SettingsView.swift`,
`apps/ios/Kit/Sources/CodyncUI/Store/AccountStore.swift`, Android `SettingsView.kt`
and `ConnectionPresentation.kt`, desktop `views/settings/ComputersView.tsx`.
Changes are in the worktree’s iOS and Android settings/access views, account stores and Swift CloudClient. New regression checks cover renewing only this phone’s grants, waiting for the actual new approval and preserving preferences. Android emulator UI tests cover the recovery button and no account-device removal. iOS 87 UI-store tests pass after removal of 2 obsolete account-deletion tests. Android core 145 and app 26 unit tests pass; build/lint pass. All new Android UI checks passed; two existing UI-test assumptions were fixed and both failed tests passed on retest.

## Order and release coordination

1. Finish mascot validation and desktop installation.
2. Implement device-access and removal rules, using the pending scope answer.
3. Validate and install the updated phone app.
4. Complete commits, release metadata, merge/push and temporary-environment cleanup.

Release PR #194 (2.22.0) was still open at the last check. Its owner
`kai/implement-codync-issue-179` is waiting for Greptile and CI on `74637b0d`.
The earlier thinking changes are already included. We agreed not to push the new
mascot change while that release is pending. Once it merges, recheck versions and
add the next patch bump before a new source change reaches main. Notify the release
owner of any change to that arrangement.

Preserve unrelated main-checkout changes: `AGENTS.md`,
`docs/guides/clean-code.md`, `host/crates/memory/README.md`, and `.agents/`.
This ledger is owned by the current UI task. Keep progress current and do not
mark an install, UI check or release complete based only on a successful build.

## Final review and validation record

Three after-change reviewers plus independent verification confirmed and fixed:

1. Renewal was dropping known direct addresses while keeping Direct-only routing.
   Both phones now retain addresses and mailbox key only for the same pinned identity.
2. Hiding Android account Remove could strand a revoked old pairing. Another account
   computer can now be approved while the old pairing remains; only successful approval
   retires the old channel/writers and switches the local mirror. Account records remain.
3. An Android approved handoff could be dropped during a busy session operation. It now
   queues behind it and rejects results from a retired context/generation.

Cleanup removes phone account-deletion APIs, their unused invalidation state and
removal-only tests. Older-copy detection and local independent-pairing removal stay.
Bilingual 2.22.1 notes are drafted; version bump and push remain held for release #194.

E2E evidence:

| Platform | Flow | Result |
| --- | --- | --- |
| macOS desktop | Final disappointed mascot; no emoji or rounded background | pass, Computer Use screenshot |
| macOS desktop | Installed Dev starts and reconnects | pass, 1 connected |
| iOS simulator | Install, launch, onboarding skip, Computers & settings | pass, Device Hub |
| iOS simulator | Live signed-in account recovery | not_run: dedicated simulator has no signed-in account; store/API regression test passes |
| Physical iPhone 18 Pro Max | Final build install | pass, devicectl reports new app installation |
| Physical iPhone 18 Pro Max | Final launch / visual check | launch pass after user unlock; physical visual walkthrough not performed |
| Android emulator | New recovery, actual-request approval, no account Remove, replacement entry point, queued handoff | pass, instrumentation |
| Android emulator | Full account/connection subset | 15/15 pass across initial run and targeted retest; spinner clock and ambiguous hidden-bot label selector corrected |
| Linux / Windows | Desktop mascot | not_run: minimal macOS E2E; shared renderer, no platform code changed |

The mascot worktree and its Electron test process remain available because the
coordinated release hold prevents merge/push. Production Codync remains untouched.

Phone source committed as `effa2091`; pre-commit Android checks passed. Desktop and phone source commits remain local, not merged/pushed, under the existing #194 release hold. No version bump was made while 2.22.0 is still pending; bump to the next patch before these source changes reach main.

## Computer-card polish (23:40)

- iOS and Android online cards now use a compact two-row layout: full computer name,
  then connection summary and adjacent action buttons. Removed the redundant route
  glyph. iOS falls back to stacked content when the summary and actions do not fit;
  Android allows the summary to wrap. Offline explanations and access recovery stay.
- Desktop and TUI are unaffected: neither uses this phone paired-computer card.
- Three read-only reviews found no defects. iOS device/simulator builds pass;
  Android compilation, unit tests and lint pass.
- iOS Device Hub: paired a dedicated test host, checked the rendered card and opened
  the Connection menu successfully. Android emulator: card controls and unreachable
  account recovery pass (2 instrumentation tests); captured the rendered card.
  API 36.1 test runner failed in Espresso's InputManager initialization; the same
  checks pass on API 36. Enlarged-text/narrow-screen visual checks remain not run.
- Installed the new card on iPhone 18 Pro Max. Automatic launch was blocked by the
  phone being locked; installation itself succeeded.
- PR #194 regression coverage is pushed separately as 62672102. Latest dev removed
  inline thought previews in 2b0cf291, so coverage follows the remaining behavior:
  420 ms exit retention/removal and cancelling removal during a restart. All 215
  desktop tests and typecheck pass. Live desktop E2E could observe onboarding but
  CUA failed to operate that Electron window (noWindowsAvailable), so it is not
  recorded as passed. Other agents' uncommitted files were preserved during rebase.

The held branch is rebased onto `62672102`, preserving latest dev's no-preview chat
and compact approval cards. Synced physical iPhone build passed and was reinstalled
successfully. Card polish commit is `27662acc`; #194 is still open, so the separate
mascot/access/card source commits remain local pending the coordinated release.
The main thinking-review Electron/Vite/host and temporary simulators were stopped;
the held mascot worktree's development app remains available until its push.

## CI and release handoff (2026-10-23)

- #194 merged as `4f18c688` after every CI check (including notarization) and
  Greptile passed, with all review threads resolved.
- Version 2.22.1 is aligned across iOS, host and desktop. The complete UI work is
  pushed to dev and release PR #195, with phone-card screenshots in the PR.
- PR review identified Android approval being hidden while a session handoff waits.
  The confirmed cloud approval is now recorded before queuing; it is restored after
  context loading as before. Three narrow after-change reviews found no new defects.
  Android unit/build/lint checks and three emulator regression cases pass, including
  the real handoff function blocked behind a busy session and request-specific UI
  success. This is Android's queued context handoff; iOS awaits its own transition.
- A memory-layer fullness finding from the pre-#194 comparison is being checked by
  the original memory owner before #195 merges. CI/review are not bypassed.

## Final renewal review fix

Adopted the memory owner's `019b4876` implementation, including per-layer prompt
labels and codync-mem 0.3.1, instead of the equivalent local draft. Android's queued
approval fix is pushed as `fb82a9d1`.

Explicit renewal now cancels and joins any earlier negotiation before revoking this
phone's grants and requesting replacement access. iOS also withdraws failed/cancelled
nonce exchanges and logs cleanup failures while retaining the original error. Normal
repeated requests still join or ignore the active negotiation.

Three narrow reviews found no correctness/refactor defect. The completeness review's
optional cleanup-completion coverage suggestion was verified as nonblocking. Android
now has a NonCancellable cleanup barrier proving replacement waits for cleanup; its
updated APK and targeted emulator test pass. iOS retains its passing endpoint-order
regression (old request DELETE, own grant DELETE, replacement POST); a more elaborate
URLProtocol completion barrier is not added. iOS tests pass 2/2, Android unit/build
checks and two initial targeted emulator cases pass, and the hardened test passes
1/1. A baseline iOS run failed until nonce cancellation cleanup was implemented.
Test devices were stopped; the preexisting iPad was untouched.

The speech/keyboard/module-size comments from the old pre-#194 comparison were
independently verified to concern files identical to current main; they were resolved
as outside this PR's current scope, not represented as fixed.
