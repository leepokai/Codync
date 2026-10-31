# File structure and naming

This is the map of the current repository. Paths below are repository-relative. Start with the [architecture overview](overview.md) for runtime relationships.

## Top-level layout

```text
Codync/
├── host/                      # Rust daemon and terminal client
│   ├── src/
│   │   ├── main.rs            # CLI entry, shared helpers (LockExt, http)
│   │   ├── *_cli.rs           # Subcommands with their own module: cloud, network, send (+ send_cli/: event feed, --wait)
│   │   ├── hub.rs, store/     # Shared state + event fan-out / SQLite persistence (schema, model, bots, entries, devices)
│   │   ├── service/           # Data dir, launchd/systemd/Windows install, keep-awake
│   │   ├── usage/, mcp.rs     # usage/: plan limits from each agent; mcp.rs: built-in stdio MCP servers; mcp/remote.rs: remote connector proxy
│   │   ├── screen/            # Remote screen: helper protocol, helper socket, phone viewers, bot computer use (cua-driver)
│   │   ├── api/               # Dispatch (mod.rs), HTTP/SSE routes (http.rs, events.rs), method groups (bots.rs, host.rs, marketplace.rs, remote.rs), caller permissions (devices.rs)
│   │   ├── agent/             # ACP client, harness discovery, registry, sign-in, setup PTYs, bot actor, new bot defaults (new_bot.rs)
│   │   ├── chat/              # Groups, bot-to-bot requests, prompt snapshots, memory
│   │   ├── remote/            # Identity, wire crypto, E2E channel, cloud, relay socket, push
│   │   ├── market/            # Marketplace (registry.rs, install.rs, skills.rs), Composio, MCP OAuth
│   │   └── tui/               # Terminal client (app/: state and input; view/: drawing; manage/ + sheets.rs: settings; connections.rs: secure connection requests)
│   └── tests/                 # Host integration tests and scripted ACP agents
├── apps/
│   ├── project.yml            # XcodeGen source of truth for every Apple target
│   ├── Codync.xcodeproj/      # Generated Xcode project
│   ├── shared/Config/         # {dev,main}.plist: environment for the iPhone, Android and desktop apps
│   ├── ios/                   # App/, Views/, Resources/, Widgets/, NotificationService/, WhatsNew.md (App Store notes)
│   │   └── Kit/               # The iPhone app's Swift package
│   │       ├── Sources/
│   │       │   ├── CodyncKit/  # Wire models, transports, design; widget-safe
│   │       │   │   ├── Client/
│   │       │   │   ├── Models/
│   │       │   │   ├── Design/
│   │       │   │   └── Resources/
│   │       │   └── CodyncUI/   # Stores, screens and custom controls
│   │       │       ├── Store/
│   │       │       ├── Bots/
│   │       │       ├── Thread/
│   │       │       ├── Call/
│   │       │       ├── Screen/
│   │       │       ├── Marketplace/
│   │       │       ├── Routines/
│   │       │       ├── Usage/
│   │       │       └── Resources/
│   │       └── Tests/         # CodyncKitTests and CodyncUITests
│   ├── desktop/               # Electron app for macOS, Linux and Windows (see desktop-app.md)
│   │   ├── src/main/          # Host service, tray, account, updates, SSH, screen, speech
│   │   ├── src/preload/       # window.codync bridge
│   │   ├── src/renderer/      # React UI: store/, components/, views/
│   │   └── native/            # codync-speech (macOS on-device speech helper)
│   ├── android/               # Android phone app (Kotlin, Compose), ported from the iPhone app; WhatsNew.md (Google Play notes)
│   │   ├── core/              # JVM models, crypto, pairing, encrypted channel, host and cloud clients
│   │   ├── design/            # Codync's components: palette, type, motion, glass, sheets, menus, dialogs, controls, symbols
│   │   ├── app/               # Screens, the stores (AccountStore, one AppStore per computer), notifications, Live updates, Glance widgets
│   │   └── tools/             # symbols.py (SF Symbol stand-ins), launcher_icon.py, agent_icons.py
│   ├── screen-macos/          # macOS screen helper (Xcode Screen target)
│   ├── screen-macos-tests/    # Swift package testing the helper's socket lifecycle and candidate rules
│   └── screen-linux/          # Linux portal/GStreamer screen helper
├── cloud/                     # Account API + encrypted relay Worker and Durable Object
│   ├── src/
│   ├── migrations/            # D1 migrations
│   └── test/                  # workerd tests; e2e/ runs against a real test host
├── private-relay/             # Single-process relay (the cloud WebSocket subset) for a person's own computers
├── relay/                     # Separate APNs push Worker; src/ and test/
├── discord/                   # Discord GitHub-role Worker (discord.codync.dev)
├── docs/                      # Architecture, features, design, guides, reference, plans, investigations, archive
├── tools/                     # Release, App Store and Google Play submission (store_release.py: what both share), font scripts
├── packaging/                 # Distribution templates
├── marketing/app-store-screenshots/ # App Store screenshot editor (bun dev; Export bundle → out/)
├── web/                       # Website (Next.js static export)
└── .github/workflows/         # CI, signing and release automation
```

