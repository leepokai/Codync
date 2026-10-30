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

### Replace an installed macOS build completely

For a requested **build and install**, replace the whole app and its running
host, then verify the installed result. Building or copying the app alone does
not finish the task.

1. Quit the selected variant and stop its old host before rebuilding. Preserve
   its data directory and leave production, other variants and sandboxes alone.
2. Run `npm run dist:mac:dev` for Codync Dev (`dist:mac:main` for production).
3. Remove the previous installed app bundle, then copy the complete new bundle
   into `/Applications`; do not merge files into the previous bundle.
4. Explicitly run the **new installed bundled host's** installer, even if its
   launch-agent plist already exists. For Dev:
   `"/Applications/Codync Dev.app/Contents/Resources/codync-host" install --port 19223`.
   A plist left after `launchctl bootout` does not mean the service is loaded;
   opening the app alone can leave it showing **The host isn't running**.
5. Open the installed app. Verify `/health` on the variant's port reports the
   expected environment, version, installed binary path and hash, and verify
   the app reaches its connected main screen. Retry its connection if it opened
   before the host was ready. Do not report installation complete before both
   checks pass.

This sequence records the Dev installation failure on 2026-10-23: the old host
was booted out, the app bundle was replaced, but the service was not reloaded
until the new bundled host's `install` command was run explicitly.

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
connections. ACP adapters run in separate process groups. Stopping an adapter
closes its stdin and gives its group 10 seconds to exit by itself, so a Claude
Code CLI can finish saving a refreshed login. Tools and MCP servers still left
in the group after that are killed, and the host waits for this before it exits.
Processes that detach into their own sessions and unrelated hosts using other
data directories are outside this cleanup.

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
| Host | `cd host && cargo build` | rustup; `host/rust-toolchain.toml` pins the version CI uses (a Homebrew `rust` ahead of rustup on PATH ignores it) |
| Host checks | `cd host && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | Local agent credentials are unnecessary for unit tests |
| iOS Kit | `cd apps/ios/Kit && xcodebuild test -scheme CodyncKit-Package -destination 'platform=iOS Simulator,id=<sim-id>'` | Swift 6 / Xcode; iOS-only package |
| Desktop | `cd apps/desktop && npm ci && npm run typecheck && npm test` | Node and npm; CI uses Node 24 |
| Cloud | `cd cloud && npm ci && npm test && npm run typecheck` | Node and npm; CI uses Node 24 |
| Cloud integration | `cd cloud && env -u CODYNC_CLOUD npm run e2e` | Built host; see [Cloudflare testing](cloudflare-testing.md) |
| APNs relay | `cd relay && npm ci && npm test && npm run typecheck` | Separate package from `cloud/` |

### Rust toolchain

`host/rust-toolchain.toml` and `apps/screen-linux/rust-toolchain.toml` each pin the Rust version for their own crates (`host/crates/*` follow the host's). The two files must name the same version: when you bump one, bump the other, run clippy on both and commit them together. CI steps that install Rust run inside the crate's folder so rustup finds its file.

### Pre-commit checks

Enable the repository's pre-commit hook once per checkout:

```sh
git config core.hooksPath .githooks
```

It runs CI's checks for each part a commit touches, so a failure shows up before the push:
host and the memory crate (`cargo fmt --all --check`, clippy, and the tests with `cargo nextest run --profile ci` as CI
runs them: install it once with `cargo install cargo-nextest --locked`), desktop, cloud,
relay, discord and private-relay (`npm run typecheck`, `npm test`), the Android app (core and app unit tests, and a Kotlin compile of
the app and its instrumentation tests, with JDK 21), Linux screen helper formatting, a parse of the macOS
screen helper's Swift sources (on macOS), and the website's user manual (`npm run check:docs` in `web/`: pages
compile, links and anchors resolve). It checks the staged files only, in a copy under `.git/precommit-snapshot`,
so other agents' uncommitted edits in the same checkout neither break nor rescue a commit; the
host builds into `host/target/precommit`. Its unit tests always run; its end-to-end tests run for the
areas the commit touches (`host_tests` in the hook: routines, network, the send CLI, memory, calls, or a changed test and
the tests sharing a changed fixture), none for areas no end-to-end test covers (the terminal UI, screen, updates,
usage, remote, analytics, service), and all of them for anything else (the hub, store, API, bots, agents and chats,
or the tests' shared helpers). nextest runs them side by side, retries a flaky one, and stops one that hangs
(`host/.config/nextest.toml`); the whole run is stopped after 15 minutes. Host CI runs only on pull requests, so a
host change that skipped end-to-end tests here is fully tested when its PR into `main` opens. The npm checks reuse each package's `node_modules`
(run `npm ci` there first). Each worker package is checked only when its own folder (or the
shared relay vectors) changes. On a merge commit it checks only the files that differ from both
sides (resolved conflicts, or both sides' changes put together); what the merge brings in was checked
when it landed.

CI on a pull request: the host runs `cargo nextest run` with retries (a test that passes only on a
retry shows as FLAKY), iOS Kit retries a failing test up to twice, Android builds the minified release
APK only after a change lands on `dev` or `main`, and the desktop installers are built only when a PR
touches what they're made from (`src/main`, `src/preload`, `src/shared`, build config, tools,
resources, dependencies, `apps/shared/Config`, `apps/screen-macos`, `packaging/`) or targets `main`; every desktop PR still typechecks, tests and builds.

Left to CI: the iOS Kit tests (an iPhone simulator) and the platform builds (Linux screen
helper: GStreamer development packages; macOS: the `Screen` Xcode target against WebRTC).

The macOS helper's socket lifecycle and candidate rule checks run without capture permissions:

```sh
swift test --package-path apps/screen-macos-tests --scratch-path build/dd/screen-macos-tests
```

Host development: `cargo run --manifest-path host/Cargo.toml -- serve`. Avoid competing with an installed host on port 19222; isolated tests should use a temporary `CODYNC_HOME` and another port. Stop test hosts when finished.

## Visual checks

For interface catalogs, supported language choices and translation checks, see [localization](localization.md).

Follow [UI conventions](../design/ui-conventions.md) for native toolbar behavior and [widget design](../design/mobile-widgets.md) for previews. Widget previews live in `apps/ios/Widgets/WidgetPreviews.swift`.

Versions are defined in `apps/project.yml` (`MARKETING_VERSION`), `host/Cargo.toml` and `apps/desktop/package.json`; keep them equal (the desktop release workflow fails when the app and host differ), then regenerate the Xcode project.

## GitHub releases

Greptile reviews every PR into `dev` or `main` when it opens and on each push (`.greptile/config.json`), and posts a *Greptile Review* check beside CI. Before the release PR merges into `main`, check each finding against the code and have the agent that owns that area fix it. `@greptileai review` on the PR asks for a fresh review.

Auto Tag (`.github/workflows/auto-tag.yml`) creates each `v*` release right after pushing the tag, before the host and desktop jobs upload to it. Releases go up without GitHub's *latest* mark: the release gate gives it to the newest fully uploaded release the App Store's iPhone app works with, so downloads, updaters and Homebrew wait for App Review when a release needs a newer iPhone app ([holding releases](../reference/compatibility.md#holding-releases-for-the-app-store)). Its description comes from `tools/release-notes.sh`: the `## Summary` of the PR that merged the version bump into `main` (or the PR's opening text without that heading), else the `feat`/`fix` commits since the previous tag, followed by GitHub's generated PR list and compare link. Write the release PR's Summary for people reading the release page. The same description is posted to the Discord server's #releases channel through the webhook in the `DISCORD_RELEASES_WEBHOOK` secret (a failed post only leaves a warning on the run); each `@login` there whose GitHub account is linked on the Discord Worker becomes a mention that pings them (`DISCORD_MENTIONS_SECRET`, [discord/README.md](../../discord/README.md)).

## codync-mem releases

The memory crate (`host/crates/memory`) has its own 0.x version and ships with the Codync release that carries its changes, through `.github/workflows/memory-crate.yml`:

- A pull request into `main` that changes the crate (its `.github`, the mirror's CI, aside) fails unless the crate's `version` differs from `main`'s.
- The push to `main` mirrors the crate to [leepokai/codync-mem](https://github.com/leepokai/codync-mem) (write deploy key in the `CODYNC_MEM_DEPLOY_KEY` secret), then publishes its version to crates.io unless crates.io already has it. Publishing uses trusted publishing: the crate's Settings → Trusted Publishing on crates.io trusts `leepokai/Codync` with the workflow `memory-crate.yml`, so no token is stored. Both crates also have **Require trusted publishing** on (since 2026-10-26): crates.io refuses a version published with an API token, so a release goes through the workflow or not at all.

To bump, set `version` in `host/crates/memory/Cargo.toml` (patch for fixes, minor for API changes while 0.x) and run `cargo check -p codync-mem` in `host/` to update its `Cargo.lock` entry. A published version can't be deleted, only yanked.

## Crate releases

`codync-network` (`host/crates/network`) ships the same way as codync-mem, through `.github/workflows/network-crate.yml`: a pull request into `main` that changes it must raise its 0.x `version`, and the push to `main` mirrors it to [leepokai/codync-network](https://github.com/leepokai/codync-network) (deploy key in `CODYNC_NETWORK_DEPLOY_KEY`) and publishes the version on crates.io by trusted publishing (`leepokai/Codync`, `network-crate.yml`). Bump with `cargo check -p codync-network` in `host/`. Its `.github` folder is the mirror's own CI and pull-request redirect, inert in Codync.

The mirror only fast-forwards: the crate's commits must reach `main` as they are. Merge a branch that touches a crate (no rebase or squash after its commits were mirrored or published), or the next push to the mirror is refused. 0.1.0 was published and mirrored by hand from `feat/codync-network` on 2026-10-26, ahead of its Codync release.

## iOS releases (Xcode Cloud)

The Xcode Cloud workflow "Release" on the product "iOS" has no automatic start condition (its own tag trigger never fired); it only runs when started through the API, and it archives the `iOS` scheme in Release with deployment preparation **TestFlight and App Store** (the "Internal Testing Only" option produces builds App Review can't take) and uploads it to App Store Connect. Xcode Cloud overrides the build number with its own counter (`CI_BUILD_NUMBER`, configured under Xcode Cloud settings → Build Number in App Store Connect), so `CURRENT_PROJECT_VERSION` in `project.yml` is only the local default. The checked-in `apps/Codync.xcodeproj` is what the cloud builds: regenerate it whenever `project.yml` changes, and commit `apps/Codync.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved` whenever a package version changes (Xcode Cloud never resolves packages itself; copy `apps/ios/Kit/Package.resolved` there if Xcode wrote it to `apps/ios/Kit/`). Xcode Cloud reads public package repositories without any connection. The `Submit iOS` GitHub workflow (`.github/workflows/ios-submit.yml` → `tools/asc-submit.py`, secrets `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_PRIVATE_KEY`) runs on every `v*` tag (the tags Auto Tag pushes after a version bump on `main`). It compares the tag with the last version sent to App Review (a version never sent or pulled back, such as one a failed run left behind, doesn't count; a rejected one does): when nothing the iPhone app is built from changed (`apps/ios/` including `apps/ios/Kit/`, except `WhatsNew.md` and the Kit tests, `apps/shared/`, the package pins, or `apps/project.yml` beyond its version lines), iOS skips that release, so host- or desktop-only releases neither build nor touch a version in review; a release whose host asks for a newer iPhone app (`MIN_APP`) than that version ships anyway, so the release gate never waits for an iPhone app that isn't coming. Otherwise it starts the Xcode Cloud archive on the tag (or on `main` while `main` is the tagged commit: Xcode Cloud hears of a new tag through a GitHub webhook that sometimes never arrives), waits for that build to finish processing and submits it for App Review, released automatically after approval. The latest version wins: a version still *Waiting for Review* is pulled back, renamed and resubmitted with the new build; one already *In Review* or approved is left alone (rerun the workflow with the version once it is out). *What's New* comes from the version's `## <version>` section in `apps/ios/WhatsNew.md` (one `### <locale>` per App Store language, written in the version bump); without one, the `### zh-Hant` / `### en-US` bullets under `## What's New: iPhone` in the PR template are collected from every PR merged since the last App Store version (changes pushed to `main` without a PR add nothing); with neither, empty fields get a generic "Bug fixes and improvements" line. Running `Submit iOS` by hand with a version ships it even without iPhone changes. After each `Submit iOS` run, the `iOS dSYMs` workflow (`.github/workflows/ios-dsyms.yml` → `tools/upload-dsyms.py`, on macOS) downloads that release's archive from Xcode Cloud and uploads its dSYMs to PostHog (secret `POSTHOG_CLI_API_KEY`), so iPhone crash reports are symbolicated; run it by hand with a version after a manual `Submit iOS` or to upload an older one ([iPhone crash tracing](iphone-crash-tracing.md)). `python3 tools/asc-submit-test.py` checks the PR parsing, the change detection, the `MIN_APP` check (all three in `tools/store_release.py`, shared with Google Play) and the archive source. `DRY_RUN=1 tools/asc-submit.py <version>` prints the writes without making them.

## Android releases (Google Play)

The Android app is `com.pokai.Codync.android` in the Play Console account *李博凱* (a personal account: production access needs a closed test with at least 12 testers opted in for 14 days, then an application in Play Console). Google Play re-signs every bundle with its own app signing key (Play App Signing); the build is signed with the upload key `upload` (`~/.codync-release/upload-keystore.p12`, password in `upload.properties` there and in the login Keychain as *Codync Android upload keystore*; Play Console can reset a lost upload key). Gradle signs `release` when `codyncUploadKeystore` and `codyncUploadKeystorePassword` are set (`apps/android/codync.local.properties` or `-P`); otherwise the release bundle stays unsigned. `versionCode` is derived from `MARKETING_VERSION` (`major × 1 000 000 + minor × 1 000 + patch`, so 2.24.0 is 2024000): no counter to keep, and a track's version code names the release it came from. R8 shrinks the release but keeps Codync's own class and method names and line numbers (`proguard-rules.pro`), so the app's PostHog crash reports read without a mapping file; Play takes the mapping from the bundle for its own reports.

The `Submit Android` GitHub workflow (`.github/workflows/android-submit.yml` → `tools/play-submit.py`) is the counterpart of `Submit iOS` and runs on the same `v*` tags. It compares the tag with the version on the Play track: when nothing the Android app is built from changed (`apps/android/` except its tests, the release acceptance build, README and `WhatsNew.md`; `apps/shared/`; the shared string catalogs and relay URL it reads from `CodyncKit`), Android skips that release, unless the track is empty or the tag's host asks for a newer app (`MIN_APP`) than the track's version. Otherwise it builds the signed bundle from the tag with the official Firebase configuration and uploads it to the track (repository variable `PLAY_TRACK`, `internal` until production access), released at once with the release notes from the version's `## <version>` section in `apps/android/WhatsNew.md`, else the `## What's New: Android` bullets of the PRs merged since the track's version, else a generic line (`zh-Hant` becomes Play's `zh-TW`; each language cut to Play's 500 characters at a whole line). Running it by hand takes a version (without the `v`; it builds that tag) and a track and ships even without Android changes; a version Play already has from another track isn't uploaded again but moved to that track, which is how a release is promoted. Secrets: `PLAY_SERVICE_ACCOUNT` (the `codync-play` service account of the `codync-auth` Google Cloud project, invited in Play Console with *View app information* and *Release apps to testing tracks*; add *Release to production* when the app gets production access), `ANDROID_UPLOAD_KEYSTORE` (base64 of the upload keystore), `ANDROID_UPLOAD_KEYSTORE_PASSWORD` and `ANDROID_GOOGLE_SERVICES_JSON`. `python3 tools/play-submit-test.py` checks the version codes, the release notes, the paths that count and the track comparison; `DRY_RUN=1 tools/play-submit.py upload <version> <bundle>` reads the track and prints the writes without making them.

Firebase lives in the same `codync-auth` project, with the Android apps `com.pokai.Codync.android` and `.dev` (one `google-services.json` serves both; the local copy is in `~/.codync-release/`, referenced by `codyncGoogleServicesFile`). The push relay sends with the `codync-fcm` service account (role *Firebase Cloud Messaging API Admin*, Worker secret `FCM_SERVICE_ACCOUNT`). Clerk's mobile SSO allowlist has `clerk://com.pokai.Codync.android.callback` and `.oauth` (production) and the `.dev` pair (development).
