# Development and validation

For release updates, automatic installation and host rollback, see [updates](updates.md).

Run commands from the repository root unless a block changes directory. See [file structure](../architecture/file-structure.md) for ownership and [environment configuration](environments-and-deployment.md) before testing cloud access.

## Desktop app

The macOS, Linux and Windows app is the Electron app in `apps/desktop/`; setup, UI-check switches and packaging are in [desktop app](../architecture/desktop-app.md#development):

```sh
cd apps/desktop
npm ci
npm run icons      # macOS only
npm run dev
```

Before launching a new build, quit only that variant. Production and Dev may coexist; see [Dev app isolation](dev-app.md). For a development rebuild:

```sh
osascript -e 'tell application id "com.pokai.Codync.dev" to quit'
pkill -x "Codync Dev"
```

`pkill` can return nonzero when no process exists.

The desktop app (`apps/desktop/src/main/host-controller.ts`) compares the running
host's executable path and SHA-256 fingerprint with the host it would install (the
bundled one in a packaged Mac app, else `CODYNC_HOST_BIN` or an installed
`codync-host`), so reopening after a rebuild replaces the service even when the
version number has not changed. **Restart host** runs `codync-host install` again,
which waits for the old host's data lock to be released. If a manually started host
still owns that data directory, installation fails instead of reporting a
successful restart; stop that host in its terminal and retry. Hosts attached using
`CODYNC_PORT` are managed manually.

On SIGTERM the host stops its bots without waiting for open SSE or WebSocket
connections. ACP adapters run in separate process groups; stopping an adapter
also kills tools and MCP servers that remain in its group. Processes that detach
into their own sessions and unrelated hosts using other data directories are
outside this cleanup.

After rebuilding the host, restart the service (macOS launch agent shown; Linux
uses the environment-specific systemd user unit). Stop only test hosts you started; do not kill another environment's host. For Dev:

```sh
launchctl kickstart -k gui/$(id -u)/com.pokai.codync.dev.host
pgrep -fl codync-host
```

## iOS app

Install Xcode and XcodeGen, then generate the project from its source. The Xcode project holds the `iOS`, `Widgets` and `NotificationService` targets plus the macOS `Screen` helper the desktop app bundles:

```sh
xcodegen generate --spec apps/project.yml
xcodebuild build -project apps/Codync.xcodeproj -scheme "iOS Dev" -configuration DevDebug -derivedDataPath build/dd -destination 'platform=iOS,id=<device-id>'
```

Use `iOS Dev` for development and `iOS` for production, with a connected device or simulator in Xcode. Do not edit `project.pbxproj` directly. Keep normal simulator signing: Clerk uses Keychain, and unsigned simulator builds can fail initialization with OSStatus -34018.

Install the device build and replace the old iPhone process (substitute its device ID):

```sh
xcrun devicectl device install app --device <device-id> "build/dd/Build/Products/DevDebug-iphoneos/Codync Dev.app"
xcrun devicectl device process launch --terminate-existing --device <device-id> com.pokai.Codync.ios.dev
```

Unlock the phone when required. Build, install, launch, and visual inspection are separate checks; report which actually completed.

## Component checks

| Component | Commands | Requirements |
| --- | --- | --- |
| Host | `cd host && cargo build` | rustup; `rust-toolchain.toml` pins the version CI uses (a Homebrew `rust` ahead of rustup on PATH ignores it) |
| Host checks | `cd host && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | Local agent credentials are unnecessary for unit tests |
| iOS Kit | `cd apps/ios/Kit && xcodebuild test -scheme CodyncKit-Package -destination 'platform=iOS Simulator,id=<sim-id>'` | Swift 6 / Xcode; iOS-only package |
| Desktop | `cd apps/desktop && npm ci && npm run typecheck && npm test` | Node and npm; CI uses Node 24 |
| Cloud | `cd cloud && npm ci && npm test && npm run typecheck` | Node and npm; CI uses Node 24 |
| Cloud integration | `cd cloud && env -u CODYNC_CLOUD npm run e2e` | Built host; see [Cloudflare testing](cloudflare-testing.md) |
| APNs relay | `cd relay && npm ci && npm test && npm run typecheck` | Separate package from `cloud/` |

### Screen helpers

Enable the repository's staged-file checks once per checkout:

```sh
git config core.hooksPath .githooks
```

When a screen helper changes, the pre-commit hook checks Linux Rust formatting
and parses the macOS Swift sources (on macOS). The platform builds remain in
GitHub Actions: Linux requires GStreamer development packages, and macOS builds
the `Screen` Xcode target against WebRTC.

Host development: `cargo run --manifest-path host/Cargo.toml -- serve`. Avoid competing with an installed host on port 19222; isolated tests should use a temporary `CODYNC_HOME` and another port. Stop test hosts when finished.

## Visual checks

Follow [UI conventions](../design/ui-conventions.md) for native toolbar behavior and [widget design](../design/mobile-widgets.md) for previews. Widget previews live in `apps/ios/Widgets/WidgetPreviews.swift`.

Versions are defined in `apps/project.yml` (`MARKETING_VERSION`), `host/Cargo.toml` and `apps/desktop/package.json`; keep them equal (the desktop release workflow fails when the app and host differ), then regenerate the Xcode project.

## iOS releases (Xcode Cloud)

The Xcode Cloud workflow "Release" on the product "iOS" has no automatic start condition (its own tag trigger never fired); it only runs when started through the API, and it archives the `iOS` scheme in Release with deployment preparation **TestFlight and App Store** (the "Internal Testing Only" option produces builds App Review can't take) and uploads it to App Store Connect. Xcode Cloud overrides the build number with its own counter (`CI_BUILD_NUMBER`, configured under Xcode Cloud settings → Build Number in App Store Connect), so `CURRENT_PROJECT_VERSION` in `project.yml` is only the local default. The checked-in `apps/Codync.xcodeproj` is what the cloud builds: regenerate it whenever `project.yml` changes, and commit `apps/Codync.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved` whenever a package version changes (Xcode Cloud never resolves packages itself; copy `apps/ios/Kit/Package.resolved` there if Xcode wrote it to `apps/ios/Kit/`). Xcode Cloud reads public package repositories without any connection. The `Submit iOS` GitHub workflow (`.github/workflows/ios-submit.yml` → `tools/asc-submit.py`, secrets `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_PRIVATE_KEY`) runs on every `v*` tag (the tags Auto Tag pushes after a version bump on `main`). It compares the tag with the last version sent to the App Store: when nothing the iPhone app is built from changed (`apps/ios/` including `apps/ios/Kit/`, `apps/shared/`, the package pins, or `apps/project.yml` beyond its version lines), iOS skips that release, so host- or desktop-only releases neither build nor touch a version in review. Otherwise it starts the Xcode Cloud archive on the tag, waits for that build to finish processing and submits it for App Review, released automatically after approval. The latest version wins: a version still *Waiting for Review* is pulled back, renamed and resubmitted with the new build; one already *In Review* or approved is left alone (rerun the workflow with the version once it is out). *What's New* comes from the version's `## <version>` section in `apps/ios/WhatsNew.md` (one `### <locale>` per App Store language, written in the version bump); without one, the `### zh-Hant` / `### en-US` bullets under `## What's New` in the PR template are collected from every PR merged since the last App Store version (changes pushed to `main` without a PR add nothing); with neither, empty fields get a generic "Bug fixes and improvements" line. Running `Submit iOS` by hand with a version ships it even without iPhone changes. `python3 tools/asc-submit-test.py` checks the PR parsing and the change detection. `DRY_RUN=1 tools/asc-submit.py <version>` prints the writes without making them.
