# Development and validation

Run commands from the repository root unless a block changes directory. See [file structure](../architecture/file-structure.md) for ownership and [environment configuration](environments-and-deployment.md) before testing cloud access.

## Apple apps

Install Xcode and XcodeGen, then generate the project from its source:

```sh
xcodegen generate
xcodebuild build -project Codync.xcodeproj -scheme macOS -configuration Debug -derivedDataPath build/dd
```

Use the `iOS` scheme with a connected device or simulator in Xcode. CLI builds accept `-destination 'platform=iOS,id=<device-id>'`. Do not edit `project.pbxproj` directly. Keep normal simulator signing: Clerk uses Keychain, and unsigned simulator builds can fail initialization with OSStatus -34018.

Before launching a newly built Mac app, quit running copies, including DerivedData copies:

```sh
osascript -e 'tell application id "com.pokai.Codync" to quit'
pkill -x Codync
open build/dd/Build/Products/Debug/Codync.app
```

`pkill` can return nonzero when no process exists. If the development launch agent is installed, restart the embedded host and inspect other host processes for stale binaries:

```sh
launchctl kickstart -k gui/$(id -u)/com.pokai.codync.host
pgrep -fl codync-host
```

Install the device build and replace the old iPhone process (substitute its device ID):

```sh
xcrun devicectl device install app --device <device-id> build/dd/Build/Products/Debug-iphoneos/Codync.app
xcrun devicectl device process launch --terminate-existing --device <device-id> com.pokai.Codync.ios
```

Unlock the phone when required. Build, install, launch, and visual inspection are separate checks; report which actually completed.

## Component checks

| Component | Commands | Requirements |
| --- | --- | --- |
| Host | `cd host && cargo build` | Rust toolchain supporting edition 2024 |
| Host checks | `cd host && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | Local agent credentials are unnecessary for unit tests |
| Shared Swift | `swift test --package-path kit` | Swift 6 / Xcode |
| Cloud | `cd cloud && npm ci && npm test && npm run typecheck` | Node and npm; CI uses Node 24 |
| Cloud integration | `cd cloud && env -u CODYNC_CLOUD npm run e2e` | Built host; see [Cloudflare testing](cloudflare-testing.md) |
| APNs relay | `cd relay && npm ci && npm test && npm run typecheck` | Separate package from `cloud/` |
| Linux desktop | `cd apps/linux && cargo test` | GTK 4 and libadwaita development packages |

Host development: `cargo run --manifest-path host/Cargo.toml -- serve`. Avoid competing with an installed host on port 19222; isolated tests should use a temporary `CODYNC_HOME` and another port. Stop test hosts when finished.

## Visual checks

Follow [UI conventions](../design/ui-conventions.md) for native toolbar behavior and [widget design](../design/mobile-widgets.md) for previews. Widget images can be rendered with `python3 tools/render-widgets.py`; output is under `build/widget-previews/`.

For releases, increment `CURRENT_PROJECT_VERSION` before another App Store upload. Versions are defined in `project.yml`; keep the host package version aligned with `MARKETING_VERSION`, then regenerate the Xcode project.
