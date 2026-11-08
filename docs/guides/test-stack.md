# Test stack

How the end-to-end (e2e) checks drive the apps: Playwright for the desktop app, Maestro for the iPhone and Android apps, and Computer Use only for the native steps neither can reach. Unit and component tests stay as they are (`npm run typecheck && npm test`, `cargo test`, the Swift tests in `apps/ios/Kit`, the Gradle checks).

## Host tests

`host/tests` drive real hosts against fake agents (the Python scripts in `tests/fixtures`). `tests/common` starts every test host with `CODYNC_AGENT_CHECKS=off`, as do `cloud/test/e2e/relay.e2e.ts` and `tools/check-connectors.py`, so it never runs the computer's agent CLIs for their sign-in and usage checks and never touches your logins. A test of those checks turns them back on (`tests/agent_checks_e2e.rs`). `tests/common` also sets `CODYNC_VAULT=folder`, so a test's credential key stays out of the system's credential store and goes with its data; `tools/check-connectors.py` keeps a real Keychain key on purpose.

Run them one at a time: never start a test binary several times in parallel, or several copies of the suite, not even to hunt a flake. Repeat it one run after another, a few times at most. On 2026-11-04 parallel repeats left about 200 `claude -p /usage` processes behind, using 55 GB on a 32 GB Mac, and froze it.

## Real agents

Everyday tests use the fake agents, which only answer the way we expect. Run the real ones too, one short turn per agent on the cheapest model, with nothing in parallel (it uses your own plan), when:

1. The registry version of `claude-acp` or `codex-acp` moves ([ACP adapter workarounds](../reference/acp-adapter-workarounds.md)).
2. A change touches how the host talks to an agent: ACP methods, sessions, turns.
3. Before a release.

## Desktop: Playwright

Playwright attaches to the worktree's running sandbox over the DevTools protocol. It doesn't launch Electron itself: the sandbox already owns the host, data, sign-in and single-instance lock, and an app launched without them would open against the shared Codync Dev.

```sh
cd apps/desktop
npm run dev -- --cdp   # the worktree's sandbox, DevTools on its port + 10000
npm run e2e            # in a second shell: e2e/*.spec.ts against that sandbox
```

`e2e/sandbox.ts` finds the sandbox from its `sandbox.json` and attaches to its main window; `e2e/smoke.spec.ts` checks the app renders and runs against the sandbox's own host. A spec reaches the main process only through the `window.codync` bridge, as the renderer does. One worker per worktree, since one app runs per sandbox; worktrees run in parallel with each other.

## Phones: Maestro

