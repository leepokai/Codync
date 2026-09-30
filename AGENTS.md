# Repository Guidelines

## Project Structure & Module Organization

- `host/`: Rust daemon, ACP integration, SQLite storage, HTTP/SSE API, and terminal UI; unit tests live alongside modules in `src/`.
- `kit/`: shared Swift package. `CodyncKit` contains models, clients, and design primitives; `CodyncUI` contains shared screens and stores. Tests and fixtures live in `kit/Tests/CodyncKitTests/`.
- `apps/`: iOS, macOS, and Linux clients, plus `screen-macos` and `screen-linux` helpers. Apple assets live in each target’s `Resources/`; widgets live in `apps/ios/Widgets/`.
- `cloud/`: Cloudflare accounts, encrypted relay, Durable Objects and D1; tests live in `cloud/test/`.
- `relay/`: Cloudflare push worker and `test/`; `web/`: website git submodule; `packaging/`: distribution templates; `docs/`: architecture and naming guidance.

## Build, Test, and Development Commands

Run from the repository root unless a command changes directories:

- `xcodegen generate --spec apps/project.yml`: regenerate `apps/Codync.xcodeproj` after editing `apps/project.yml`; never edit `project.pbxproj` directly.
- `xcodebuild build -project apps/Codync.xcodeproj -scheme macOS -configuration Debug`: build the Mac app. Use Xcode’s `iOS` scheme to run on a simulator or device.
- `cd host && cargo build`: build the host; `cargo run -- serve` starts it in the foreground.
- `cd host && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`: run host CI checks.
- `cd kit && swift test`: run shared Swift tests.
- `cd apps/linux && cargo test`: test Linux code; requires GTK 4, libadwaita and VTE GTK 4 development packages.
- `cd cloud && npm ci && npm test && npm run typecheck`: validate the cloud service; see `docs/guides/cloudflare-testing.md` for integration checks.
- `cd relay && npm ci && npm test && npm run typecheck`: install dependencies and validate the relay.

## Installing a new build: kill the old one first

Whenever you build and install/run a new build, stop the old copies so nothing stale keeps running (old host = old protocol, old app = old UI):
- **Mac app**: quit every running Codync (including copies from Xcode DerivedData) before opening the new one: `osascript -e 'tell application id "com.pokai.Codync" to quit'; pkill -x Codync`, then `open build/dd/Build/Products/Debug/Codync.app`.
- **Host**: the launchd agent `com.pokai.codync.host` runs `build/dd/.../Codync.app/Contents/MacOS/codync-host`; after rebuilding, restart it with `launchctl kickstart -k gui/$(id -u)/com.pokai.codync.host`, and kill any other `codync-host` still running from a different path (`pgrep -fl codync-host`). Test hosts you start yourself must be stopped when done.
- **iPhone**: after `xcrun devicectl device install app …`, relaunch with `xcrun devicectl device process launch --terminate-existing --device <id> com.pokai.Codync.ios` so the old process doesn't linger.

## Coding Style & Naming Conventions

Use four-space indentation for Swift and Rust. Follow Swift 6 strict concurrency, SwiftUI, and structured `async/await`; avoid unchecked sendability. Rust uses edition 2024, rustfmt, and Clippy; avoid `unwrap()` outside tests.

Name Swift files after their main `UpperCamelCase` type; use `snake_case.rs` and `kebab-case.ts`. Follow role suffixes such as `View`, `Row`, and `Store`. See `docs/architecture/file-structure.md` and `CLAUDE.md` for architectural conventions.

- UI controls default to the shared custom components. Explicit exception: iOS BotListView and ThreadView use native navigation/toolbar items and automatic back navigation for system Liquid Glass, as specified in `docs/design/ui-conventions.md`. The macOS menu bar also uses native `MenuBarExtra(.menu)`, menus, pickers and toggles. Keep system authentication and widget containers native. Outside these exceptions, avoid: no `Menu`/`Picker`, `.switch` toggles, `Form`/`List` styling, `confirmationDialog`/`alert`, `ProgressView`, `.sheet`/`.popover`/`.fullScreenCover`, `.toolbar`/navigation bars, `TabView`, `ContentUnavailableView`. Use `kit/Sources/CodyncUI/Controls.swift` + `Chrome.swift` (`.codyncSheet`, `ModalHeader`, `ScreenHeader`, `TabBar`, `.codyncMenu`, `.codyncDialog`, `ToggleStyle.codync`). Every tap that shows/hides something animates (`Motion`). Anything with a background fill gets no border line.

## Cross-platform UI changes

- Any UI change in any client must include the corresponding updates to all other native clients and the TUI in the same change: shared SwiftUI (`kit/Sources/CodyncUI/`), iOS (`apps/ios/`), macOS (`apps/macos/`), Linux GTK (`apps/linux/src/`), and terminal UI (`host/src/tui/`). This applies in every direction; Linux and TUI changes must also be reflected in SwiftUI.
- Keep shared features, actions, terminology, displayed information, and loading, empty, error, and permission states consistent. Adapt layout, controls, and input to each platform, including terminal keyboard interaction, while preserving the same user-facing behavior.
- Inspect every client's corresponding implementation before finishing a UI task. Implement applicable changes together; do not silently defer another client. For a platform-only change or an unsupported capability, document which clients are unaffected and the concrete reason in the change summary.
- Validate each affected client with its relevant build/tests and UI checks. Report any checks that could not run and why.

## Testing Guidelines

Use Swift Testing (`@Test`, `#expect`), Rust unit tests, and the relay’s Node assertion tests. Name tests after observable behavior. Add regression coverage for changed logic; no numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

- Conventional Commits: `type(scope): subject`. Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`. Scope is the area touched: `ios`, `macos`, `kit`, `host`, `linux`, `cloud`, `relay`, `web`, `docs`.
- Subject: imperative mood ("add", not "added"), lowercase after the colon, no trailing period, at most 72 characters (aim for 50). Say what changes for the user, not which files moved.
- Body (after a blank line, wrapped at 72 columns) when the change isn't obvious from the subject: what and why, not how. Bullets are fine.
- One logical change per commit. Don't mix unrelated work, and stage only your own hunks when others have uncommitted changes in the tree.
- Breaking changes: `!` after the type/scope, or a `BREAKING CHANGE:` footer.
- English only; no `Co-Authored-By` or other AI attribution trailers (the commit-msg hook rejects them).

PRs should explain behavior changes, link relevant issues, list validation performed, and include screenshots for UI changes. Update affected documentation in the same change.
