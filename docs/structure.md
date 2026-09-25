# Layout & naming

```
apps/{ios,macos,linux}   one folder per client (iOS widgets in apps/ios/Widgets, push decryption in apps/ios/NotificationService)
kit/Sources/CodyncKit/           Models/ Client/ Design/   (no CodyncUI, widget-safe)
kit/Sources/CodyncUI/            Store/ Bots/ Thread/ Marketplace/ Usage/ Resources/, cross-platform glue at root (Platform.swift)
host/  cloud/  relay/  web/  packaging/
```

Remote access ([spec](remote-relay-spec.md)): host `identity.rs` (keys) · `crypto.rs` (wire crypto) · `devices.rs` (authorized devices, `Caller`) · `channel.rs` (E2E channel, direct or relayed) · `relay.rs` (relay socket) · `cloud.rs` (cloud HTTP client, access requests). Kit `Client/`: `RelayCrypto`, `DeviceIdentity`, `Pairing`, `HostTransport`, `ChannelTransport`, `HostConnector`, `CloudClient`; models `Computer`, `Cloud`; `CodyncUI/Store/AccountStore`. Apps: `apps/shared/{AccountSession,ComputerStatus}.swift`, iOS `AccessRequestView` + `AccountSwitcherView` + `NotificationService/` (decrypts pushes), Mac `App/{HostController,SSHTunnel}.swift` + `Views/ComputersView.swift`. Cloud (`cloud/`, Worker + Durable Objects): `src/{index,auth,api,relay}.ts`, `migrations/` (D1), `test/` (vitest; `test/e2e/` against a live host).

Bot memory & teams ([context-and-memory](context-and-memory.md), [bot-collaboration](bot-collaboration.md)): host `memory.rs`, `context.rs`, `team.rs` (+ `tests/team_e2e.rs`); kit `CodyncUI/Bots/MemoryView.swift`. Widgets ([design](mobile-widgets-design.md)): shared cards in `CodyncKit/Design/WidgetCards.swift`, iOS `Widgets/WidgetPreviews.swift` + `Views/WidgetGalleryView.swift`.

- Directories: lowercase for repo-level roles (`apps/`, `kit/`, `host/`); PascalCase inside Swift targets (`Views/`, `Thread/`). Apple app folders are `App/` (entry point, app-wide services), `Views/`, `Resources/` (Info.plist, entitlements, xcprivacy, xcassets).
- Files follow their language: Swift `UpperCamelCase.swift` named after the file's main type; Rust/TS `snake_case.rs` / `kebab-case.ts`.
- One main type per file. Small private helpers of that type stay in it; a file of several small siblings takes the plural role (`ChatRows.swift`, `UsageViews.swift`, `Dialogs.swift`).
- Type suffixes by role: full screen / sheet → `…View`; list item → `…Row`; card → `…Card`; chat bubble → `…Bubble`; window scene → `…Window`; `@Observable` state → `…Store` / `…Controller`; `ButtonStyle` → describes the effect (`PressScale`).
- One word per concept across Swift, Rust host and Linux: **bot** (the persona you message), **agent** (the harness it runs, `Backend` in code), **thread** (the endless chat; not conversation/session), **session** (the ACP session only), **trace** (full conversation sheet), **marketplace** for UI / **market** for its data, **computer** (a paired host, user-facing) vs **host** (code).
- Linux mirrors the Swift feature names in its module names (`thread`, `editor`, `trace`, `settings`) when a file is split; don't invent new terms there.