One flow in `apps/maestro/` serves both phones: it launches the iPhone or Android Dev app by platform (`runFlow` with `when: platform`) and asserts the same copy, since the apps match screen for screen. Run every phone in one Maestro process, with the tags [the check needs](#what-each-e2e-check-runs):

```sh
maestro --device "<simulator udid>,<adb serial>" test --shard-all 2 --include-tags smoke,<area> apps/maestro
```

The devices are the worktree's own simulator `Codync <sandbox name>` and the shared `codync_worktree` emulator, under its lock ([worktree sandboxes](dev-app.md#worktree-sandboxes)). Install the worktree's builds first: `xcodebuild … -scheme "iOS Dev" -configuration DevDebug -derivedDataPath build/dd` for the simulator, `./gradlew :app:assembleDebug` for the emulator.

A flow past the welcome screen (`agent-logos.yaml`) needs the phone signed in with `npm run phone-account` and keeps the app's state. `welcome.yaml` (`smoke`) clears it. That signs the Android app out, but the iPhone's sign-in stays in the simulator's Keychain, so run `xcrun simctl keychain <device> reset` before `smoke` on a signed-in simulator. `signed-out-pairing.yaml` needs a sandbox in no account and the phone signed out, and takes the pairing link as `PAIRING_URL`. Run a flow past the welcome screen in a Maestro run of its own, after the sign-in. Signed in isn't past first-run setup: `agent-logos.yaml` skips the choice page when it's there. The `computer-choice` flows need setup unfinished (fresh app data) and each finishes or changes it, so each gets its own run with its own start: `computer-choice` and `computer-choice-skip` with a computer in the account (`npm run dev -- --cloud`), `computer-choice-empty` without one.

Two things that look like failures but aren't the app's:

- A tag no flow carries yet makes Maestro stop with "Include / Exclude tags did not match any Flows" and exit 1. Leave out area tags that have no flows.
- Right after a cold boot the emulator can cover the app with "Pixel Launcher isn't responding", and the flow fails on its first assertion. Each run keeps a screenshot of the failure in `~/.maestro/tests/<run>/`: when a system dialog covers the app, clear it with `adb -s <serial> shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS` and run again. It happened twice in a row on 2026-11-03 until the dialog was cleared.

**One Maestro process per Mac.** Two at once break each other: with Maestro 2.1.0, an iPhone run beside an Android run made the Android driver refuse connections on port 7001 in three of three tries, and two runs starting in the same second removed each other's log folder in `~/Library/Logs/maestro`. Shard inside one run instead. Across worktrees, take the lock folder `/private/tmp/claude-$(id -u)/maestro.lock` the way the emulator lock works: `mkdir` it (it fails while another run holds it), write your worktree's path into `owner`, `rm -r` it when done; one older than an hour, or whose worktree is gone, is stale.

## What Computer Use covers

Computer Use (cua-driver, in the background: never bring an app to the front) checks only these steps. Tag them `@native` in the change's e2e list. Everything else uses Playwright or Maestro. If cua-driver fails, stop and ask the user to reconnect it with `/mcp`.

- **Desktop**: the tray or menu bar icon and its menu; the approval island's global shortcut; system notification banners; macOS privacy prompts (screen recording, notifications); Clerk sign-in in the system browser and the `codync-dev://` callback back into the app; anything that needs the packaged `Codync Dev.app` (Codync Screen, computer use, `SMAppService`).
- **Phones**: how system alerts and notification banners look; the desktop QR pairing (Computer Use reads the QR on the Mac, Maestro finishes on the phone); widgets, Live Activity and the Dynamic Island.

**Covered by none of them**, so check on a physical device and say so in the report: microphone and voice-call audio, and real APNs delivery (a simulator only simulates it with `xcrun simctl push`).

## What each e2e check runs

| Tool | In every `after-change` e2e | Why |
| --- | --- | --- |
| Playwright | The whole suite: `npm run e2e` | Each spec takes under a second, and each worktree has its own sandbox, so worktrees run it in parallel without waiting on each other. It is the regression net for host and shared-component changes, which reach every client. |
| Maestro | Only the flows tagged `smoke` plus the tags of the areas the change touches: `--include-tags smoke,<area>,…` | A flow takes 6–12 s and the Maestro lock lets one run at a time on the Mac, so a long run holds up every other worktree. |
| Maestro, whole suite | Not run anywhere for now | Decided on 2026-11-03 (see [Decisions](#decisions)). |
| Computer Use | Only the changed flows' native steps | The slowest, and it holds the one screen and cursor. |

Every Maestro flow carries `tags` in its header: one tag per area, named after the feature (`onboarding`, `pairing`, `composer`, …), and `smoke` on the few flows that guard the core path. The agent picks the area tags from the diff; a change it can't map to an area runs `smoke` only and says so in its report.

## Writing and changing tests

- A changed flow that Playwright or Maestro can express gets its spec or flow in the same change. Each changed flow names the tool that covers it.
- Add or update only the tests for the flows this change touches. Leave every other test alone.
- Editing an existing test needs a reason that matches the behavior the change says it changes. List each such edit and its reason in the e2e report. When no such reason exists, the failing test found a regression: fix the code, not the test.
- Computer Use never runs two tasks at once. Playwright and the one Maestro run go beside it.
- Report every check that didn't run and why.

## Decisions

- **2026-11-03: no full Maestro suite for now.** Every `after-change` runs the whole Playwright suite but only the `smoke` and changed-area Maestro flows; no other step (release, nightly) runs every Maestro flow yet. The cost is that a phone regression outside the tagged areas can go unnoticed until someone walks that flow. Decide again with the maintainer before adding a full run, for example at the `dev` → `main` release once the suite has grown.

## Why these tools

- Playwright's Electron launch (`_electron`) is documented as experimental, and launching the app directly bypasses the sandbox. Attaching over the DevTools protocol uses the sandbox as `npm run dev` starts it, and matches where Playwright's Electron support is heading.
- Maestro runs one YAML flow on both phones, which fits the rule that every iPhone change lands on Android and is checked on both. XCUITest stays available for iOS-only checks Maestro can't express.
- Computer Use is slower and holds the one screen and cursor, so it covers only what no in-app driver can reach.
