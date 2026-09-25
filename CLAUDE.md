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

- Clients: iOS app, native Mac window (menu bar app), native Linux app (`apps/linux/`, GTK 4 + libadwaita in Rust), terminal UI (`codync-host tui`, `host/src/tui/`, ratatui; layout and state vocabulary modeled on herdr). All talk to the host API; no web UI.
- Shared Swift (`kit/`): `CodyncKit` (models, client, theme, avatars) + `CodyncUI` (`BotStore` + chat screens) used by iOS and macOS. Platform specifics go through `BotStore` hooks or `Platform.swift`.
- UI principle: buttons an icon can express are icon-only (with tooltip / accessibility label); text only where an icon would be ambiguous (approval choices).
- No native/system UI at all: no `Menu`/`Picker`, `.switch` toggles, `Form`/`List` styling, `confirmationDialog`/`alert`, `ProgressView`, `.sheet`/`.popover`/`.fullScreenCover`, `.toolbar`/navigation bars, `TabView`, `ContentUnavailableView`. Use `kit/Sources/CodyncUI/Controls.swift` + `Chrome.swift` (`.codyncSheet`, `ModalHeader`, `ScreenHeader`, `TabBar`, `.codyncMenu`, `.codyncDialog`, `ToggleStyle.codync`). Every tap that shows/hides something animates (`Motion`). Anything with a background fill gets no border line.
- `host/` — **codync-host** (Rust, macOS + Linux). Detects installed harnesses (`backends.rs`: login-shell PATH + known dirs) and the ACP registry (`registry.rs`, cached in `~/.codync/registry.json`, binaries under `~/.codync/agents`). Drives agents over **ACP** (JSON-RPC on stdio, hand-rolled in `acp.rs`, updates kept as `serde_json::Value` so new adapter variants never break parsing). One actor per bot (`bot.rs`) owns the agent process + session and maps `session/update` onto transcript entries.
- **Thread ≠ session**: a bot is one endless transcript (SQLite `entries`, ordered by `seq`); the ACP session underneath is resumed with `session/load` or replaced by *New session*.
- Bot collaboration: built-in `team` MCP (`team.rs`) lists visible bots and asks one for a reply through its actor queue. Requests are separate turns, cycle-checked and cancellable; native subagents stay with the harness. See [docs/bot-collaboration.md](docs/bot-collaboration.md).
- **Context & memory** (Grok Bot's design): frozen instruction snapshot per session + compaction epoch (Claude gets it as a system prompt), profile edits as update blocks, per-bot memory files written by a keeper agent, busy-time messages folded into one turn, interrupted turns resumed: [docs/context-and-memory.md](docs/context-and-memory.md).
- **Chat shows only**: user messages, the *final* agent message of each turn (`data.final`), permission cards, notices. Narration, thoughts, tool calls, plans are trace entries (Full conversation sheet).
- **Sync**: every mutation stamps a global `rev`. Clients call `GET /events?since=<rev>` (catch-up in rev order, then live). Emission happens under `Hub::emit_lock` so events leave in rev order. Clients upsert by id; never skip undecodable events (the iOS app rewinds to rev 0).
- API: `POST /api/<method>` + SSE with the bearer token (`~/.codync/token`) is **loopback only** (Mac app, SSH tunnel, local helpers). Phones and other remote clients use the E2E channel (`/channel` direct, or the Cloudflare relay). Default port **19222**.
- Remote access (Cloudflare relay primary, direct LAN/Tailscale alternative, accounts, SSH): [docs/remote-relay-spec.md](docs/remote-relay-spec.md).
- Remote screen (`host/src/screen.rs` is the reference): phones view/control the computer over WebRTC (hardware H.264, non-trickle SDP relayed by `screenOffer`, input on data channels `input` / `input-fast`); bots get the built-in `computer` MCP server (`codync-host mcp computer`, `host/src/mcp.rs`) when their `computer` flag is on. Capture/input live in a helper on `~/.codync/screen.sock`: `apps/screen` (macOS, launchd agent via `SMAppService`, owns the TCC grants) or `apps/screen-linux` (`codync-screen`: portals + GStreamer, started by the host). Off by default; `setScreenEnabled` is accepted only from loopback. An interactive phone takes over (bots may only look).
- Environments: **dev** (`dev-api.codync.dev`, Clerk development instance, Debug builds) and **main** (`api.codync.dev`, Clerk production, Release builds); `apps/shared/Config/<env>.plist` becomes `AccountConfig.plist`. Details: [spec §14.0](docs/remote-relay-spec.md).
- Push: iOS registers its APNs token with `relay/` → gets an AES-GCM ticket → gives it (plus its X25519 push key) to the host; the host seals title/body to that key and a Notification Service Extension opens it, so `relay/` sees only generic text. Only two alert kinds: *needs you* and *done*, suppressed while the iOS app is connected.
- Usage: local only — `claude -p /usage --no-session-persistence`, the Claude status line (`codync-host statusline`, wrapping any existing one), Claude ACP `usage_update` rate-limit meta, Codex rollout files. Never call provider APIs with agent credentials.
- macOS app is thin: embeds `codync-host` in `Contents/MacOS` (Xcode post-build script runs cargo), installs it as a launchd agent via `codync-host install`; menu bar shows status/pairing/usage and opens the native chat window (NavigationSplitView over `CodyncUI`). Not sandboxed, not Mac App Store (the host must spawn CLIs).

## Codync 1.x does not exist for us

- Ignore everything from Codync 1.x (the Claude Code hooks + CloudKit session monitor): no migration, no compatibility shims, no cleanup of its files or hooks, no keeping old workers or App Store copy alive for it. Don't mention 1.x in code, docs or release notes.
- Build only the current design; don't reintroduce hooks or CloudKit.
- Don't carry legacy along. Old names, settings, schemes, files or code paths left from earlier designs get renamed or deleted outright when you meet them, not kept "for compatibility". Put full effort into the new design.

## Project generation

- `project.yml` + `xcodegen generate` produce `Codync.xcodeproj`. Edit `project.yml`, not the pbxproj.

## App Store Upload

- **IMPORTANT**: Every time you archive and upload a new build to App Store Connect, you MUST increment `CURRENT_PROJECT_VERSION` first (App Store Connect rejects duplicate build numbers).
- Versions live in `project.yml` (`MARKETING_VERSION`, `CURRENT_PROJECT_VERSION`); regenerate the project after changing them. Keep `host/Cargo.toml` `version` in sync with `MARKETING_VERSION`.

## Versioning

- **Major version** defines phone ↔ host compatibility: an iOS app works with hosts/Mac apps of the same major
- Minor/patch bumps are always backward compatible within the same major; the iOS decoder is lenient (`Bot.init(from:)`) so small host additions don't break older apps

## Targets

- `macOS` (`apps/macos/`) — menu bar app + embedded host
- `iOS` (`apps/ios/`) — iOS app
- `Screen` (`apps/screen/`) — Codync Screen: capture, input and WebRTC for Remote screen, embedded in the Mac app (`Contents/Library/LoginItems`)
- `apps/screen-linux/` — `codync-screen` (Rust, GStreamer + xdg portals), the Linux Remote screen helper; build/test in the same container as `apps/linux`
- `Widgets` (`apps/ios/Widgets/`) — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `CodyncKit` (`kit/`) — shared Swift package: `CodyncKit` + `CodyncUI` libraries
- `apps/linux/` — `codync` GTK app (build/test in a container with libgtk-4-dev + libadwaita-1-dev)

## Layout & naming

Folder layout, file naming and shared terms: [docs/structure.md](docs/structure.md). Follow it when adding or moving files.

## Keeping this file short

- CLAUDE.md holds only rules an agent needs on every task. Reference material (file structure, naming tables, API details, audits, how-tos) goes in `docs/` as its own file, with a one-line pointer here.
- When a section here grows past a few lines of reference detail, move it to `docs/` and leave the pointer.
- Keep `docs/` current: update the doc in the same change that makes it stale.
