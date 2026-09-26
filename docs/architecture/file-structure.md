# Layout & naming

```
apps/{ios,macos,linux}   one folder per client (iOS widgets in apps/ios/Widgets, push decryption in apps/ios/NotificationService)
kit/Sources/CodyncKit/           Models/ Client/ Design/   (no CodyncUI, widget-safe)
kit/Sources/CodyncUI/            Store/ Bots/ Thread/ Call/ Screen/ Marketplace/ Usage/ Resources/, cross-platform glue at root (Platform.swift)
host/  cloud/  relay/  web/  packaging/
```

Remote access ([spec](../reference/remote-relay.md)): host `identity.rs` (keys) · `crypto.rs` (wire crypto) · `devices.rs` (authorized devices, `Caller`) · `channel.rs` (E2E channel, direct or relayed) · `relay.rs` (relay socket) · `cloud.rs` (cloud HTTP client, access requests). Kit `Client/`: `RelayCrypto`, `DeviceIdentity`, `Pairing`, `HostTransport`, `ChannelTransport`, `HostConnector`, `CloudClient`; models `Computer`, `Cloud`; `CodyncUI/Store/AccountStore`. Apps: `apps/shared/{AccountSession,ComputerStatus}.swift`, iOS `AccessRequestView` + `AccountSwitcherView` + `NotificationService/` (decrypts pushes), Mac `App/{HostController,SSHTunnel}.swift` + `Views/ComputersView.swift`. Cloud (`cloud/`, Worker + Durable Objects): `src/{index,auth,api,relay}.ts`, `migrations/` (D1), `test/` (vitest; `test/e2e/` against a live host).

Bot memory & teams ([context-and-memory](../features/context-and-memory.md), [bot-collaboration](../features/bot-collaboration.md)): host `memory.rs`, `context.rs`, `team.rs` (+ `tests/team_e2e.rs`); kit `CodyncUI/Bots/MemoryView.swift`. Widgets ([design](../design/mobile-widgets.md)): shared cards in `CodyncKit/Design/WidgetCards.swift`, iOS `Widgets/WidgetPreviews.swift` + `Views/WidgetGalleryView.swift`.

- Directories: lowercase for repo-level roles (`apps/`, `kit/`, `host/`); PascalCase inside Swift targets (`Views/`, `Thread/`). Apple app folders are `App/` (entry point, app-wide services), `Views/`, `Resources/` (Info.plist, entitlements, xcprivacy, xcassets).
- Files follow their language: Swift `UpperCamelCase.swift` named after the file's main type; Rust/TS `snake_case.rs` / `kebab-case.ts`.
- One main type per file. Small private helpers of that type stay in it; a file of several small siblings takes the plural role (`ChatRows.swift`, `UsageViews.swift`, `Dialogs.swift`).
- Type suffixes by role: full screen / sheet → `…View`; list item → `…Row`; card → `…Card`; chat bubble → `…Bubble`; window scene → `…Window`; `@Observable` state → `…Store` / `…Controller`; `ButtonStyle` → describes the effect (`PressScale`).
- One word per concept across Swift, Rust host and Linux: **bot** (the persona you message), **agent** (the harness it runs, `Backend` in code), **chat** (the endless conversation with a bot or group; not conversation/session), **thread** (replies on one chat message), **group** (a chat with several bots), **lane** (a chat or thread a turn talks in), **session** (the ACP session only), **trace** (full conversation sheet), **marketplace** for UI / **market** for its data, **computer** (a paired host, user-facing) vs **host** (code).
- Linux mirrors the Swift feature names in its module names (`thread`, `editor`, `trace`, `settings`) when a file is split; don't invent new terms there.

## iPhone navigation chrome

The bot list and conversation use the native `NavigationStack` toolbar: account and action buttons are `ToolbarItem`s, and the conversation uses the system back button. iOS supplies their Liquid Glass, sizing, grouping and press feedback. Do not add glass backgrounds or `IconButtonStyle` to these toolbar buttons. Other screens continue to use Codync's custom chrome.

Transient connection status appears in the toolbar center and remains visible for at least 0.7 seconds. It does not insert a connecting banner into the bot list or conversation content.
