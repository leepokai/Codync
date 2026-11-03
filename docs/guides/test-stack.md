# Test stack

How the end-to-end (e2e) checks drive the apps: Playwright for the desktop app, Maestro for the iPhone and Android apps, and Computer Use only for the native steps neither can reach. Unit and component tests stay as they are (`npm run typecheck && npm test`, `cargo test`, the Swift tests in `apps/ios/Kit`, the Gradle checks).

## Desktop: Playwright

Playwright attaches to the worktree's running sandbox over the DevTools protocol. It doesn't launch Electron itself: the sandbox already owns the host, data, sign-in and single-instance lock, and an app launched without them would open against the shared Codync Dev.

```sh
cd apps/desktop
npm run dev -- --cdp   # the worktree's sandbox, DevTools on its port + 10000
npm run e2e            # in a second shell: e2e/*.spec.ts against that sandbox
```

`e2e/sandbox.ts` finds the sandbox from its `sandbox.json` and attaches to its main window; `e2e/smoke.spec.ts` checks the app renders and runs against the sandbox's own host. A spec reaches the main process only through the `window.codync` bridge, as the renderer does. One worker per worktree, since one app runs per sandbox; worktrees run in parallel with each other.

## Phones: Maestro

One flow in `apps/maestro/` serves both phones: it launches the iPhone or Android Dev app by platform (`runFlow` with `when: platform`) and asserts the same copy, since the apps match screen for screen. Run every phone in one Maestro process:

```sh
maestro --device "<simulator udid>,<adb serial>" test --shard-all 2 apps/maestro
```

The devices are the worktree's own simulator `Codync <sandbox name>` and the shared `codync_worktree` emulator, under its lock ([worktree sandboxes](dev-app.md#worktree-sandboxes)). Install the worktree's builds first: `xcodebuild … -scheme "iOS Dev" -configuration DevDebug -derivedDataPath build/dd` for the simulator, `./gradlew :app:assembleDebug` for the emulator.

**One Maestro process per Mac.** Two at once break each other: with Maestro 2.1.0, an iPhone run beside an Android run made the Android driver refuse connections on port 7001 in three of three tries, and two runs starting in the same second removed each other's log folder in `~/Library/Logs/maestro`. Shard inside one run instead. Across worktrees, take the lock folder `/private/tmp/claude-$(id -u)/maestro.lock` the way the emulator lock works: `mkdir` it (it fails while another run holds it), write your worktree's path into `owner`, `rm -r` it when done; one older than an hour, or whose worktree is gone, is stale.

## What Computer Use covers

Computer Use (cluadex, in the background) checks only these steps. Tag them `@native` in the change's e2e list. Everything else uses Playwright or Maestro. If cluadex fails, stop and ask the user to reconnect it with `/mcp`; don't switch to cua-driver.

- **Desktop**: the tray or menu bar icon and its menu; the approval island's global shortcut; system notification banners; macOS privacy prompts (screen recording, notifications); Clerk sign-in in the system browser and the `codync-dev://` callback back into the app; anything that needs the packaged `Codync Dev.app` (Codync Screen, computer use, `SMAppService`).
- **Phones**: how system alerts and notification banners look; the desktop QR pairing (Computer Use reads the QR on the Mac, Maestro finishes on the phone); widgets, Live Activity and the Dynamic Island.

**Covered by none of them**, so check on a physical device and say so in the report: microphone and voice-call audio, and real APNs delivery (a simulator only simulates it with `xcrun simctl push`).

## Rules

- Each changed flow names the tool that covers it, and a changed flow that Playwright or Maestro can express gets a spec or flow in the same change.
- Computer Use never runs two tasks at once. Playwright and the one Maestro run go beside it.
- Report every check that didn't run and why.

## Why these tools

- Playwright's Electron launch (`_electron`) is documented as experimental, and launching the app directly bypasses the sandbox. Attaching over the DevTools protocol uses the sandbox as `npm run dev` starts it, and matches where Playwright's Electron support is heading.
- Maestro runs one YAML flow on both phones, which fits the rule that every iPhone change lands on Android and is checked on both. XCUITest stays available for iOS-only checks Maestro can't express.
- Computer Use is slower and holds the one screen and cursor, so it covers only what no in-app driver can reach.
