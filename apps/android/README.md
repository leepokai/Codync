# Codync Android

A native Kotlin/Compose phone client using the existing host's encrypted channel
and account APIs. It follows the iPhone app screen for screen (layout, wording,
motion and the Liquid Glass look), built from the iOS sources in
`apps/ios/Views` and `apps/ios/Kit/Sources/CodyncUI`. What's still missing is in the
[parity and acceptance checklist](../../docs/plans/android-app.md).

## Build

Use JDK 21, Android SDK 37 and Build Tools 37.0.0. Set `JAVA_HOME` and
`ANDROID_HOME`, or set `sdk.dir` in ignored `local.properties`.

```sh
cd apps/android
./gradlew :core:test :app:testDebugUnitTest :app:assembleDebug :app:lintDebug
./gradlew :app:assembleRelease
adb install -r -d app/build/outputs/apk/debug/app-debug.apk
```

Stop the old app before rebuilding. Upgrade with `adb install -r -d`, using the
original package and signing key; retain device data and identity (`-d` lets a debug
build from an older version replace a newer one). The checksum-pinned
Gradle wrapper is checked in. `versionName` follows `apps/project.yml` and
`versionCode` is derived from it (2.24.0 → 2024000). Feature PRs do not bump product versions.

Release builds are signed with the Google Play upload key when `codyncUploadKeystore`
and `codyncUploadKeystorePassword` are set (`codync.local.properties` or `-P`), and stay
unsigned otherwise. `./gradlew :app:bundleRelease` makes the bundle Play takes; releases
reach Play through the `Submit Android` workflow ([Android releases](../../docs/guides/development.md#android-releases-google-play)),
with the release notes in `WhatsNew.md` here.

## Account and push configuration

A clean checkout builds `com.pokai.Codync.android.dev` (Codync Dev) or
`com.pokai.Codync.android` (Codync). Debug/release read their account configuration from
`apps/shared/Config/dev.plist` and `main.plist`. The default push relay comes from
`CodyncKit/Client/SharedStore.swift`.

Firebase client configuration is deployment-specific and excluded from Git (the
official one comes from the `codync-auth` Firebase project: a CI secret, and
`codyncGoogleServicesFile` locally). Without it the app builds, pairs and chats;
Account and State explain that push is unavailable. It does not request notification permission or schedule FCM
registration in that configuration. Foreground task status remains available.

For a private deployment, create ignored `apps/android/codync.local.properties`.
Override cloud, Clerk and push relay **together**; partial tuples fail the build.
Command-line Gradle properties take precedence over this file.

```properties
codyncApplicationId=com.example.codync
codyncAppName=Codync Local
codyncCloudUrl=https://cloud.example.com
codyncClerkPublishableKey=YOUR_MATCHING_CLERK_PUBLISHABLE_KEY
codyncPushRelayUrl=https://push.example.com
codyncGoogleServicesFile=/absolute/path/to/google-services.json
```

The Firebase client file must contain the final package names, including `.dev`
for debug. Its matching service account belongs only in the push Worker's
`FCM_SERVICE_ACCOUNT` secret, never in the app. Before installing a private APK,
check its generated `BuildConfig`, package, Firebase project and certificate
against the saved deployment configuration. Compilation alone does not verify
which backend the app will use.

## Pair and use

Run `codync-host pair` on the computer and scan its code or paste the resulting
`codync://pair` link (`codync-dev://pair` for Codync Dev, which pairs only with a
development host). Setup shows the Mac and Linux install paths, as on iOS. Pairing links
and one-time codes remain in memory. Account login discovers computers; each
host must still approve access.

This increment supports **one paired computer per account context**. Remove the
current pairing in Computers & settings before pairing another. Multiple-computer
support is tracked in the checklist. Accounts have separate device keys, mirrors,
files, image caches and composer drafts.

The roster shows activity, previews, unread counts, groups and pin/hide actions;
touch and hold a bot, then move it, to reorder (the order syncs through the host). Cached bots remain readable offline and are labeled as saved. An
unreachable computer is distinguished from an empty online roster. Chat supports
final replies, Markdown/code, permissions, reactions, quote replies (touch and hold a
message, then Reply), history and Full conversation.

Text and private attachment drafts survive process death separately for each bot. Sends retain their nonce through durable queue commit and retry.
Uncertain sends require explicit retry with the same ID. Text can wait in the
encrypted relay mailbox; files require a live computer. Files are capped at 100 MiB each and uploaded in 384 KiB chunks. Sharing
uses a temporary copy rather than exposing the private cache.

## Ownership and storage

- `app`: Compose screens, Android lifecycle, Keystore-wrapped identity,
  account-scoped storage, files, notifications and widgets.
- `core`: JVM models, crypto, pairing, RPC and transport. The host retains bot,
  agent, memory, routine, connector and other product logic.
- `design`: Codync's design system on Android, mirroring CodyncUI: `Palette`,
  `Type` (Inter, since SF Pro ships only on Apple platforms), `Motion`, glass
  (`Glass.kt`, Kyant backdrop: blur, lens refraction and highlight standing in for
  Liquid Glass), sheets, menus, dialogs, controls and avatars. `Symbol("sf.name")`
  draws the Material Symbols Rounded stand-in for each SF Symbol the iOS app names
  (`python3 tools/symbols.py` regenerates them; SF Symbols may not ship on Android).
  Screens use these components, never Material widgets beyond `Text`.

Ed25519/X25519 seeds use Android Keystore AES-GCM wrapping; no global crypto
provider is replaced. Local data is excluded from backup and device transfer.
The mirror commits updates, send reconciliation and the event cursor together.
Context retirement joins writers before changing identity or closing databases.
A changed host identity produces an error rather than silently trusting new keys.

Direct HTTP/WebSocket connections use Codync's encrypted channel. Cloud URLs
require HTTPS. Minimum API is 29 and target API is 36; declared minimums do not
establish device acceptance. Targeting API 37 requires local-network permission
work.

## Validation

```sh
./gradlew :app:assembleDebugAndroidTest
ANDROID_SERIAL=emulator-5580 ./gradlew :app:connectedDebugAndroidTest
```

Use a disposable emulator/application for ordinary instrumentation: it includes
local reset and credential fixtures. Do not run it against a personal phone's
saved accounts. Mutating live suites require `fixtureComputerId` matching an
isolated fake-agent host. Shared vectors come from
[remote-relay-vectors.json](../../docs/reference/fixtures/remote-relay-vectors.json).

`LiveChatTest` uses `liveHost=true` and a bot named `Android chat acceptance`
running [fake_agent.py](../../host/tests/fake_agent.py). `LiveDraftTest` uses `draftPhase=prepare` and `restore` with an OS process
stop between phases. File acceptance verifies a multi-chunk round trip byte for
byte. `LiveRosterTest` offers separately opted-in read-only inspection of an
existing connection; it is not the ordinary reset suite.

The emulator-only `-PcodyncReleaseTests=true` variant signs an R8 acceptance build
with the debug key; ordinary release builds take only the upload key. Physical camera,
keyboard/TalkBack, provider login, background push, voice and release acceptance
are separate [checklist](../../docs/plans/android-app.md) gates.

## Dependencies and assets

Versions are pinned in [libs.versions.toml](gradle/libs.versions.toml). Terminal
packages, hashes and MIT notices are in
[vendor-manifest.json](app/src/main/assets/terminal/vendor-manifest.json).
`python3 tools/vendor-terminal.py` regenerates them from integrity-checked npm
archives using pinned esbuild. The terminal exposes only a constrained bridge
and explicit HTTP links. Vosk/JNA and the checksum-verified English model have
notices in [voice-manifest.json](app/src/main/assets/licenses/voice-manifest.json).
That speech prototype does not establish full voice parity.

AndroidX, Compose, CameraX, Glance, WorkManager, Firebase and desugaring artifacts
come from Google Maven. Other dependencies use the configured repositories.
AndroidX native libraries, Vosk and JNA are upstream artifacts; no custom native
crypto or camera library is introduced. WebRTC comes from the webrtc-sdk build of libwebrtc.

| Dependency | Version | Source/license and role |
| --- | --- | --- |
| Gradle | 9.6.0 | [Gradle](https://gradle.org/releases/), Apache-2.0; checksum-pinned wrapper |
| Android Gradle plugin | 9.4.1 | [Compatibility](https://developer.android.com/build/releases/agp-9-4-0-release-notes), Apache-2.0; SDK 36 build |
| Kotlin and Compose compiler | 2.4.20 | [Kotlin](https://kotlinlang.org/docs/releases.html), Apache-2.0 |
| Compose BOM | 2026.09.00 | [AndroidX](https://developer.android.com/develop/ui/compose/bom/bom-mapping), Apache-2.0; UI components |
| AndroidX Activity / Lifecycle | 1.12.2 / 2.10.0 | [AndroidX](https://developer.android.com/jetpack/androidx/releases), Apache-2.0; platform lifecycle |
| Coroutines / serialization | 1.10.2 / 1.9.0 | [Kotlin libraries](https://github.com/Kotlin), Apache-2.0; concurrency and JSON |
| OkHttp | 4.12.0 | [Square](https://square.github.io/okhttp/), Apache-2.0; WebSockets and test server |
| Bouncy Castle | 1.86 | [BC Java](https://www.bouncycastle.org/download/bouncy-castle-java/), [BC license](https://www.bouncycastle.org/licence.html); lightweight Ed25519/X25519/ChaCha20-Poly1305 |
| JUnit / AndroidX Test | 4.13.2 / runner 1.7.0 / ext 1.3.0 | [JUnit](https://junit.org/junit4/), EPL-1.0; [AndroidX Test](https://developer.android.com/training/testing), Apache-2.0 |
| CommonMark and GFM extensions | 0.30.0 | [CommonMark Java](https://github.com/commonmark/commonmark-java), BSD-2-Clause; selectable Markdown, tables and strikethrough |
| Coil | 3.4.0 | [Coil](https://github.com/coil-kt/coil/releases/tag/3.4.0), Apache-2.0; bounded image loading and caching |
| Java API desugaring | 2.1.5 | [Android desugaring](https://developer.android.com/studio/write/java11-default-support-table), GPL-2.0 with Classpath Exception; CommonMark runtime support |
| Clerk Android | 1.1.11 | [Clerk SDK](https://github.com/clerk/clerk-android), MIT; native account sessions and browser/provider authentication |
| Firebase BOM / Google Services plugin | 34.19.0 / 4.5.0 | [Firebase Android](https://firebase.google.com/support/release-notes/android), Apache-2.0; FID registration and data-only FCM delivery |
| WorkManager | 2.12.0 | [AndroidX Work](https://developer.android.com/jetpack/androidx/releases/work), Apache-2.0; bounded background registration retries |
| Glance | 1.2.0 | [AndroidX Glance](https://developer.android.com/jetpack/androidx/releases/glance), Apache-2.0; responsive native app widgets |
| CameraX | 1.6.2 | [AndroidX Camera](https://developer.android.com/jetpack/androidx/releases/camera), Apache-2.0; lifecycle-bound preview/analysis, minimum API 23, optional camera permission |
| ZXing Core | 3.5.4 | [ZXing releases](https://github.com/zxing/zxing/releases/tag/zxing-3.5.4), Apache-2.0; bundled Java QR decoding without a network/model download |
| xterm / fit / web-links | 6.0.0 / 0.11.0 / 0.12.0 | [xterm.js](https://github.com/xtermjs/xterm.js/tree/6.0.0), MIT; bundled terminal rendering and explicit HTTP links, with each license included in the assets |
| Vosk Android / English model | 0.3.75 / small-en-us-0.15 | [Vosk](https://github.com/alphacep/vosk-api), Apache-2.0; bundled offline recognition and checksum-pinned model |
| JNA | 5.18.1 | [Java Native Access](https://github.com/java-native-access/jna/tree/5.18.1), Apache-2.0 option with notice; native speech binding |
| WebRTC (webrtc-sdk) | 125.6422.07 | [webrtc-sdk/android](https://github.com/webrtc-sdk/webrtc), BSD-3-Clause; remote screen video and input channels, OpenAI Realtime voice |
| Kyant backdrop / shapes | 2.0.1 / 1.2.1 | [AndroidLiquidGlass](https://github.com/Kyant0/AndroidLiquidGlass), Apache-2.0; glass blur, lens and highlight (RenderEffect API 31+, RuntimeShader API 33+), continuous-corner shapes |
| Inter | 4.x variable | [Inter](https://rsms.me/inter/), OFL-1.1; the app's type face in `design/src/main/res/font` |
| Material Symbols Rounded | upstream SVGs | [Material Design Icons](https://github.com/google/material-design-icons), Apache-2.0; vector stand-ins for SF Symbols |
| esbuild (asset regeneration only) | 0.28.2 | [esbuild](https://esbuild.github.io/api/#target), MIT; transforms newer syntax for older WebViews, never included as an APK binary |
