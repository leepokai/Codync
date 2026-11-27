# Test stack

How the end-to-end (e2e) checks drive the apps: Playwright for the desktop app, Maestro for the iPhone and Android apps, and Computer Use only for the native steps neither can reach. Unit and component tests stay as they are (`npm run typecheck && npm test`, `cargo test`, the Swift tests in `apps/ios/Kit`, the Gradle checks).

## Headless by default

An e2e run never takes the screen, the Dock, the keyboard or the pointer from whoever is using the computer. Every tool runs without a window unless the maintainer asks, in that message, to watch it ("headed"); an agent never chooses headed by itself, not to debug a failure either (it reads the trace and screenshots instead).

| What | Headless (the default) | Headed, only when asked |
| --- | --- | --- |
| Desktop app | `npm run dev -- --cdp`: the app starts with `CODYNC_HEADLESS=1`, so no window is ever shown, it has no Dock icon and it never becomes the front app. The pages render and run as usual and Playwright drives them over the DevTools protocol. | `npm run dev -- --cdp --headed` |
| Playwright | It attaches to that app and opens no browser of its own. | The same |
| iPhone simulator | `xcrun simctl boot <udid>`, driven by Maestro and `simctl`: a booted simulator has no window by itself. | Open the Simulator app on it |
| Android emulator | `emulator -avd codync_worktree -no-window -no-snapshot-save -no-audio -no-boot-anim`, driven by Maestro and `adb`. | The same command without `-no-window` |
| Computer Use | cua-driver in the background: it never brings an app to the front ([what it covers](#what-computer-use-covers)). | The same |

**How the desktop app does it** (`apps/desktop/src/main/headless.ts`, development builds only: a packaged app ignores the variable). Every window is shown through `present()`, which does nothing in a headless run, so the chat window, the pairing window and the island stay hidden; `activate()`, the one place that brings the app to the front, does nothing either; the Dock icon is hidden, which also keeps the app from becoming the front one at launch; a hidden chat window counts as looked at, so cloud computers stay connected; and every window takes its `webPreferences` from `webPreferences()`, which turns `backgroundThrottling` off, so a page whose window was never shown still draws, animates and runs its timers at full speed. A new window must be shown through `present()` too, or it would appear in a headless run: `e2e/smoke.spec.ts` asks System Events that the app has no window and isn't the front one (`dev.mjs` writes `headless` into the sandbox's `sandbox.json`, so the check skips on a headed run).

**What a headless run can't show.** Nothing that Playwright checks: the whole suite passes headless, the specs that pick app menu items through System Events (`e2e/find.spec.ts`) included. What needs a window on the screen is a native step no headless sandbox can run: how the chat window itself looks (its title bar, vibrancy, where macOS places it), the approval island, the pairing window's QR, and the windows the tray's items open (the tray icon and its menu are still there). When a change touches one, check it with Computer Use on the packaged `Codync Dev.app` (it ignores `CODYNC_HEADLESS`) or on a headed run the maintainer asked for; otherwise report the step as not run.

Screenshots and traces work the same headless: `page.screenshot()` and `trace: 'retain-on-failure'` read the page, not the screen.

## Host tests

`host/tests` drive real hosts against fake agents (the Python scripts in `tests/fixtures`). `tests/common` starts every test host with `CODYNC_AGENT_CHECKS=off`, as do `cloud/test/e2e/relay.e2e.ts` and `tools/check-connectors.py`, so it never runs the computer's agent CLIs for their sign-in and usage checks and never touches your logins. A test of those checks turns them back on (`tests/agent_checks_e2e.rs`). `tests/common` also sets `CODYNC_VAULT=folder`, so a test's credential key stays out of the system's credential store and goes with its data; `tools/check-connectors.py` keeps a real Keychain key on purpose.

Run them one at a time: never start a test binary several times in parallel, or several copies of the suite, not even to hunt a flake. Repeat it one run after another, a few times at most. On 2026-11-04 parallel repeats left about 200 `claude -p /usage` processes behind, using 55 GB on a 32 GB Mac, and froze it.

`tests/iphone_reconnect_e2e.rs` drives the iPhone's own store and channel (`HostRestartCatchUpTests.swift` in `apps/ios/Kit`, run with `tools/xcodebuild.sh` on a simulator) through `private-relay/` against a host it restarts, with 30 chats and five days of history, and checks the phone stays on the relay, its catch-up after each restart holds only what is new, and no relayed device link overflows. It needs macOS, Xcode and Node, so it runs only when asked, after `npm ci` in `private-relay`: `CODYNC_IOS_SIMULATOR=<udid> cargo test --test iphone_reconnect_e2e -- --ignored`.

## Real agents

Everyday tests use the fake agents, which only answer the way we expect. Every agent the [upstream agents](#upstream-agents) check covers is tested through it (`node check.mjs … --all`, aimock in place of the model, no logins); only Cursor, which aimock can't stand in for, still runs as a real agent, one short turn on its cheapest model (it uses your own plan), when:

1. The registry version of `claude-acp` or `codex-acp` moves ([upstream agent bugs](../upstream-agents/README.md)).
2. A change touches how the host talks to an agent: ACP methods, sessions, turns.
3. Before a release.

Each check ends by stopping the host: no agent process may outlive it, except a daemon an agent detaches on purpose and shares with its other clients, which its [upstream page](../upstream-agents/README.md) lists (Hermes's `gateway run --idle-exit`).

## Upstream agents

Every computer running Codync takes an adapter's new [ACP registry](https://agentclientprotocol.com/registry) version within a day (`registry.rs` refreshes it daily), so a daily CI job (`.github/workflows/agent-upstream.yml`) runs a scripted turn of each agent in [`tools/agent-upstream/accepted.json`](../../tools/agent-upstream/accepted.json): each registry adapter whose version moved, and every day each agent Codync runs from its own CLI (`registry: false`). Each runs in a real host built from the branch, with the agent's latest CLI, a new bot on that agent in its asking mode, and `codync-host send --wait`. The adapter is the real one the host downloads; only the model is a stand-in, an [aimock](https://www.npmjs.com/package/@copilotkit/aimock) server on the runner speaking the provider's own API (Anthropic Messages, OpenAI Responses and Chat Completions, Gemini), on an API key in a home of the check's own, so no login or token is used.

The turn (`scenario.mjs`) has the agent run `printf beta > notes.txt` through its own shell tool, and passes when the transcript holds the tool call, its permission card was answered, the file reads `beta`, the reply comes after the command's result reached the model, and no agent process outlives the stopped host. `agents.mjs` points each agent at the mock and makes its shell tool ask first where it wouldn't by itself (a pi extension, an OpenCode `permission`, a Hermes hook). It finds an adapter that no longer starts, opens a session, runs a tool, asks for approval or ends a turn; it stands in for a real-model turn on every agent it covers.

Covered: Claude Code, Codex, Copilot, pi, oh-my-pi, OpenCode, Grok Build, DeepSeek Harness, Hermes and Antigravity. Not covered: Cursor, whose CLI speaks Cursor's own protobuf RPC (`aiserver.v1`) to its backend, which aimock doesn't, and WorkBuddy, which ships only inside the macOS and Windows desktop app. Adding an agent: its setup in `agents.mjs`, its row in `accepted.json` (with the `mode` in which its shell tool asks) and its install line in the workflow (npm where the CLI ships there, else the install script in the host's catalog).

A failing scheduled run opens an issue labeled `agent-upstream` (later failures comment on the open one). Opening it starts the "Codync upstream agent fix" Claude routine (claude.ai, the maintainer's account, no connectors). The routine builds `dev` and runs the check again on the failing agents (`--only <backend>`, with the workflow's CLIs installed), and stops there if `dev` already passes. Otherwise it fixes the adapter, opens a PR into `dev` (its body fills the template's *Validation* section, as every PR must) and merges that PR itself when all three hold: the PR changes only agent code (`host/crates/agent`, `host/src/agent`, `host/tests`, `docs/upstream-agents`, `accepted.json`'s version), every check passes and Greptile scores it 4/5 or more. It never touches the check or CI. After three PRs for one adapter version (for a `registry: false` agent, one issue) it leaves the issue to a person. These limits live in the routine's prompt, so the `main` release gate's [real agents](#real-agents) check is what catches a wrong fix. A passing run on a new registry version says so in its summary: raise that version in `accepted.json` (`registry: false` rows have none). A pull request that changes the check, or the host's agent code (`host/crates/agent`, `host/src/agent`), runs every agent. To run it on this computer, with the CLIs of the agents it runs installed (the workflow's install step; a missing one fails its row; about eight minutes for all of them, one at a time; it starts each agent in a home of its own, never on your logins, without your shell's provider keys; `--only <backend>` runs one):

```sh
cd host && cargo build --bin codync-host
cd ../tools/agent-upstream && npm ci && node check.mjs ../../host/target/debug/codync-host --all
```

## Desktop: Playwright

Playwright attaches to the worktree's running sandbox over the DevTools protocol. It doesn't launch Electron itself: the sandbox already owns the host, data, sign-in and single-instance lock, and an app launched without them would open against the shared Codync Dev.

```sh
cd apps/desktop
npm run dev -- --cdp   # the worktree's sandbox, DevTools on its port + 10000
npm run e2e            # in a second shell: e2e/*.spec.ts against that sandbox
```

`e2e/sandbox.ts` finds the sandbox from its `sandbox.json` and attaches to its main window; `e2e/smoke.spec.ts` checks the app renders and runs against the sandbox's own host. A spec reaches the main process only through the `window.codync` bridge, as the renderer does. Keys sent over the DevTools protocol never reach the app menu's shortcuts (⌘F, ⌘G): `e2e/find.spec.ts` picks those menu items through System Events instead, so it runs on macOS only and needs Accessibility access for the terminal running Playwright. Every unpackaged sandbox app is a process named "Electron", so it picks the attached app by the process ID its DevTools protocol reports (`SystemInfo.getProcessInfo`), never by name. One worker per worktree, since one app runs per sandbox; worktrees run in parallel with each other.

### Stand-in agents

Some screens need an agent the host knows by name: an agent's sheet in Extensions, its sign-in terminal, its [login profiles](../features/login-profiles.md). A scripted agent on a custom command (the `echoBot` kind) can't reach them, and the real agents on the computer must not be signed in and out by a test: repeated automated sign-ins can get an account banned by its provider. So an e2e sandbox runs stand-ins of real agents' CLIs.

**How it works.** The host finds an agent by its CLI's name on `PATH`. `npm run dev -- --cdp` puts `host/tests/fixtures/bin` first on the sandbox host's `PATH` (`apps/desktop/tools/dev.mjs`), so a script there named like an agent's CLI is that agent for this host: listed as installed, signed in by the host's own sign-in command, started by the host's own ACP command. Nothing in the host knows about the stand-ins, so a spec on one runs the same code a real agent does. Only a `--cdp` sandbox has them: a sandbox started without the flag, Codync Dev and every released build see the computer's real agents only.

The one stand-in so far is `host/tests/fixtures/bin/devin`, for the Devin CLI: an agent whose account login profiles sign in in a terminal and keep the login as a file in the profile's folder. What each of its commands does, and how to try one by hand, is in [`host/tests/fixtures/bin/README.md`](../../host/tests/fixtures/bin/README.md).

**What runs on it.** `e2e/login-profiles.spec.ts`, in the whole suite every time: Add account opens the sign-in terminal and leaving it signs nothing in; the row's Sign in finishes the sign-in and the row reads Signed in; a bot on the login profile answers from it; the bot's settings move it back to the default login and its fallbacks follow; an API key login profile gets its keys from its sign-in card and its key form; and each login profile is removed once no bot uses it. A spec that needs a stand-in asks for it with `standInAgent` (`e2e/sandbox.ts`), which checks the agent's `path` in `hello` is the script and fails with the command to start the sandbox when it isn't.

**Phones.** A phone paired with a `--cdp` sandbox (as `after-change` pairs them) sees the stand-in Devin as installed too. The Maestro flow `apps/maestro/login-profiles.yaml` (tag `agents`) gives each phone an agent whose accounts sign in in a terminal: the iPhone the first in the host's list, which is a real agent on the computer (Claude Code when it is installed), and Android the last, which in such a sandbox is the stand-in Devin. The flow adds its login profiles through the host API and never opens a sign-in, so neither half signs an account in; the iPhone's half still runs on the real agent's CLI.

**Not on a stand-in.** `e2e/usage-logins.spec.ts` (the limits per login) runs the computer's real Claude Code, and skips itself without it: the host reads the default login's limits with `claude -p /usage`, and runs the CLI's own checks on an empty throwaway login profile, which the spec marks signed in through the host's store. It never opens a sign-in.

**Limits.** The stand-ins are Python scripts started by their `#!` line: they don't run on Windows, where `dev.mjs` leaves `PATH` alone and the spec is skipped. In a `--cdp` sandbox Devin shows as installed, and a real Devin on the computer is hidden behind the stand-in. Start the sandbox without `--cdp` to use the real one.

**Adding one.** The steps are in the same [README](../../host/tests/fixtures/bin/README.md#adding-a-stand-in): pick the catalog agent whose sign-in and login storage match the screen under test, name the script after its CLI, and add its row to the README's table.

### Real login tests

`e2e/login-profiles-real.spec.ts` is the one spec that opens a real agent's own sign-in: on Claude Code or Codex it adds an account, checks the sign-in terminal opens, leaves without signing in and removes the login profile. It is skipped unless it is asked for, so `npm run e2e` and `after-change` never run it:

```sh
cd apps/desktop
npm run dev -- --cdp                                          # the worktree's sandbox
CODYNC_E2E_REAL_LOGINS=1 npm run e2e -- login-profiles-real   # in a second shell
```

Run it only when the maintainer asks for a real login test. It needs Claude Code or Codex installed on the computer, and skips itself without.

## Phones: Maestro

One flow in `apps/maestro/` serves both phones: it launches the iPhone or Android Dev app by platform (`runFlow` with `when: platform`) and asserts the same copy, since the apps match screen for screen. Run every phone in one Maestro process, with the tags [the check needs](#what-each-e2e-check-runs):

```sh
maestro --device "<simulator udid>,<adb serial>" test --shard-all 2 --include-tags smoke,<area> apps/maestro
```

The devices are the worktree's own simulator `Codync <sandbox name>` and the shared `codync_worktree` emulator, under its lock ([worktree sandboxes](dev-app.md#worktree-sandboxes)). Install the worktree's builds first: `tools/xcodebuild.sh … -scheme "iOS Dev" -configuration DevDebug -derivedDataPath build/dd` for the simulator, `./gradlew :app:assembleDebug` for the emulator.

A flow past the welcome screen (`agent-logos.yaml`) needs the phone signed in with `npm run phone-account` and keeps the app's state. `welcome.yaml` (`smoke`) clears it. That signs the Android app out, but the iPhone's sign-in stays in the simulator's Keychain, so run `xcrun simctl keychain <device> reset` before `smoke` on a signed-in simulator. `sign-in-cancel.yaml` (`sign-in`) clears it the same way and needs the phone online, since it opens Google's page and leaves it. `signed-out-pairing.yaml` needs a sandbox in no account and the phone signed out, and takes the pairing link as `PAIRING_URL`. Run a flow past the welcome screen in a Maestro run of its own, after the sign-in. Signed in isn't past first-run setup: `agent-logos.yaml` skips the choice page when it's there. The `computer-choice` flows need setup unfinished (fresh app data) and each finishes or changes it, so each gets its own run with its own start: `computer-choice` and `computer-choice-skip` with a computer in the account (`npm run dev -- --cloud`, or the sandbox's own local cloud on a branch that changes `cloud/`), `computer-choice-empty` without one.

Three things that look like failures but aren't the app's:

- A tag no flow carries yet makes Maestro stop with "Include / Exclude tags did not match any Flows" and exit 1. Leave out area tags that have no flows.
- Right after a cold boot the emulator can cover the app with "Pixel Launcher isn't responding", and the flow fails on its first assertion. Each run keeps a screenshot of the failure in `~/.maestro/tests/<run>/`: when a system dialog covers the app, clear it with `adb -s <serial> shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS` and run again. It happened twice in a row on 2026-11-03 until the dialog was cleared.
- On iOS, `inputText` types as a hardware keyboard and hides the simulator's keyboard until the next touch brings it back, which lifts the composer: a `tapOn: "Send"` right after the typing can land on the keyboard and send nothing (`reply-part.yaml` failed this way about half the time). Touch the field first and `waitForAnimationToEnd`, as `reply-part.yaml` does, or wait for a keyboard key, as `compaction.yaml` does.

**One Maestro process per Mac.** Two at once break each other: with Maestro 2.1.0, an iPhone run beside an Android run made the Android driver refuse connections on port 7001 in three of three tries, and two runs starting in the same second removed each other's log folder in `~/Library/Logs/maestro`. Shard inside one run instead. Across worktrees, take the lock folder `/private/tmp/claude-$(id -u)/maestro.lock` the way the emulator lock works: `mkdir` it (it fails while another run holds it), write your worktree's path into `owner`, `rm -r` it when done; one older than an hour, or whose worktree is gone, is stale.

## What Computer Use covers

Computer Use (cua-driver, in the background: never bring an app to the front) checks only these steps. Tag them `@native` in the change's e2e list. Everything else uses Playwright or Maestro. If cua-driver fails, stop and ask the user to reconnect it with `/mcp`.

- **Desktop**: the tray or menu bar icon and its menu; the approval island's global shortcut (not in a headless sandbox, like every step that needs a window: [what a headless run can't show](#headless-by-default)); system notification banners; macOS privacy prompts (screen recording, notifications); Clerk sign-in in the system browser and the `codync-dev://` callback back into the app; anything that needs the packaged `Codync Dev.app` (Codync Screen, computer use, `SMAppService`).
- **Phones**: how system alerts and notification banners look; the desktop QR pairing (Computer Use reads the QR on the Mac, Maestro finishes on the phone; the QR needs the packaged app or a headed run); widgets, Live Activity and the Dynamic Island.

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
