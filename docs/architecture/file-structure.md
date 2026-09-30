# File structure and naming

This is the map of the current repository. Paths below are repository-relative. Start with the [architecture overview](overview.md) for runtime relationships.

## Top-level layout

```text
Codync/
├── host/                      # Rust daemon and terminal client
│   ├── src/
│   │   ├── main.rs            # CLI entry, shared helpers (LockExt, http)
│   │   ├── hub.rs, store.rs   # Shared state + event fan-out / SQLite persistence
│   │   ├── service.rs         # Data dir, launchd/systemd install, keep-awake
│   │   ├── usage.rs, screen.rs, mcp.rs
│   │   ├── api/               # HTTP/SSE dispatch, caller permissions (devices.rs)
│   │   ├── agent/             # ACP client, harness discovery, registry, sign-in, setup PTYs, bot actor
│   │   ├── chat/              # Groups, bot-to-bot requests, prompt snapshots, memory
│   │   ├── remote/            # Identity, wire crypto, E2E channel, cloud, relay socket, push
│   │   ├── market/            # Marketplace, Composio, MCP OAuth
│   │   └── tui/               # Terminal client (manage.rs + sheets.rs: settings; connections.rs: secure connection requests)
│   └── tests/                 # Host integration tests and scripted ACP agents
├── kit/                       # Shared Swift package
│   ├── Sources/
│   │   ├── CodyncKit/          # Wire models, transports, design; widget-safe
│   │   │   ├── Client/
│   │   │   ├── Models/
│   │   │   ├── Design/
│   │   │   └── Resources/
│   │   └── CodyncUI/           # Stores, shared screens and custom controls
│   │       ├── Store/
│   │       ├── Bots/
│   │       ├── Thread/
│   │       ├── Call/
│   │       ├── Screen/
│   │       ├── Marketplace/
│   │       ├── Usage/
│   │       └── Resources/
│   └── Tests/                 # CodyncKitTests and CodyncUITests
├── apps/
│   ├── project.yml            # XcodeGen source of truth for every Apple target
│   ├── Codync.xcodeproj/      # Generated Xcode project
│   ├── shared/                # AccountSession, ComputerStatus, Config/{dev,main}.plist
│   ├── ios/                   # App/, Views/, Resources/, Widgets/, NotificationService/
│   ├── macos/                 # App/, Views/, Resources/, LaunchAgents/
│   ├── linux/                 # GTK 4/libadwaita desktop client
│   ├── screen-macos/          # macOS screen helper
│   └── screen-linux/          # Linux portal/GStreamer screen helper
├── cloud/                     # Account API + encrypted relay Worker and Durable Object
│   ├── src/
│   ├── migrations/            # D1 migrations
│   └── test/                  # workerd tests; e2e/ runs against a real test host
├── relay/                     # Separate APNs push Worker; src/ and test/
├── docs/                      # Architecture, guides, reference, features, design, archive
├── tools/                     # Widget rendering utilities
├── packaging/                 # Distribution templates
├── web/                       # Website git submodule
└── .github/workflows/         # CI, signing and release automation
```

`build/`, `kit/.build/`, Cargo `target/`, `node_modules/` and Wrangler local state are generated working data. They are not source modules and should not become documentation locations. Edit `apps/project.yml` and run `xcodegen generate --spec apps/project.yml`; never hand-edit `project.pbxproj`.

## Where to make a change

| Responsibility | Source entry points |
|---|---|
| CLI and background service | `host/src/main.rs`, `service.rs` |
| API, caller permissions, event ordering | `host/src/api/`, `hub.rs` |
| SQLite transcript, bots, lanes and sessions | `host/src/store.rs` |
| Agent process, ACP, queue and session lifecycle | `host/src/agent/bot.rs`, `acp.rs` |
| Group room turns / bot-to-bot requests | `host/src/chat/group.rs` / `team.rs` |
| Prompt snapshots and memory keeper | `host/src/chat/context.rs`, `memory.rs` |
| Identity, encryption and direct channel | `host/src/remote/identity.rs`, `crypto.rs`, `channel.rs` |
| Host cloud state and relay connection | `host/src/remote/cloud.rs`, `relay.rs` |
| Agent discovery, sign-in, setup terminal | `host/src/agent/backends.rs`, `registry.rs`, `auth.rs`, `term.rs` |
| Marketplace, Composio, connector OAuth | `host/src/market/` |
| Screen bridge and built-in MCP tools | `host/src/screen.rs`, `mcp.rs` |
| Swift transport and cloud API | `kit/Sources/CodyncKit/Client/` |
| Account aggregation / one host mirror | `kit/Sources/CodyncUI/Store/AccountStore.swift` / `BotStore.swift` |
| Chat, replies, composer and trace | `kit/Sources/CodyncUI/Thread/` |
| Reusable Apple UI chrome | `kit/Sources/CodyncUI/Chrome.swift`, `Controls.swift`, `Platform.swift` |
| iOS navigation, pairing, account settings | `apps/ios/Views/RootView.swift`, `BotListView.swift`, `PairingView.swift`, `AccountSwitcherView.swift`, `SettingsView.swift` |
| Apple account sessions / public environment config | `apps/shared/AccountSession.swift`, `apps/shared/Config/` |
| Mac local host / SSH lifecycle | `apps/macos/App/HostController.swift`, `SSHTunnel.swift` |
| Cloud routes / authentication / relay | `cloud/src/index.ts`, `api.ts`, `auth.ts`, `relay.ts` |
| Push encryption / APNs delivery / decryption | `host/src/remote/push.rs`, `relay/src/`, `apps/ios/NotificationService/` |
| Widget and activity rendering | `kit/Sources/CodyncKit/Design/`, `apps/ios/Widgets/` |

## Dependency rules

- `CodyncKit` owns serializable models, clients and rendering primitives shared with widgets. It must not import `CodyncUI` or ClerkKit.
- `CodyncUI` owns observable stores and shared application screens. App-specific lifecycle, OAuth configuration and device hooks belong in `apps/`.
- Each `BotStore` talks to one computer. `AccountStore` aggregates stores and routes by `BotReference`; bare bot IDs are not globally unique.
- The host owns routing, reply counts, permissions and group turn scheduling. Clients render these results; they do not reimplement host policy.
- `cloud/` transports encrypted chat traffic and manages account metadata. `relay/` delivers APNs pushes. The two Workers have separate configuration and tests.
- `web/` is its own git repository; website changes require working in that submodule explicitly.

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

Use lowercase repository folders; PascalCase folders inside Swift targets. Swift files follow their main type (`UpperCamelCase.swift`), Rust uses `snake_case.rs` (a folder module is `name/mod.rs`), and TypeScript uses `kebab-case.ts`. Prefer one main type per file; small related view siblings can share a plural file such as `ChatRows.swift`.

Role suffixes: `View`, `Row`, `Card`, `Window`, `Store`, `Controller`; button styles describe the effect (`PressScale`). Keep feature vocabulary aligned across Swift, Rust, Linux and the terminal client.

UI-specific rules, including the native iPhone toolbar exception, live in [UI conventions](../design/ui-conventions.md).