`build/`, `apps/ios/Kit/.build/`, `apps/desktop/out/`, `apps/desktop/dist/`, Cargo `target/`, `node_modules/` and Wrangler local state are generated working data. They are not source modules and should not become documentation locations. Edit `apps/project.yml` and run `xcodegen generate --spec apps/project.yml`; never hand-edit `project.pbxproj`.

## Targets

- `apps/desktop/` — desktop app for macOS, Linux and Windows (Electron; `npm run dev`, packaged by `.github/workflows/release-desktop.yml`)
- `iOS` (`apps/ios/`) — iOS app
- `Screen` (`apps/screen-macos/`) — Codync Screen: capture, input and WebRTC for Remote screen, embedded in the macOS desktop app (`Contents/Library/LoginItems`); starts the computer-use driver it carries in `Contents/Helpers`
- `apps/screen-macos-tests/` — native helper tests: socket lifecycle and candidate rules (`swift test --package-path apps/screen-macos-tests --scratch-path build/dd/screen-macos-tests`)
- `apps/screen-linux/` — `codync-screen` (Rust, GStreamer + xdg portals), the Linux Remote screen helper
- `Widgets` (`apps/ios/Widgets/`) — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `apps/android/` — Android app (`com.pokai.Codync.android`, Dev `.dev`; Gradle modules `app`, `core`, `design`; CI `.github/workflows/android.yml`)
- `CodyncKit` (`apps/ios/Kit/`) — the iPhone app's Swift package: `CodyncKit` + `CodyncUI` libraries (tests: `xcodebuild test -scheme CodyncKit-Package` on an iOS simulator)

## Where to make a change

