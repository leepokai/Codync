# File structure and naming

This is the map of the current repository. Paths below are repository-relative. Start with the [architecture overview](overview.md) for runtime relationships.

## Top-level layout

```text
Codync/
├── host/                      # Rust daemon and terminal client
│   ├── src/
│   │   ├── main.rs            # CLI entry, shared helpers (LockExt, http)
│   │   ├── hub.rs, store/     # Shared state + event fan-out / SQLite persistence (schema, model, bots, entries, devices)
│   │   ├── service.rs         # Data dir, launchd/systemd install, keep-awake
│   │   ├── usage.rs, mcp.rs
│   │   ├── screen/            # Remote screen: helper protocol, helper socket, phone viewers, bot computer use (cua-driver)
│   │   ├── api/               # Dispatch (mod.rs), HTTP/SSE routes (http.rs, events.rs), method groups (bots.rs, host.rs, marketplace.rs, remote.rs), caller permissions (devices.rs)
│   │   ├── agent/             # ACP client, harness discovery, registry, sign-in, setup PTYs, bot actor
│   │   ├── chat/              # Groups, bot-to-bot requests, prompt snapshots, memory
│   │   ├── remote/            # Identity, wire crypto, E2E channel, cloud, relay socket, push
│   │   ├── market/            # Marketplace (registry.rs, install.rs, skills.rs), Composio, MCP OAuth
│   │   └── tui/               # Terminal client (app/: state and input; view/: drawing; manage/ + sheets.rs: settings; connections.rs: secure connection requests)
│   └── tests/                 # Host integration tests and scripted ACP agents
├── apps/
│   ├── project.yml            # XcodeGen source of truth for every Apple target
│   ├── Codync.xcodeproj/      # Generated Xcode project
│   ├── shared/Config/         # {dev,main}.plist: environment for the iPhone and desktop apps
│   ├── ios/                   # App/, Views/, Resources/, Widgets/, NotificationService/
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
│   ├── screen-macos/          # macOS screen helper (Xcode Screen target)
│   └── screen-linux/          # Linux portal/GStreamer screen helper
├── cloud/                     # Account API + encrypted relay Worker and Durable Object
│   ├── src/
│   ├── migrations/            # D1 migrations
│   └── test/                  # workerd tests; e2e/ runs against a real test host
├── relay/                     # Separate APNs push Worker; src/ and test/
├── docs/                      # Architecture, guides, reference, features, design, archive
├── tools/                     # Widget rendering utilities
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
- `apps/screen-linux/` — `codync-screen` (Rust, GStreamer + xdg portals), the Linux Remote screen helper
- `Widgets` (`apps/ios/Widgets/`) — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `CodyncKit` (`apps/ios/Kit/`) — the iPhone app's Swift package: `CodyncKit` + `CodyncUI` libraries (tests: `xcodebuild test -scheme CodyncKit-Package` on an iOS simulator)

## Where to make a change

| Responsibility | Source entry points |
|---|---|
| CLI and background service | `host/src/main.rs`, `service.rs` |
| API, caller permissions, event ordering | `host/src/api/`, `hub.rs` |
| SQLite transcript, bots, lanes and sessions | `host/src/store/` (`entries.rs`, `bots.rs`, `model.rs`, `devices.rs`, `schema.rs`) |
| Agent process, ACP, queue and session lifecycle | `host/src/agent/bot/` (`queue.rs`, `session.rs`, `turn.rs`, `updates.rs`), `acp.rs` |
| Group room turns / bot-to-bot requests | `host/src/chat/group.rs` / `team.rs` |
| Prompt snapshots and memory keeper | `host/src/chat/context.rs`, `memory/` (`facts.rs`, `extract.rs`, `keeper.rs`, `search.rs`) |
| Identity, encryption and direct channel | `host/src/remote/identity.rs`, `crypto.rs`, `channel.rs` |
| Host cloud state and relay connection | `host/src/remote/cloud.rs`, `relay.rs` |
| Agent discovery, sign-in, setup terminal | `host/src/agent/backends.rs`, `registry.rs`, `auth.rs`, `term.rs` |
| Marketplace, Composio, connector OAuth | `host/src/market/` |
| Screen bridge and built-in MCP tools | `host/src/screen/`, `mcp.rs` |
| Swift transport and cloud API | `apps/ios/Kit/Sources/CodyncKit/Client/` |
| Account aggregation / one host mirror | `apps/ios/Kit/Sources/CodyncUI/Store/AccountStore.swift` / `BotStore.swift` (+ `BotStore+Connection`, `+Sync`, `+Sending`, `+Actions`, `+Plugins`, `+Cache`) |
| iPhone chat, replies, composer and trace | `apps/ios/Kit/Sources/CodyncUI/Thread/` |
| Reusable iPhone UI chrome | `apps/ios/Kit/Sources/CodyncUI/Chrome.swift`, `Controls.swift`, `Platform.swift` |
| iOS navigation, pairing, account settings | `apps/ios/Views/RootView.swift`, `BotListView.swift`, `PairingView.swift`, `AccountSwitcherView.swift`, `SettingsView.swift` |
| Apple account sessions / public environment config | `apps/ios/App/AccountSession.swift`, `apps/shared/Config/` |
| Desktop local host / SSH lifecycle | `apps/desktop/src/main/host-controller.ts`, `ssh.ts` |
| Desktop store, chat and controls | `apps/desktop/src/renderer/store/bot-store.ts` (actions; state in `bot-mirror.ts`, events stream in `bot-sync.ts`), `views/thread/`, `components/` |
| Cloud routes / authentication / relay | `cloud/src/index.ts`, `api.ts`, `routes/`, `auth.ts`, `relay.ts` |
| Push encryption / APNs delivery / decryption | `host/src/remote/push.rs`, `relay/src/`, `apps/ios/NotificationService/` |
| Widget and activity rendering | `apps/ios/Kit/Sources/CodyncKit/Design/`, `apps/ios/Widgets/` |

## Dependency rules

- `CodyncKit` owns serializable models, clients and rendering primitives shared with widgets. It must not import `CodyncUI` or ClerkKit.
- `CodyncUI` owns the iPhone app's observable stores and screens. App-specific lifecycle, OAuth configuration and device hooks belong in `apps/`.
- Each `BotStore` (Swift, and its TypeScript port in the desktop app) talks to one computer. `AccountStore` aggregates stores and routes by `BotReference`; bare bot IDs are not globally unique.
- The host owns routing, reply counts, permissions and group turn scheduling. Clients render these results; they do not reimplement host policy.
- `cloud/` transports encrypted chat traffic and manages account metadata. `relay/` delivers APNs pushes. The two Workers have separate configuration and tests.

## Names

| Term | Meaning |
|---|---|
| bot | Named agent persona; a group is also a roster entry with `kind: group` |
| agent / backend | Coding harness and its ACP adapter |
| chat | Main, persistent conversation with a bot or group |
| thread | Replies under a main-chat root message |
| group | Chat with several bots and the user; no agent process of its own |
| lane | `(chat, thread)` destination of a turn |
| session | Harness-owned ACP model context; distinct from stored chat history |
| trace | Full conversation, including intermediate/tool events |
| computer / host | User-facing paired environment / the daemon that serves it |
| cloud / relay | Account + E2E service / context-dependent transport or APNs Worker; always name the directory when ambiguous |

Use lowercase repository folders; PascalCase folders inside Swift targets. Swift files follow their main type (`UpperCamelCase.swift`), Rust uses `snake_case.rs` (a folder module is `name/mod.rs`), TypeScript uses `kebab-case.ts`, and React components use `PascalCase.tsx`. Prefer one main type per file; small related view siblings can share a plural file such as `ChatRows.swift`.

Role suffixes: `View`, `Row`, `Card`, `Window`, `Store`, `Controller`; button styles describe the effect (`PressScale`). Keep feature vocabulary aligned across Swift, the desktop app, Rust and the terminal client.

UI-specific rules, including the native iPhone toolbar exception and the desktop controls, live in [UI conventions](../design/ui-conventions.md).
