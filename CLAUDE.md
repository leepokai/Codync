# Codync

Bot-based remote for coding agents: persistent named bots on your computer, messaged from the iPhone (UI patterns from Grok Bot).

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (`@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance
- Desktop app (`apps/desktop/`) is Electron + React + strict TypeScript: state in `Observable` models (`src/renderer/store/`), host calls through the `window.codync` bridge (`src/shared/ipc.ts`), main-process code in `src/main/`; CI runs `npm run typecheck` and `npm test`
- Host is Rust 2024 edition and follows the `rust-skills` rules (`~/.agents/skills/rust-skills`). Lints live in `host/Cargo.toml` (`[lints]`: default groups + pedantic, `unwrap_used`); CI runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- Host conventions: no `unwrap()` outside tests (`expect("why this can't fail")` for true invariants); lock std mutexes with `LockExt::locked()` (poison-tolerant); enums, not strings, for states and modes (`BotStatus`, `Permission`, `EntryKind`, `AlertKind`); `tracing` with structured fields (`error = format!("{e:#}")` keeps the context chain); blocking fs/process work goes through `spawn_blocking`; registry JSON is untrusted (paths are validated)

## Clean code

- Keep code modular and clean on every change: reuse before writing, one responsibility per file/type, no new code appended to files past ~500 lines (split first), views/components hold no I/O or logic, short single-purpose functions, enums over flags, no dead code, no swallowed errors. Full rules: [docs/guides/clean-code.md](docs/guides/clean-code.md).

## Architecture

- Clients: iOS app (SwiftUI), desktop app for macOS, Linux and Windows (`apps/desktop/`, Electron: menu bar/tray + chat window; why and how: [docs/architecture/desktop-app.md](docs/architecture/desktop-app.md)), terminal UI (`codync-host tui`, `host/src/tui/`, ratatui; layout and state vocabulary modeled on herdr). All talk to the host API.
- iPhone Swift package (`apps/ios/Kit/`, iOS only): `CodyncKit` (models, client, theme, avatars; also used by the widgets and the notification extension) + `CodyncUI` (`BotStore` + screens).
- UI principle: buttons an icon can express are icon-only (with tooltip / accessibility label); text only where an icon would be ambiguous (approval choices).
- UI controls default to the shared custom components. Explicit exception: iOS BotListView and ThreadView use native navigation/toolbar items and automatic back navigation for system Liquid Glass, as specified in `docs/design/ui-conventions.md`. **iOS menus are always the system ones**: tap menus through `DropdownMenu`/`ChoicePicker` (native `Menu`), long-press through `.contextActions` (native `contextMenu`). Never hand-build a dropdown on iOS: a custom overlay lands in the wrong place (sheets, scroll views, the composer's + menu). The desktop app's menu bar/tray menu is the system menu. Keep system authentication and widget containers native. On iOS `.codyncSheet` presents the system sheet (grabber, swipe down). Outside these exceptions, avoid: no `Menu`/`Picker`, `.switch` toggles, `Form`/`List` styling, `confirmationDialog`/`alert`, `ProgressView`, `.sheet`/`.popover`/`.fullScreenCover`, `.toolbar`/navigation bars, `TabView`, `ContentUnavailableView`. Use `apps/ios/Kit/Sources/CodyncUI/Controls.swift` + `Chrome.swift` (`.codyncSheet`, `ModalHeader`, `ScreenHeader`, `TabBar`, `.codyncDialog`, `ToggleStyle.codync`); on the desktop, `apps/desktop/src/renderer/components/` (`Sheet`, `Dialog`, `AnchoredMenu`, `ModalHeader`, `Controls.tsx`, `Icon` for SF Symbols). Every tap that shows/hides something animates (`Motion`). Anything with a background fill gets no border line.
- `host/` — **codync-host** (Rust, macOS + Linux + Windows; platform differences in `service/` and `shell.rs`). Detects installed harnesses (`agent/backends.rs`: login-shell PATH + known dirs) and the ACP registry (`agent/registry.rs`, cached in `~/.codync/registry.json`, binaries under `~/.codync/agents`). Drives agents over **ACP** (JSON-RPC on stdio, hand-rolled in `agent/acp.rs`, updates kept as `serde_json::Value` so new adapter variants never break parsing). One actor per bot (`agent/bot/`) owns the agent process + session and maps `session/update` onto transcript entries.
- **Chat ≠ session**: a bot is one endless transcript (SQLite `entries`, ordered by `seq`); the ACP session underneath is resumed with `session/load` or replaced by *New session*.
- **Group chats and threads** are host features every client drives through the same methods (`send` with `threadId`, `thread`, `createBot {kind: group}`); clients never route, parse mentions or count replies themselves. A group is a roster row whose members answer in their own sessions (Grok Bot's room turns); a thread on a bot's message is a forked session: [docs/features/groups-and-threads.md](docs/features/groups-and-threads.md).
- Bot collaboration: built-in `team` MCP (`chat/team.rs`) lists visible bots and asks one for a reply through its actor queue. Requests are separate turns, cycle-checked and cancellable; native subagents stay with the harness. See [docs/features/bot-collaboration.md](docs/features/bot-collaboration.md).
- **Context & memory** (Grok Bot's design): frozen instruction snapshot per session + compaction epoch (Claude gets it as a system prompt), profile edits as update blocks, per-bot memory files written by a keeper agent, busy-time messages folded into one turn, interrupted turns resumed: [docs/features/context-and-memory.md](docs/features/context-and-memory.md).
- **Chat shows only**: user messages, the bot's messages (`data.final`: each `send_message` call from the built-in `chat` MCP, Grok Bot's way; a turn that sent none falls back to its last text), permission cards, notices. Nothing streams into a bubble; narration, thoughts, tool calls, plans are trace entries (Full conversation sheet): [docs/features/chat-messages.md](docs/features/chat-messages.md).
- **Sync**: every mutation stamps a global `rev`. Clients call `GET /events?since=<rev>` (catch-up in rev order, then live). Emission happens under `Hub::emit_lock` so events leave in rev order. Clients upsert by id; never skip undecodable events (the iOS app rewinds to rev 0).
- API: `POST /api/<method>` + SSE with the bearer token (`~/.codync/token`) is **loopback only** (desktop app, SSH tunnel, local helpers). Phones and other remote clients use the E2E channel (`/channel` direct, or the Cloudflare relay). Default port **19222**.
- Remote access (Cloudflare relay primary, direct LAN/Tailscale alternative, accounts, SSH, public routine webhooks queued in the relay §7.8): [docs/reference/remote-relay.md](docs/reference/remote-relay.md).
- Remote screen (`host/src/screen/` is the reference): phones view/control the computer over WebRTC (hardware H.264, non-trickle SDP relayed by `screenOffer`, input on data channels `input` / `input-fast`); bots get the built-in `computer` MCP server (`codync-host mcp computer`, `host/src/mcp.rs`) when their `computer` flag is on; its tools run on the bundled cua-driver, which Codync Screen starts on macOS so its grants apply ([computer use](docs/features/computer-use.md)). Capture/input live in a helper on `~/.codync/screen.sock`: `apps/screen-macos` (macOS, launchd agent the desktop app registers via `SMAppService`, owns the TCC grants) or `apps/screen-linux` (`codync-screen`: portals + GStreamer, started by the host). On by default (Linux: only with a graphical session); `setScreenEnabled` is accepted only from loopback. An interactive phone takes over (bots may only look).
- Multiple computers per account are future work, not a current priority: the product targets one computer. The multi-computer structure (`AccountStore`, `BotReference`) stays because relay/accounts/SSH build on it; don't extend or polish multi-computer features unless asked.
- Environments: **dev** (`dev-api.codync.dev`, Clerk development instance, Debug builds) and **main** (`api.codync.dev`, Clerk production, Release builds); `apps/shared/Config/<env>.plist` becomes the iOS `AccountConfig.plist` and the desktop `resources/account-config.json` (`tools/account-config.mjs`). Details: [spec §14.0](docs/reference/remote-relay.md).
- Push: iOS registers its APNs token with `relay/` → gets an AES-GCM ticket → gives it (plus its X25519 push key) to the host; the host seals title/body to that key and a Notification Service Extension opens it, so `relay/` sees only generic text. Alert kinds: *needs you*, *done*, and *failed*, suppressed while the iOS app is connected. Delivery and lifecycle: [notification design](docs/design/push-and-live-activity.md).
- Voice call (iPhone, desktop): on-device speech (macOS desktop: the `codync-speech` helper), or OpenAI / Gemini realtime on the user's own key (kept in the host's vault, which mints per-call credentials; audio goes device ↔ provider); the host sees plain messages. Grok-style call bar: [docs/features/voice-call.md](docs/features/voice-call.md).
- Analytics: opt-in PostHog, asked once per device, feature use only (never content); host events come from `api::dispatch`: [docs/features/analytics.md](docs/features/analytics.md).
- Usage: local only — `claude -p /usage --no-session-persistence`, the Claude status line (`codync-host statusline`, wrapping any existing one), Claude ACP `usage_update` rate-limit meta, Codex rollout files. Never call provider APIs with agent credentials.
- The desktop app is thin: the Mac build bundles `codync-host` in `Contents/Resources` (Linux uses the installed host) and installs it as a launchd/systemd agent via `codync-host install`; the menu bar/tray shows status/pairing/usage and opens the chat window. Not sandboxed, not Mac App Store (the host must spawn CLIs).

## Cross-platform UI changes

- Any UI change in any client must include the corresponding updates to the other clients in the same change: iOS (`apps/ios/`, `apps/ios/Kit/Sources/CodyncUI/`), desktop (`apps/desktop/src/renderer/`, macOS, Linux and Windows), and terminal UI (`host/src/tui/`). This applies in every direction.
- Keep shared features, actions, terminology, displayed information, and loading, empty, error, and permission states consistent. Adapt layout, controls, and input to each platform, including terminal keyboard interaction, while preserving the same user-facing behavior.
- Inspect every client's corresponding implementation before finishing a UI task. Implement applicable changes together; do not silently defer another client. For a platform-only change or an unsupported capability, document which clients are unaffected and the concrete reason in the change summary.
- Validate each affected client with its relevant build/tests and UI checks. Report any checks that could not run and why.
- The desktop app is one codebase for macOS, Linux and Windows: platform differences go through `window.codync.platform` checks or the main process, never a fork of a view.

## Codync 1.x does not exist for us

- Ignore everything from Codync 1.x (the Claude Code hooks + CloudKit session monitor): no migration, no compatibility shims, no cleanup of its files or hooks, no keeping old workers or App Store copy alive for it. Don't mention 1.x in code, docs or release notes.
- Build only the current design; don't reintroduce hooks or CloudKit.
- Don't carry legacy along. Old names, settings, schemes, files or code paths left from earlier designs get renamed or deleted outright when you meet them, not kept "for compatibility". Put full effort into the new design.

## Installing a new build: kill the old one first

Always stop the old desktop app, host and iPhone process before running a new build (old host = old protocol, old app = old UI); commands in [docs/guides/development.md](docs/guides/development.md#apple-apps).

Keep only the latest build: in this checkout, Apple builds go to `build/dd` only (no other `-derivedDataPath`, no copies in scratchpads, `/tmp` or Xcode's DerivedData); delete any older Codync build right away, so macOS never launches a stale copy. A git worktree may keep its own single build inside that worktree.

## Project generation

- `apps/project.yml` + `xcodegen generate --spec apps/project.yml` produce `apps/Codync.xcodeproj`. Edit `project.yml`, not the pbxproj.

## App Store Upload

- Xcode Cloud archives the `iOS` scheme on every `v*` tag and uploads it to App Store Connect with its own build number, then the `Submit iOS` workflow submits it to App Review for the App Store; don't upload from the local machine or edit `CURRENT_PROJECT_VERSION` for it. Details: [development guide](docs/guides/development.md#ios-releases-xcode-cloud).

## Versioning

- Phone ↔ host compatibility: each side names the oldest version of the other it works with (`minApp` in the host's `hello`, `minHost` in each client). Additive changes need nothing; a rename/removal raises `minApp` in the same release, and hosts hold that release until the App Store has the iPhone app: [docs/reference/compatibility.md](docs/reference/compatibility.md)
- **Every source change ships as a release.** A version bump on `main` is the only release trigger (Auto Tag → desktop app for macOS, Linux and Windows, host, Homebrew, in-app update), so any change to shipped code (`apps/`, `host/`, `packaging/`) bumps the version before it reaches `main`, without being asked. Docs, `web/`, `cloud/`, `relay/` and CI-only changes don't bump.
- Pick the bump yourself: patch for fixes and small tweaks, minor for new features or protocol additions. Never bump major (stay on 2.x). One bump per merge into `main`: if `MARKETING_VERSION` is already ahead of the latest `v*` tag, leave it.
- How: set `MARKETING_VERSION` in `apps/project.yml` and `version` in `host/Cargo.toml` (+ its `Cargo.lock` entry) and `apps/desktop/package.json` (+ `package-lock.json`) to the same value, run `xcodegen generate --spec apps/project.yml`, commit as `build: bump version to X.Y.Z`. Never touch `CURRENT_PROJECT_VERSION` (Xcode Cloud sets the iOS build number). When the iOS app changes, also write that version's section in `apps/ios/WhatsNew.md` (zh-Hant + en-US, what iPhone users notice); it becomes the App Store "What's New".

## Layout & naming

Build targets, folder layout, file naming and shared terms: [docs/architecture/file-structure.md](docs/architecture/file-structure.md). Follow it when adding or moving files.

## Several agents share `dev`

- Several agents often work in this checkout on `dev` at once. When you start a task, name your session after its area (`/rename kit-markdown`, `host-voice`, …; ask the user if you can't rename yourself) so others can find you in `ListAgents`.
- Before touching files with someone else's uncommitted changes, or anything tree-wide (renames, `xcodegen`, version bump, `git stash`/`reset`/`checkout`), `SendMessage` the agents involved with what you'll change and wait for or answer their replies. Never discard, revert or reformat hunks that aren't yours.
- External contributors open PRs against `dev`, never `main`. Retarget a contributor PR aimed at `main` to `dev` before reviewing or merging it.

## Commit messages

- Conventional Commits: `type(scope): subject`. Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`. Scope is the area touched: `ios`, `desktop`, `host`, `cloud`, `relay`, `web`, `docs`.
- Subject: imperative mood ("add", not "added"), lowercase after the colon, no trailing period, at most 72 characters (aim for 50). Say what changes for the user, not which files moved.
- Body (after a blank line, wrapped at 72 columns) when the change isn't obvious from the subject: what and why, not how. Bullets are fine.
- One logical change per commit. Don't mix unrelated work, and stage only your own hunks when others have uncommitted changes in the tree.
- Breaking changes: `!` after the type/scope, or a `BREAKING CHANGE:` footer.
- English only.
- PRs: fill the template's *What's New* bullets (zh-Hant + en-US) for user-visible iOS changes; they become the App Store notes when `apps/ios/WhatsNew.md` has no section for the release.

## Keeping this file short

- CLAUDE.md and AGENTS.md hold only rules an agent needs on every task. Reference material (file structure, naming tables, API details, audits, how-tos) goes in `docs/` as its own file, with a one-line pointer here.
- Whenever you edit either file, check its length: past ~100 lines, or a section past a few lines of reference detail, refactor that detail into `docs/` and leave the pointer, without being asked.
- Keep `docs/` current: update the doc in the same change that makes it stale.