| Responsibility | Source entry points |
|---|---|
| CLI and background service | `host/src/main.rs`, `service/`; subcommands `cloud_cli.rs`, `network_cli.rs`, `send_cli.rs` (+ `send_cli/events.rs`, `wait.rs`) |
| API, caller permissions, event ordering | `host/src/api/`, `hub.rs` |
| SQLite transcript, bots, lanes and sessions | `host/src/store/` (`entries.rs`, `bots.rs`, `model.rs`, `devices.rs`, `schema.rs`) |
| Agent process, ACP, queue and session lifecycle | `host/src/agent/bot/` (`launch.rs`, `queue.rs`, `session.rs`, `turn.rs`, `updates.rs`), `acp.rs` (+ `acp/codebuddy.rs`, CodeBuddy's ACP extensions), `process.rs` (process-tree ownership; `process_windows.rs` on Windows) |
| Compaction timing and rollover | `host/src/agent/bot/compaction.rs`, the handoff card in `host/src/chat/handoff.rs`; the rules in `codync-mem`'s `compaction.rs` |
| Group room turns / bot-to-bot requests | `host/src/chat/group.rs` / `team.rs` |
| Voice calls (the call, the voice's instructions and tools, transcripts, call lines) | `host/src/chat/calls/` (`mod.rs`, `context.rs`, `project.rs` the git state, `tools.rs`, `announce.rs` what the voice hears while the call lasts, `speech.rs` text as a voice says it); the user's messages it relays go through `host/src/chat/messages.rs` like typed ones; clients' `CallLink` (iPhone `CodyncUI/Call/CallLink.swift`, desktop `views/call/call-link.ts`, Android `core/Calls.kt` + `HostCallLink.kt`) and call rows (`CallRows`); the call on the iPhone's Lock Screen (`CodyncUI/Call/CallActivity.swift`, the Live Activity) and Android's call notification (`VoiceService.kt`) |
| Team management (bots creating and changing bots) | `host/src/chat/manage.rs`, its approval card (`Cmd::Approve`) in `agent/bot/updates.rs` |
| codync-network (your computers' network) | `host/crates/network` (the crate: roster, letters, reach, budgets, `rules.rs`; `node/endpoint.rs` for where a computer is), `host/src/network/` (one node per network in `mod.rs`, the saved networks in `saved.rs`, its bots' address names in `handles.rs`, the `network*` methods, `networkView` and the `network` event in `api.rs`), `host/src/network_cli.rs`; across-computer bot requests in `host/src/chat/team/remote.rs`; desktop `apps/desktop/src/renderer/views/settings/NetworkSection.tsx`, `NetworkCard.tsx`, `NetworkSheets.tsx`, `RuleSheet.tsx` and the bot editor's `views/bots/NetworkSwitches.tsx` (+ `store/network.ts`, `lib/network-copy.ts`, `src/shared/network.ts`); iPhone `apps/ios/Kit/Sources/CodyncUI/NetworkSheet.swift` (+ `CodyncKit/Models/Network.swift`); Android `NetworkSheet.kt`, `AppStoreNetwork.kt` (+ `core/Network.kt`); terminal `host/src/tui/manage/network.rs`, `network_setup.rs`, `network_rules.rs` and `view/network.rs`, `view/network_rules.rs` |
| Prompt snapshots and memory | `host/src/chat/context.rs`, `memory/` (`api.rs` the apps' read-only view, `tools.rs` and `file_tools.rs` the MCP tools, `files.rs` the folders and scan, `turns.rs` which bot's turn a found change is credited to, `file_api.rs`, `dream.rs`, `import.rs`); the rules in `host/crates/memory` (`codync-mem`: files and facts in `files/`, dreaming in `dream/`, tombstones in `store.rs`, the old entries in `legacy.rs`); harnesses' own memory turned off in `host/src/agent/own_memory.rs` |
| Identity, encryption and direct channel | `host/src/remote/identity.rs`, `crypto.rs`, `channel.rs` |
| Host cloud state and relay connection | `host/src/remote/cloud.rs`, `cloud_url.rs`, `relay.rs` |
| Agent discovery, sign-in, setup terminal, Node.js for npm agents | `host/src/agent/backends.rs` (+ `backends/catalog.rs` the known harnesses, `backends/sign_in.rs`, `backends/search_path.rs`, `backends/workbuddy.rs` the CLI inside the WorkBuddy app), `registry.rs`, `auth.rs` (+ `auth/agent.rs` starting an agent and its own sign-in, `auth/models.rs`), `term.rs`, `node.rs` |
| Fast mode (session config option) | `host/src/agent/config.rs`, `agent/bot/fast_mode.rs` |
| Marketplace, Composio, connector OAuth | `host/src/market/` |
| Skills a bot writes (`save_skill`) | `host/src/chat/skills.rs`, stored by `host/src/market/skills.rs` |
| Artifacts (`publish_artifact`) and a chat's Library | `host/src/chat/artifacts.rs`, `library.rs`; desktop `apps/desktop/src/renderer/views/library/` (+ `src/main/artifacts.ts`); iPhone `apps/ios/Kit/Sources/CodyncUI/Artifacts/` (+ `Store/ArtifactCache.swift`, `BotStore+Artifacts.swift`); Android `ArtifactCardView.kt`, `ArtifactViewer.kt`, `ArtifactWebView.kt`, `LibraryView.kt`, `AppStoreArtifacts.kt` (+ `core/Artifact.kt`, `core/Library.kt`); terminal `host/src/tui/net/artifacts.rs`, `manage/library.rs` |
| Screen bridge and built-in MCP tools | `host/src/screen/`, `mcp.rs` |
| Swift transport and cloud API | `apps/ios/Kit/Sources/CodyncKit/Client/` |
| Account aggregation / one host mirror | `apps/ios/Kit/Sources/CodyncUI/Store/AccountStore.swift` / `BotStore.swift` (+ `BotStore+Connection`, `+Sync`, `+Sending`, `+Actions`, `+Plugins`, `+Cache`, `+Shortcuts`) |
| iPhone chat, replies, composer and trace | `apps/ios/Kit/Sources/CodyncUI/Thread/` |
| Reusable iPhone UI chrome | `apps/ios/Kit/Sources/CodyncUI/Chrome.swift`, `Controls.swift`, `MenuItem.swift` (system menus: `MenuItem`, `.contextActions`, `DropdownMenu`), `Platform.swift` |
| iOS navigation, pairing, account, settings, computer management | `apps/ios/Views/RootView.swift`, `BotListView.swift`, `PairingView.swift`, `AccountSwitcherView.swift`, `SettingsView.swift`, `ComputersView.swift` |
| Apple account sessions / public environment config | `apps/ios/App/AccountSession.swift`, `apps/shared/Config/` |
| Desktop local host / SSH lifecycle | `apps/desktop/src/main/host-controller.ts`, `ssh.ts` |
| Desktop store, chat and controls | `apps/desktop/src/renderer/store/bot-store.ts` (actions; state in `bot-mirror.ts`, events stream in `bot-sync.ts`), `views/thread/`, `components/` |
| Cloud routes / authentication / relay | `cloud/src/index.ts`, `api.ts`, `routes/`, `auth.ts`, `relay.ts` |
| Cloud assistants (the MCP connector at mcp.codync.dev) | `cloud/src/routes/assistant.ts` (MCP, OAuth metadata, routing to computers), `host-calls.ts` (the relay DO's calls to its host); `host/src/chat/team/assistant.rs` (the setting, the tools, the replies); the switch in desktop `apps/desktop/src/renderer/views/settings/ComputersView.tsx` (+ `store/cloud.ts`) and terminal `host/src/tui/manage/network.rs`; shown read-only in the iPhone and Android network sheets |
| Push encryption / APNs delivery / decryption | `host/src/remote/push.rs`, `relay/src/`, `apps/ios/NotificationService/` |
| Widget and activity rendering | `apps/ios/Kit/Sources/CodyncKit/Design/`, `apps/ios/Widgets/` |
| Android screens (one file per iOS view: `BotListView.kt`, `ChatScreen.kt`, `BotDetailsView.kt` (+ `BotSettingsForm.kt`, `BotTemplateView.kt`), `PairingView.kt`, `SettingsView.kt`, `ComputersView.kt`, …) | `apps/android/app/src/main/kotlin/com/codync/android/` |
| Android components and glass | `apps/android/design/` (`Controls.kt`, `Chrome.kt`, `Sheet.kt`, `Menus.kt`, `Dialog.kt`, `Glass.kt`) |
| Android transport, pairing and wire models | `apps/android/core/` (`EncryptedChannel.kt`, `HostConnector.kt`, `Pairing.kt`, `Compatibility.kt`) |
| Android widgets, Live updates, push | `apps/android/app/` (`CodyncWidgets.kt`, `WidgetCards.kt`, `LiveUpdateGallery.kt`, `PushService.kt`) |

## Dependency rules

- `CodyncKit` owns serializable models, clients and rendering primitives shared with widgets. It must not import `CodyncUI` or ClerkKit.
- `CodyncUI` owns the iPhone app's observable stores and screens. App-specific lifecycle, OAuth configuration and device hooks belong in `apps/`.
- Each `BotStore` (Swift, and its TypeScript port in the desktop app) talks to one computer. `AccountStore` aggregates stores and routes by `BotReference`; bare bot IDs are not globally unique.
- The host owns routing, reply quotes, permissions and group turn scheduling. Clients render these results; they do not reimplement host policy.
- Android: `core` is plain JVM (no Android or Compose imports) and `design` holds no I/O; `app` uses both. Like the other clients it renders host state and sends intents, never product logic.
- `cloud/` transports encrypted chat traffic and manages account metadata. `relay/` delivers APNs pushes. The two Workers have separate configuration and tests. `private-relay/` is a single-process subset of the cloud relay protocol for a person's own computers. It is not the APNs worker.

## Names

| Term | Meaning |
|---|---|
| bot | What the user builds and talks to: one agent combined with a profile (name, avatar, instructions), working folder, memory, skills, routines and connectors. Never call a bot an agent. A group is also a roster entry with `kind: group` |
| agent / backend | A coding harness Codync composes into bots (Claude Code, Codex, …) and its ACP adapter |
| `agent@computer` | A codync-network address: whoever can take a letter there, whether a bot, a bare agent session (Claude Code, Codex) or a person at a terminal |
| chat | Main, persistent conversation with a bot or group |
| reply | A user message answering an earlier message of its chat, with that message's quote |
| group | Chat with several bots and the user; no agent process of its own |
| lane | Destination of a turn: a bot's own chat or a group's |
| session | Harness-owned ACP model context; distinct from stored chat history |
| trace | Full conversation, including intermediate/tool events |
| computer / host | User-facing paired environment / the daemon that serves it |
| cloud / relay | Account + E2E service / context-dependent transport or APNs Worker; always name the directory when ambiguous |

Use lowercase repository folders; PascalCase folders inside Swift targets. Swift files follow their main type (`UpperCamelCase.swift`), Rust uses `snake_case.rs` (a folder module is `name/mod.rs`), TypeScript uses `kebab-case.ts`, and React components use `PascalCase.tsx`. Prefer one main type per file; small related view siblings can share a plural file such as `ChatRows.swift`.

Role suffixes: `View`, `Row`, `Card`, `Window`, `Store`, `Controller`; button styles describe the effect (`PressScale`). Keep feature vocabulary aligned across Swift, the desktop app, Rust and the terminal client.

UI-specific rules, including the native iPhone toolbar exception and the desktop controls, live in [UI conventions](../design/ui-conventions.md).
