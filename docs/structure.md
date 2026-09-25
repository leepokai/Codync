# Layout & naming

```
apps/{ios,macos,linux}   one folder per client (iOS widgets in apps/ios/Widgets)
kit/Sources/CodyncKit/           Models/ Client/ Design/   (no CodyncUI, widget-safe)
kit/Sources/CodyncUI/            Store/ Bots/ Thread/ Marketplace/ Usage/ Resources/, cross-platform glue at root (Platform.swift)
host/  relay/  web/  packaging/
```

- Directories: lowercase for repo-level roles (`apps/`, `kit/`, `host/`); PascalCase inside Swift targets (`Views/`, `Thread/`). Apple app folders are `App/` (entry point, app-wide services), `Views/`, `Resources/` (Info.plist, entitlements, xcprivacy, xcassets).
- Files follow their language: Swift `UpperCamelCase.swift` named after the file's main type; Rust/TS `snake_case.rs` / `kebab-case.ts`.
- One main type per file. Small private helpers of that type stay in it; a file of several small siblings takes the plural role (`ChatRows.swift`, `UsageViews.swift`, `Dialogs.swift`).
- Type suffixes by role: full screen / sheet → `…View`; list item → `…Row`; card → `…Card`; chat bubble → `…Bubble`; window scene → `…Window`; `@Observable` state → `…Store` / `…Controller`; `ButtonStyle` → describes the effect (`PressScale`).
- One word per concept across Swift, Rust host and Linux: **bot** (the persona you message), **agent** (the harness it runs, `Backend` in code), **thread** (the endless chat; not conversation/session), **session** (the ACP session only), **trace** (full conversation sheet), **marketplace** for UI / **market** for its data, **computer** (a paired host, user-facing) vs **host** (code).
- Linux mirrors the Swift feature names in its module names (`thread`, `editor`, `trace`, `settings`) when a file is split; don't invent new terms there.
