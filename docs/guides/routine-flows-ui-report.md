# Routine flows, screens: report

Branch `feat/routine-flows-ui` (from `feat/routine-flows-host`). Feature: [routine flows](../features/routine-flows.md).
Host report: [routine-flows-host-report.md](routine-flows-host-report.md).

The code was first written without building (at the user's request). The gates were then run for the first time; they found
five small compile, test and lint errors, all fixed (commits `3bfb6528`, `d1b54ec8`). Statements about how the screens
look and feel are still design statements: **no screen has been seen**.

## Scope

| Client | State |
|---|---|
| Desktop (Electron/React) | Type-checks; model tests pass; not seen running |
| Android (Compose) | Compiles, lint clean, unit tests pass; not seen running |
| Terminal | Done in the host branch; vocabulary checked against these screens (same statuses and words) |
| iPhone / Mac native | **Not done, by instruction** (no iOS work). CLAUDE.md asks for every client in the same change: this is a deliberate gap, and the iPhone has no Flow screen yet |

## Decisions

- **Desktop canvas: hand-written, no library.** Openstark's board is hand-written too (no graph library in its dependencies). Needs are small
  (dozens of nodes, bezier edges, pan/zoom/drag), and a library such as React Flow would add a dependency and its own
  stylesheet and inline-style model, against `desktop-styles.md`. The only inline styles are computed transforms.
- **Model apart from the views**: `lib/routine-flow.ts` (desktop) and `core/RoutineFlow.kt` (Android) hold types, automatic
  columns, edges, the list rows, the connect pre-check and the view math, with tests, so the views stay without logic. Both mirror the host's rules.
- **Events**: the host's `routineFlow` event carries no data; each client keeps the newest rev per bot (`flowRev` / `flowRevs`) and a canvas on
  screen loads again when it moves. Desktop's parser treats a malformed one as undecodable (the usual rewind); Android ignores it.
- **Connecting**: desktop by dragging from a bot's dot or from the inspector's menu; Android from the bot's detail sheet (explicit action).
  When several routines of the canvas allow the same link, a menu asks which routine.
- **Accessibility**: nodes are focusable buttons (arrow keys nudge a selected node on desktop); a list view with the same selection and
  inspector is the keyboard / TalkBack equivalent; Android defaults to it with TalkBack on or under 600 dp.
- **Link handles on Android**: edges are hard to hit with a finger, so each link has a small handle at its middle.

## Differences from Openstark

No agent chat panel, no "new agent" card on the canvas, no webhook configuration on the canvas (routines keep their own editor),
no auto-layout beyond columns by depth. Terms are Codync's (bot). The node card follows Openstark's content (avatar, name, model,
status, "Not connected" / "No runs yet") in this app's tokens. No Openstark file was copied.

## Gates

| Gate | Result |
|---|---|
| `npm run typecheck`, `npm test` (desktop) | clean; 395 passed, 0 failed |
| `./gradlew assembleDebug testDebugUnitTest :core:test lintDebug --no-daemon -Dorg.gradle.jvmargs=-Xmx2g` | BUILD SUCCESSFUL (after fixing: a missing `contentOrNull` import, a `Dp` radius, a composable called in a plain lambda, and a lint error reading the screen width from `resources.configuration` instead of `LocalConfiguration`) |
| E2E on desktop (`npm run dev`) and Android (signed APK, adb) | **not run**; screenshots: none. Needs the Electron binary (dependencies were installed with `--ignore-scripts`), a build of the host of this branch, and the owner to accept the MIUI prompt for the APK |
| Host | unchanged by this branch |

## Known gaps to check on the first build

- Translations: new phrases are English until added to the shared language tables (under `apps/ios/Kit/.../Languages`, not touched).
- Desktop `package.json` test glob already covers `src/renderer/lib/*.test.ts`, where the model test lives.
- Android: `FlowCanvasView` mixes dp layout with a pixel-translated layer scaled by the view; drag deltas are divided by density only. Verify the feel on a device.
- The `BotAvatar`, `Palette.added`, `Type.system` and symbol names (`clock.arrow.circlepath`, `circle.dashed`, ...) were taken from nearby code; any missing symbol falls back as other icons do.
- Android `action("flow:<bot>")` runs one flow change at a time per bot: a move made while a load is in flight is dropped.
- The two host failures listed in the host report are still open.
