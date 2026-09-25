# Codync

Bot-based remote for coding agents: persistent named bots on your computer, messaged from the iPhone (UI patterns from Grok Bot).

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (`MenuBarExtra`, `@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance
- Host is Rust 2024 edition and follows the `rust-skills` rules (`~/.agents/skills/rust-skills`). Lints live in `host/Cargo.toml` (`[lints]`: default groups + pedantic, `unwrap_used`); CI runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- Host conventions: no `unwrap()` outside tests (`expect("why this can't fail")` for true invariants); lock std mutexes with `LockExt::locked()` (poison-tolerant); enums, not strings, for states and modes (`BotStatus`, `Permission`, `EntryKind`, `AlertKind`); `tracing` with structured fields (`error = format!("{e:#}")` keeps the context chain); blocking fs/process work goes through `spawn_blocking`; registry JSON is untrusted (paths are validated)

## Architecture

- Clients: iOS app, native Mac window (menu bar app), native Linux app (`apps/linux/`, GTK 4 + libadwaita in Rust). All talk to the host API; no web UI.
- Shared Swift (`kit/`): `CodyncKit` (models, client, theme, avatars) + `CodyncUI` (`BotStore` + chat screens) used by iOS and macOS. Platform specifics go through `BotStore` hooks or `Platform.swift`.
- UI principle: buttons an icon can express are icon-only (with tooltip / accessibility label); text only where an icon would be ambiguous (approval choices).
- `host/` — **codync-host** (Rust, macOS + Linux). Detects installed harnesses (`backends.rs`: login-shell PATH + known dirs) and the ACP registry (`registry.rs`, cached in `~/.codync/registry.json`, binaries under `~/.codync/agents`). Drives agents over **ACP** (JSON-RPC on stdio, hand-rolled in `acp.rs`, updates kept as `serde_json::Value` so new adapter variants never break parsing). One actor per bot (`bot.rs`) owns the agent process + session and maps `session/update` onto transcript entries.
- **Thread ≠ session**: a bot is one endless transcript (SQLite `entries`, ordered by `seq`); the ACP session underneath is resumed with `session/load` or replaced by *New session*.
- **Chat shows only**: user messages, the *final* agent message of each turn (`data.final`), permission cards, notices. Narration, thoughts, tool calls, plans are trace entries (Full conversation sheet).
- **Sync**: every mutation stamps a global `rev`. Clients call `GET /events?since=<rev>` (catch-up in rev order, then live). Emission happens under `Hub::emit_lock` so events leave in rev order. Clients upsert by id; never skip undecodable events (the iOS app rewinds to rev 0).
- API: `POST /api/<method>` + SSE, bearer token (`~/.codync/token`). Default port **19222**, binds 0.0.0.0 for Tailscale/LAN.
- Push: iOS registers its APNs token with `relay/` → gets an AES-GCM ticket → gives it to the host. Only two alert kinds: *needs you* and *done*, suppressed while the iOS app is connected.
- Usage: local only — `claude -p /usage --no-session-persistence`, the Claude status line (`codync-host statusline`, wrapping any existing one), Claude ACP `usage_update` rate-limit meta, Codex rollout files. Never call provider APIs with agent credentials.
- macOS app is thin: embeds `codync-host` in `Contents/MacOS` (Xcode post-build script runs cargo), installs it as a launchd agent via `codync-host install`; menu bar shows status/pairing/usage and opens the native chat window (NavigationSplitView over `CodyncUI`). Not sandboxed, not Mac App Store (the host must spawn CLIs).

## Codync 1.x does not exist for us

- Ignore everything from Codync 1.x (the Claude Code hooks + CloudKit session monitor): no migration, no compatibility shims, no cleanup of its files or hooks, no keeping old workers or App Store copy alive for it. Don't mention 1.x in code, docs or release notes.
- Build only the current design; don't reintroduce hooks or CloudKit.

## Project generation

- `project.yml` + `xcodegen generate` produce `Codync.xcodeproj`. Edit `project.yml`, not the pbxproj.

## App Store Upload

- **IMPORTANT**: Every time you archive and upload a new build to App Store Connect, you MUST increment `CURRENT_PROJECT_VERSION` first (App Store Connect rejects duplicate build numbers).
- Versions live in `project.yml` (`MARKETING_VERSION`, `CURRENT_PROJECT_VERSION`); regenerate the project after changing them. Keep `host/Cargo.toml` `version` in sync with `MARKETING_VERSION`.

## Versioning

- **Major version** defines phone ↔ host compatibility: an iOS app works with hosts/Mac apps of the same major
- Minor/patch bumps are always backward compatible within the same major; the iOS decoder is lenient (`Bot.init(from:)`) so small host additions don't break older apps

## Targets

- `Codync-macOS` (`apps/macos/`) — menu bar app + embedded host
- `Codync-iOS` (`apps/ios/`) — iOS app
- `CodyncWidgets` (`apps/widgets/`) — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `CodyncKit` (`kit/`) — shared Swift package: `CodyncKit` + `CodyncUI` libraries
- `apps/linux/` — `codync` GTK app (build/test in a container with libgtk-4-dev + libadwaita-1-dev)

## Layout & naming

```
apps/{ios,macos,widgets,linux}   one folder per client
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
