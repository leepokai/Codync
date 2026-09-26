# Repository Guidelines

## Project Structure & Module Organization

- `host/`: Rust daemon, ACP integration, SQLite storage, HTTP/SSE API, and terminal UI; unit tests live alongside modules in `src/`.
- `kit/`: shared Swift package. `CodyncKit` contains models, clients, and design primitives; `CodyncUI` contains shared screens and stores. Tests and fixtures live in `kit/Tests/CodyncKitTests/`.
- `apps/`: iOS, macOS, and Linux clients, plus `screen` and `screen-linux` helpers. Apple assets live in each target’s `Resources/`; widgets live in `apps/ios/Widgets/`.
- `relay/`: Cloudflare push worker and `test/`; `web/`: website git submodule; `packaging/`: distribution templates; `docs/`: architecture and naming guidance.

## Build, Test, and Development Commands

Run from the repository root unless a command changes directories:

- `xcodegen generate`: regenerate `Codync.xcodeproj` after editing `project.yml`; never edit `project.pbxproj` directly.
- `xcodebuild build -project Codync.xcodeproj -scheme macOS -configuration Debug`: build the Mac app. Use Xcode’s `iOS` scheme to run on a simulator or device.
- `cd host && cargo build`: build the host; `cargo run -- serve` starts it in the foreground.
- `cd host && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`: run host CI checks.
- `cd kit && swift test`: run shared Swift tests.
- `cd apps/linux && cargo test`: test Linux code; requires GTK 4 and libadwaita development packages.
- `cd relay && npm ci && npm test && npm run typecheck`: install dependencies and validate the relay.

## Installing a new build: kill the old one first

Whenever you build and install/run a new build, stop the old copies so nothing stale keeps running (old host = old protocol, old app = old UI):
- **Mac app**: quit every running Codync (including copies from Xcode DerivedData) before opening the new one: `osascript -e 'tell application id "com.pokai.Codync" to quit'; pkill -x Codync`, then `open build/dd/Build/Products/Debug/Codync.app`.
- **Host**: the launchd agent `com.pokai.codync.host` runs `build/dd/.../Codync.app/Contents/MacOS/codync-host`; after rebuilding, restart it with `launchctl kickstart -k gui/$(id -u)/com.pokai.codync.host`, and kill any other `codync-host` still running from a different path (`pgrep -fl codync-host`). Test hosts you start yourself must be stopped when done.
- **iPhone**: after `xcrun devicectl device install app …`, relaunch with `xcrun devicectl device process launch --terminate-existing --device <id> com.pokai.Codync.ios` so the old process doesn't linger.

## Coding Style & Naming Conventions

Use four-space indentation for Swift and Rust. Follow Swift 6 strict concurrency, SwiftUI, and structured `async/await`; avoid unchecked sendability. Rust uses edition 2024, rustfmt, and Clippy; avoid `unwrap()` outside tests.

Name Swift files after their main `UpperCamelCase` type; use `snake_case.rs` and `kebab-case.ts`. Follow role suffixes such as `View`, `Row`, and `Store`. See `docs/architecture/file-structure.md` and `CLAUDE.md` for architectural conventions.

- No native/system UI at all: no `Menu`/`Picker`, `.switch` toggles, `Form`/`List` styling, `confirmationDialog`/`alert`, `ProgressView`, `.sheet`/`.popover`/`.fullScreenCover`, `.toolbar`/navigation bars, `TabView`, `ContentUnavailableView`. Use `kit/Sources/CodyncUI/Controls.swift` + `Chrome.swift` (`.codyncSheet`, `ModalHeader`, `ScreenHeader`, `TabBar`, `.codyncMenu`, `.codyncDialog`, `ToggleStyle.codync`). Every tap that shows/hides something animates (`Motion`). Anything with a background fill gets no border line.

## Testing Guidelines

Use Swift Testing (`@Test`, `#expect`), Rust unit tests, and the relay’s Node assertion tests. Name tests after observable behavior. Add regression coverage for changed logic; no numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

History mixes descriptive subjects with scoped Conventional Commits, such as `feat(macos): ...`. Keep subjects concise and changes focused. PRs should explain behavior changes, link relevant issues, list validation performed, and include screenshots for UI changes. Update affected documentation in the same change.
