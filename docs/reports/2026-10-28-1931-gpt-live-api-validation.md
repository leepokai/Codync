# GPT-Live API adaptation validation

Date: 2026-10-28 (America/Los_Angeles)
Branch: `codex/gpt-live-api`; base: `ed4f89fc8f588f239e6b9c55a55fc6ae24896158`.

## Change

Adds opt-in GPT-Live WebRTC sessions to desktop, iOS and Android. The Rust host creates sessions with its stored API key and routes the existing six call tools through Responses (`gpt-6-luna`). Realtime defaults and existing selections stay unchanged. The new host advertises `voiceStatus.supportsLive`; clients reject saved Live selections on older hosts before calling unsupported methods.

GPT-Live audio and Responses routing are billed separately. The API model list for the test account returned `gpt-live-1`, not `gpt-live-1.1`. No credentials are part of this report or repository.

## Automated validation

- Desktop: TypeScript check and 280 tests passed, including host capability checks, event ordering, stale-session rejection, graceful close, ICE gathering, actual playback-state handling, and delayed setup failures during reconnect.
- Host: full `cargo test` passed 653 tests with one pre-existing OS-keyring test ignored. Subsequent targeted Live protocol tests (13) and voice tests (10) passed. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and final debug build passed.
- iOS: full package run passed 306 tests; final focused run passed 169 tests after compatibility changes. Signed iOS Dev simulator build/install/launch passed.
- Android: `:core:test :app:testDebugUnitTest :app:assembleDebug :app:lintDebug` passed; 172 core tests (one pre-existing skip) and 44 app tests in initial run, with final Live regression suite expanded to seven passing tests. Lint reported zero errors.

## Review and fixes

Three read-only review perspectives covered correctness/security, completeness and refactoring, followed by verification of findings.

- Fixed desktop stale setup errors and late SDP completion affecting replacement connections; added a regression test.
- Fixed iOS cues arriving before the Live transport was created being queued for the Realtime protocol; Live uses a typed queue.
- Made iOS ICE gathering timeout fail rather than send an incomplete offer.
- Cut off microphone and playback immediately on Android hangup before waiting for close acknowledgement.
- Added optional-feature capability gates for new clients connected to older hosts; fixed contradictory credential documentation.
- Fixed interleaved Responses tool batches: return all known outputs before continuation; failed or unsafe cancelled sibling responses now fail explicitly rather than hanging silently.
- Coalesced Live transcript fragments for readable call history while preserving original fragments/timestamps; never modify words already handed to a bot.
- Added Live-only API-key validation without creating a billable silent Live session.
- Refactor review found no remaining structural issue requiring changes. Necessary module extractions are separated from feature work; no generic cross-language engine abstraction was added.

## App checks

All app tests used isolated development data, with the shared production apps left intact.

- Desktop real app: OpenAI settings, saved key, `gpt-live-1` selection and separate-billing note observed. The final normal-microphone call lasted 2m49s. The user confirmed an audible reply, then a second reply with a successful bot-status lookup (no task dispatch requested). WebRTC connected, input/output transcript events and nonzero media activity were observed. The call ended normally. Temporary phase/media diagnostics were removed; TypeScript and 23 focused regression tests passed afterward.
- iOS Device Hub: dedicated simulator paired to task host; no-key state, real model discovery, selecting `gpt-live-1`, billing note, old-host update error and successful **Model access verified** result observed.
- Android emulator: dedicated read-only AVD paired to task host; equivalent settings, capability rejection and successful model-access test observed. A real `gpt-live-1` session reached Listening for about 22 seconds with host microphone input disabled, then ended normally; no crash or close-finalization warning was observed.
- Windows/Linux app E2E unavailable: Lima and UTM executables/VM tooling were not present; no VMs were installed.

### Desktop input incident during QA

The coordinator temporarily launched Electron with a synthetic microphone fixture to avoid capturing room audio. Chromium could not read that fixture under its media sandbox. The user tested that same instance before normal input was restored, so the model could speak its greeting but received no human speech. Process arguments and media log errors confirmed the cause; all three affected call histories contained zero user transcript fragments. The synthetic flags were removed and the sandbox relaunched normally. Chromium security settings were not weakened. This was a QA launch configuration error, not a completed audio acceptance test.

### Subsequent desktop retry

After the launch flags were restored, the user reported another silent attempt. Its precise cause was not established. A fresh diagnostic call confirmed session start, model transcript/audio arrival, an active unmuted HTML audio player, and the system default built-in microphone and speakers. The user then explicitly confirmed audible interaction and a second successful bot-status query. This recovery does not prove the earlier silent attempt had the same cause as the synthetic-input incident; no speculative audio workaround was added.

## Remaining acceptance

Desktop real microphone conversation and a read-only bot-status tool round trip were confirmed by the user. Phone real spoken conversation, interruption, long calls, and forced network-drop recovery have not been accepted on physical devices. Automated lifecycle tests do not substitute for those checks. The test account exposed `gpt-live-1`; `gpt-live-1.1` was not live-tested. iOS session creation/audio was not run: Device Hub kept Input=System after attempts to select None, so environment-audio isolation could not be confirmed. Model-access checks passed, but are not a substitute for a real iOS call.

## Handoff

The task branch is submitted for review, without merging into dev or main. The main task worktree and isolated sandbox are retained for follow-up. No real user audio or credentials are included in repository artifacts.


## Screenshots

- [Desktop GPT-Live settings](../assets/gpt-live/desktop-live-settings.png)
- [iOS model access](../assets/gpt-live/ios-model-access.png)
- [Android model access](../assets/gpt-live/android-model-access.png)
- [Android connected call](../assets/gpt-live/android-live-ready.png)
