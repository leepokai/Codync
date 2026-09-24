# Codync

Bot-based remote for coding agents: persistent named bots on your computer, messaged from the iPhone (UI patterns from Grok Bot).

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (`MenuBarExtra`, `@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance
- Host is Rust 2024 edition; keep `cargo clippy -- -D warnings` clean

## Architecture

- Clients: iOS app, native Mac window (menu bar app), native Linux app (`linux/`, GTK 4 + libadwaita in Rust). All talk to the host API; no web UI.
- Shared Swift: `CodyncKit` (models, client, theme, avatars) + `CodyncUI` (`BotStore` + chat screens) used by iOS and macOS. Platform specifics go through `BotStore` hooks or `Platform.swift`.
- UI principle: buttons an icon can express are icon-only (with tooltip / accessibility label); text only where an icon would be ambiguous (approval choices).
- `host/` — **codync-host** (Rust, macOS + Linux). Detects installed harnesses (`backends.rs`: login-shell PATH + known dirs) and the ACP registry (`registry.rs`, cached in `~/.codync/registry.json`, binaries under `~/.codync/agents`). Drives agents over **ACP** (JSON-RPC on stdio, hand-rolled in `acp.rs`, updates kept as `serde_json::Value` so new adapter variants never break parsing). One actor per bot (`bot.rs`) owns the agent process + session and maps `session/update` onto transcript entries.
- **Thread ≠ session**: a bot is one endless transcript (SQLite `entries`, ordered by `seq`); the ACP session underneath is resumed with `session/load` or replaced by *New session*.
- **Chat shows only**: user messages, the *final* agent message of each turn (`data.final`), permission cards, notices. Narration, thoughts, tool calls, plans are trace entries (Full conversation sheet).
- **Sync**: every mutation stamps a global `rev`. Clients call `GET /events?since=<rev>` (catch-up in rev order, then live). Emission happens under `Hub::emit_lock` so events leave in rev order. Clients upsert by id; never skip undecodable events (the iOS app rewinds to rev 0).
- API: `POST /api/<method>` + SSE, bearer token (`~/.codync/token`). Default port **19222**, binds 0.0.0.0 for Tailscale/LAN.
- Push: iOS registers its APNs token with `relay/` → gets an AES-GCM ticket → gives it to the host. Only two alert kinds: *needs you* and *done*, suppressed while the iOS app is connected.
- Usage: local only — `claude -p /usage --no-session-persistence`, the Claude status line (`codync-host statusline`, wrapping any existing one), Claude ACP `usage_update` rate-limit meta, Codex rollout files. Never call provider APIs with agent credentials.
- macOS app is thin: embeds `codync-host` in `Contents/MacOS` (Xcode post-build script runs cargo), installs it as a launchd agent via `codync-host install`; menu bar shows status/pairing/usage and opens the native chat window (NavigationSplitView over `CodyncUI`). Not sandboxed, not Mac App Store (the host must spawn CLIs).
- Do **not** reintroduce Claude Code hooks or CloudKit — both were removed in 2.0.

## Project generation

- `project.yml` + `xcodegen generate` produce `Codync.xcodeproj`. Edit `project.yml`, not the pbxproj.

## App Store Upload

- **IMPORTANT**: Every time you archive and upload a new build to App Store Connect, you MUST increment `CURRENT_PROJECT_VERSION` first (App Store Connect rejects duplicate build numbers).
- Versions live in `project.yml` (`MARKETING_VERSION`, `CURRENT_PROJECT_VERSION`); regenerate the project after changing them. Keep `host/Cargo.toml` `version` in sync with `MARKETING_VERSION`.

## Versioning

- **Major version** defines phone ↔ host compatibility: 2.x iOS works with 2.x host/macOS
- 2.x is NOT compatible with 1.x (1.x was the hooks + CloudKit monitor)
- Minor/patch bumps are always backward compatible within the same major; the iOS decoder is lenient (`Bot.init(from:)`) so small host additions don't break older apps

## Targets

- `Codync-macOS` — menu bar app + embedded host
- `Codync-iOS` — iOS app
- `CodyncWidgets` — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `CodyncKit` — shared Swift package: `CodyncKit` + `CodyncUI` libraries
- `linux/` — `codync` GTK app (build/test in a container with libgtk-4-dev + libadwaita-1-dev)
