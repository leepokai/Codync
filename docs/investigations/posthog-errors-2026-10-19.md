# PostHog error triage — 2026-10-19

A pass over production errors and failed agent turns in PostHog (project "Codync App", 649427),
2026-10-12 to 2026-10-19, `env = main`. Each finding names the evidence, the root cause as far as
it could be established, the fix and what is still open. Version numbers are the first release
with the fix. Error tracking itself is described in [usage analytics](../features/analytics.md);
richer error detail is proposed in [error tracking detail](../plans/error-tracking-detail.md).

## Method

- **Issues**: error tracking issues ranked by users, then by first seen; sample `$exception`
  events per issue (`query-error-tracking-issue-events` with `onlyAppFrames: false`).
- **Who**: SQL over `$exception` grouped by issue, `process`, `app_version`, `$os`; persons checked
  against cohort 618410 (internal and test hosts). Every person below is a real user.
- **Failed turns**: `turn_finished` with `outcome = failed`, by agent, `error`, `rpc_code`, `source`,
  and per hour for the heaviest person.
- **Desktop renderer frames**: the renderer bundle is reproducible. `git archive v2.18.0
  apps/desktop`, link `node_modules`, `npx electron-vite build`: the same `index-<hash>.js`
  comes out, so a frame's line and column map to source.
- **iPhone frames** (`Codync + <offset>`, the offset into the binary's text segment; from 2.19.2
  PostHog symbolicates them, see [iPhone crash tracing](../guides/iphone-crash-tracing.md)): the dSYM of
  that version comes from its Xcode Cloud run ([iOS releases](../guides/development.md#ios-releases-xcode-cloud)).
  With the App Store Connect API: `GET /v1/ciProducts/<iOS product>/buildRuns?sort=-number` (the
  commit message names the version) → `/v1/ciBuildRuns/<id>/actions` → `/v1/ciBuildActions/<id>/artifacts`;
  the `ARCHIVE` artifact's `downloadUrl` (`/v1/ciArtifacts/<id>`) is a zipped `.xcarchive` with
  `dSYMs/Codync.app.dSYM`. Then `atos -o <dSYM>/Contents/Resources/DWARF/Codync -arch arm64
  -l 0x100000000 <0x100000000 + offset>`.

## Findings

| # | Signal | Users | Root cause | Fix |
|---|---|---|---|---|
| 1 | Host panic `JoinHandle polled after completion` (tokio `core.rs:427`), Linux and macOS | 3 | `remote/channel.rs` `run` awaited the writer task again after `serve` had already taken its end (a phone's transport closed first). Reproduced by a unit test. | 2.19.0 (`b0c2e8fe`) |
| 2 | Host panic in std `stdio.rs` `print_to`, macOS and Windows | 4 | tracing-subscriber's fmt layer `eprintln!`s when a log write fails; with stderr gone too (closed terminal, full disk) that panics. Inferred from the location and the source, not reproduced. | 2.19.0 (`b0c2e8fe`): `log_internal_errors(false)` |
| 3 | Desktop renderer `TypeError` in `MemoryCard` (2.18.0) | 1 | `view.dreams.length` (mapped from the rebuilt bundle): a 2.18 app talking to a host before 2.18, whose `memory` method still answered with the Engram listing. | 2.19.0 (`37689bd6`): desktop and iPhone show "Update Codync on your computer…" |
| 4 | Desktop renderer `NonError`, mechanism `onerror`, 48 events, mostly Windows | 5 | Error events that carry no error object (the ResizeObserver loop notice, cross-origin "Script error."). Nothing to act on. | 2.19.0 (`74e71a6b`): no longer reported |
| 5 | `GPUGone`, `UtilityGone`, `RendererGone` with reason `killed`, all in the same millisecond | 4 | The app being torn down (logout, shutdown, an update), not a crash. | 2.19.0 (`74e71a6b`): no longer reported |
| 6 | `GPUGone` reason `launch-failed`, Linux, up to 10 per launch | 4 | Chromium couldn't start a GPU process and fell back to software rendering; the same people went on to send 30+ messages. | 2.19.1 (`aa269562`): no longer reported |
| 7 | `turn_finished` failed, `agent_error` `-32603`, Claude: 333 on 2026-10-17 from one person, 307 of them under 2 s with `source = bot` | 1 heavy, a few each for others | Most likely a dead adapter session: once its query stream ends, Claude's adapter answers every later prompt on that session with "The Claude Agent session has ended" until the session is closed, and a busy bot never went idle long enough for the idle release. Established from the adapter's source and an e2e fixture; the person's actual messages are not recorded. | 2.19.1 (`430e87e4`, folded into `close_reloadable` by `c014f70f`): the session is closed after such a failure and loaded again on the next message. Failed turns now carry the adapter's own category as `agent_reason`. |
| 8 | iPhone 2.17.0 `EXC_BREAKPOINT` | 1 | `URLSessionSocket.ping()` (`HostConnector.swift:100`): `sendPing`'s handler resumed the checked continuation twice (the trap is in `libswift_Concurrency` right after the closure). | 2.19.1 (`49e116d4`): resumes once |
| 9 | iPhone 2.17.0 `EXC_CRASH` (SIGABRT from an Objective-C exception in AVFAudio) | 1 | `OnDeviceEngine.listen()` installed a microphone tap on a 0 Hz input format (the microphone briefly unavailable). | 2.19.1 (`49e116d4`): `MicTap.format(of:)` throws instead, used by every engine and dictation; the macOS speech helper got the same guard |
| 10 | iPhone 2.17.0 `EXC_BREAKPOINT` inside SwiftUI (iOS 27.2) | 1 | Unknown: the report kept only the 50 frames nearest the crash, all SwiftUI and AttributeGraph. | Open. 2.19.1 keeps 200 frames, so the next one shows the app's frames |
| 11 | Desktop renderer `NotFoundError`, mechanism `unhandledrejection`, Windows 2.17.5 and macOS 2.17.1 | 3 | Unknown: a `DOMException` has no stack, and its message isn't sent. Not the microphone (`microphone()` rewraps its errors). | Open |

Also seen, not ours: a fork (`realbot/Codync`, versions 2.18.7 and 2.18.9, above any real tag,
frames from `localhost:5173`) reports into the project with `env = main`. Its `deleteChat` and
`lastMessagePreview` errors are fork code. Ignore versions newer than the latest `v*` tag.

## Verification

- 1: `a_device_that_stops_reading_ends_the_channel_cleanly` reproduces the exact panic without the fix.
- 3: desktop sandbox against a 2.17.5 debug host (Memory → Dreams shows the update message) and against the fixed host (normal).
- 7: `a_session_that_ended_in_the_agent_is_closed_and_loaded_again` (`tests/fixtures/ended_session_agent.py` mimics the adapter) fails without the fix; a real Claude bot still answers through the sandbox.
- 8, 9: iPhone simulator paired with a sandbox host: chat, a voice call, and killing the host under a connected phone, without a crash. The 0 Hz microphone case itself couldn't be produced on the simulator.

## Open

- 10 and 11 need more detail than the reports carry; see [error tracking detail](../plans/error-tracking-detail.md).
  From 2.19.2 PostHog symbolicates iPhone crashes itself and every error carries its session.
- 7: watch `agent_reason` and `rpc_code` on failed Claude turns after 2.19.1; bursts that remain with `rate_limit` or `overloaded` are the provider, not Codync.
