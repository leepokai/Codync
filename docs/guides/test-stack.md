# Test stack

Target design for the end-to-end (e2e) checks. **Status: not installed yet.** Playwright and Maestro are not in the repo, and the Electron tool is still open (see [Open items](#open-items)). Until the harness below is checked in, every e2e check keeps using Computer Use through cluadex, as [CLAUDE.md](../../CLAUDE.md#end-to-end-test-every-change) says.

## Tiers

1. **Unit and component**: unchanged. `npm run typecheck && npm test` (desktop), `cargo test` (host), Swift tests (`apps/ios/Kit`), Gradle checks (Android).
2. **E2E per platform**:
   - **Electron desktop**: Playwright through `_electron` (renderer over CDP). Playwright's Electron support is documented as experimental, so it is one of two candidates. The other is WebdriverIO with `wdio-electron-service`, which reaches the main process and can load the electron-vite entry through `appEntryPoint`. Spike both on Electron 44 before choosing.
   - **iPhone**: Maestro flows on the worktree's own simulator (`Codync <sandbox name>`, created by `npm run dev`). XCUITest is optional for iOS-only checks that Maestro cannot express.
   - **Android**: Maestro on the shared `codync_worktree` emulator, under its lock as the [after-change skill](../../.agents/skills/after-change/SKILL.md) describes.
   - **Native gaps**: Computer Use through cluadex, only for the steps tagged `@native`.

## What Computer Use covers

Only these steps go to Computer Use. Everything else uses Playwright or Maestro.

- **Desktop**: the tray or menu bar icon and its menu; the global shortcut (approval island); system notification banners; macOS privacy prompts (screen recording, notifications); the Clerk sign-in in the system browser and the `codync-dev://` callback back into the app; features that need the packaged `Codync Dev.app` (Codync Screen, computer use, `SMAppService` registration).
- **Phones**: how system alerts and notification banners look on the simulator or emulator; the desktop QR pairing, where Computer Use reads the QR on the Mac screen and Maestro completes the pairing on the phone; widgets, Live Activity and the Dynamic Island.

**Covered by neither**, so check on a physical device and say so in the report: microphone and voice-call audio; real APNs delivery (the simulator can only simulate it with `xcrun simctl push`).

## Parallelism

- **Playwright and Maestro run in parallel.** Each worker gets its own instance: its own sandbox (port 19300–19399), simulator or emulator, Clerk test account and Electron `userDataDir`. Tests must not depend on each other's state.
- **Maestro sharding**: start with the device count that is stable on this Mac. One team reported freezes above four simulators on virtualized CI, and seven running on native Mac hardware. Measure here before raising the count.
- **Computer Use runs in one lane.** One CU task at a time, in the background, never in the foreground. If cluadex fails, stop and ask the user to reconnect with `/mcp`. Do not silently switch to cua-driver.
- Tag CU steps `@native` so a regression run can skip them and the release run includes them.

## Rules

- Each changed flow names the tier that covers it. A `@native` step names the single native action it checks.
- Do not use Computer Use for anything Playwright or Maestro can express. It is slower and cannot run in parallel.
- Report every check that did not run and why, as the existing e2e rule requires.

## Open items

- Choose the Electron tool (Playwright or WebdriverIO) through a spike on Electron 44.
- Check Maestro's behavior on several simulators at once here, and record the stable count.
- Install the chosen tools and check in the first flows. Wire them into CI.
- Once the harness exists, update [CLAUDE.md](../../CLAUDE.md), [AGENTS.md](../../AGENTS.md), the `after-change` workflow (`.claude/workflows/after-change.js`) and the [after-change skill](../../.agents/skills/after-change/SKILL.md) to this split. Until then they stay on Computer Use.
