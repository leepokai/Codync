# Android parity and acceptance

Status: **in development, merged into `dev`** (#151, #152, #154, #155), not released yet. The
Android app is a 1:1 port of the iPhone app: each screen
is built from its iOS source (`apps/ios/Views`, `apps/ios/Kit/Sources/CodyncUI`) with the
Android `design` module (see the [Android README](../../apps/android/README.md)). Product
logic stays in the host. This checklist tracks what's still different and what still needs
acceptance on real devices.

## Coverage

"Ported" means the iOS screen and flow exist on Android and were walked through on an
emulator against a real host; device acceptance is listed separately.

| Area | Android | Remaining |
| --- | --- | --- |
| Welcome/pairing | Ported: welcome crew and chat glimpse, Install (Mac/Linux) and Scan pages with the camera on the page, paste link, Tailscale note, sign-in | Physical optical scan, camera permission denial |
| Accounts/access | Ported: account sheet and switcher, Computers & settings (connection status, badge color, Revoke this phone's access, older copies removed from the account), account computer access with its code, access banner, `codync-dev://computers` and `codync-dev://plugins` | Provider browser round trip on a device; Revoke and older copies against a signed-in account (covered by cloud request tests only); one computer per phone ([multiple computers are future work](../architecture/overview.md)) |
| Roster | Ported: rows, menus, pin/hide/delete, groups, drag to reorder (host-synced order) and TalkBack Move up/down, computer header, pull to refresh (also in Computers & settings) | Multiple computers |
| Conversation | Ported: bubbles, Markdown/code, permissions, reactions, quote replies, Full conversation, bot-to-bot exchange lines and their conversation, routine notices that open their routine, files a bot shares (verified download, then the share sheet), sent images full size, pull down to reconnect, jump to latest, read receipts for every conversation on screen | Physical keyboard and TalkBack |
| Composer | Ported: files/photos/paste (groups too, image thumbnails, Paste only when the clipboard holds files), `/` commands (Fast mode, skills listed fresh on `/`), @mentions in groups, call/send/stop/interrupt button | Dictation ([voice call](../features/voice-call.md)): the mic, its recording pill, the engine menu beside it (On device, or OpenAI / Gemini transcribed on the user's key with *Transcribing…*) and the *Couldn't dictate* dialog |
| Bot and group editors | Ported: avatar, profile, agent and model options, Fast mode, workspace folder picker, permissions, activity, connectors, skills (with their details) | — |
| Memory | Broken since 2.18.0: the screen calls removed host methods (see [Not ported yet](#not-ported-yet)) | Port the read-only memory screen from `MemoryView.swift`, `MemoryFilesView.swift` and `MemoryInfoSheet.swift`, without add, edit, pin or forget |
| Routines | Ported: list with switches, editor (schedule, webhook panel, run settings), test run, delete | Live provider/routine acceptance |
| Extensions | Ported: agents (install, sign-in, setup terminal), registry connectors, Composio apps, skills, installed list, custom connector, own skill, credentials, chat connection request card | Real provider OAuth, 1Password |
| Voice | Ported: call bar (provider name, fallback notice), Voice chat settings (engine, mode, models, voice, minutes this month), on device (the call's language through Android's on-device recognizer, Vosk English as the fallback, replies read by an offline voice for their language), OpenAI Realtime (WebRTC), Gemini Live, Speech mode, fallback to on device, group calls | Both providers with a real key on a device; a live microphone on a device |
| State/widgets | Ported: Widget (with the animated Add a widget guide), Live update and Status bar pages, Usage sheet, Choose the look; Provider usage, Usage limits, Bots and Team widgets with launcher Edit widget and picker previews (Android 15+); the Bots Quick Settings tile (iOS's Control Center control) | Physical launcher refresh |
| Notifications | Ported: alerts as conversations with the bot's face (a group's face and its speaking member), conversation shortcuts, what happened ("Task complete") as sub text | Android's conversation layout doesn't draw sub text; background delivery needs FCM in the deployment |
| Live updates | Android 16 Live updates (`ProgressStyle`), the status bar chip on Android 16 QPR1 (API 36.1) | Background updates need FCM in the deployment |
| Version check | `VersionMismatch.MIN_HOST`, the host's `minApp`, the update card in the roster and in place of the composer | Set `MIN_HOST` to the first release that ships Android support when it's cut |
| Remote screen | Ported: viewer from Computers, the chat header (watch a bot, then take over) and `codync-dev://screen`; WebRTC H.264, trackpad and direct touch, long-press drag, two-finger scroll, pinch zoom streaming the visible region, keyboard with IME and modifier keys, clipboard, displays, take over/hand back, Rotate | Pinch, IME composition and a hardware keyboard on a device; TURN relay path and renewal |
| Analytics | Ported: PostHog feature events with the Settings switch (and its sheet), crash reports sent on the next launch | — |
| Packaging | Debug, unsigned R8 release, CI, optional Firebase/private configuration | Play signing, distribution, `versionCode` policy |

## Gates before merge into dev

- [x] `./gradlew :core:test :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest :app:assembleRelease :app:lintDebug` passes (CI and the pre-commit hook run it).
- [x] Every screen above walked through on an API 36 emulator against a disposable host, in light and dark mode, side by side with the iOS simulator.
- [x] Live update and its status bar chip on an API 36.1 emulator.
- [x] `AppStore.kt` split by responsibility (clean-code size rule), undecodable events stop after one rewind and show *Update this app*, timeouts surface as errors.
- [ ] Instrumentation suite run on an emulator: only single classes so far (`PlatformSpeechTest`, `AlertNotificationTest`, probes), through `am instrument` because Gradle's connected run uninstalls the paired app.

## Not done yet

Where the Android app still falls short, as of 2026-10-18. Everything else in the table above
was walked through on an emulator against a real host.

### Not ported yet

- **Bot memory (2.18.0).** The host's memory moved out of Engram ([bot memory](../design/bot-memory.md)). The
  memory screen still calls the removed `memoryDetail`, `reviewMemory`, `exportMemory` and `importMemory`
  (`MemoryModel.kt`, `MemoryCard.kt`, `LiveMemoryTest.kt`), so it shows an error. Missing too: the two layers
  (yours and the bot's) with each `MEMORY.md` line and who wrote it, memory notes, the dream log with its info
  sheet, the harness's own memory switch with its info sheet (2.19.0) and the handoff card. Since 2.21.0 the memory screen is read-only
  (the user changes memory by telling the bot), so the port drops its add and edit controls. Port them from the
  iPhone (`MemoryView.swift`, `MemoryFilesView.swift`, `MemoryInfoSheet.swift`, `HandoffRows.swift`).

### Not verified (needs an account, a key, a device or the real Mac)

| What | Code | What it needs |
| --- | --- | --- |
| Revoke this phone's access (also on Remove of a computer approved through the account); removing older copies of the computer from the account | `core/.../CloudClient.kt` (`revokeAccess`, `removeComputer`), `AppStoreAccount.kt`, `SettingsView.kt` | A signed-in account on the emulator or a phone; only request tests cover it |
| Push delivery (alerts, background Live update refresh) | `PushService.kt`, `AppStoreNotifications.kt` | A Firebase project and the push Worker's FCM secret (owner's go-ahead). The notification look is covered by `AlertNotificationTest` |
| OpenAI Realtime, Gemini Live and Speech mode with a real key | `WebRtcPeer.kt`, `core/.../OpenAIRealtimeEngine.kt`, `GeminiLiveEngine.kt`, `CloudSpeechEngine.kt` | A provider key on the paired computer; the fallback to on-device voice is verified |
| Remote screen: Chinese IME composition, a hardware keyboard, the TURN relay path and its credential renewal | `ScreenKeys.kt`, `ScreenSession.kt`, `ScreenView.kt` | A computer it's safe to type into, and a signed-in host for the relay; viewing, trackpad and pinch zoom are verified |
| On-device voice with a live microphone | `PlatformSpeech.kt`, `LocalVoiceAudio.kt` | A phone; on the emulator the recognizer was fed the speech fixture |
| Camera QR pairing, launcher widget refresh, foldables, TalkBack end to end | `QrViewfinder.kt`, `CodyncWidgets.kt` | Physical devices; portrait lock with the rotating screen viewer, 130% text and TalkBack's Move up/down were checked on the emulator |

### Different from the iPhone on purpose or by platform

- **Notification subtitle**: "Task complete" / "Response needed" / "Task failed" is the
  notification's sub text, which Android's conversation layout doesn't draw (`PushService.kt`).
- **Bots tile** refreshes when Quick Settings opens, not live; between two bots that need you
  equally it picks the pinned one, the iPhone the most recent (`core/.../WidgetFeed.kt` `leadBot`).
- **Shared files**: leaving the chat during a download skips the share sheet when it finishes
  (the card shows *Downloaded*); shared copies are deleted after a day (`AppStoreFiles.kt`,
  `AttachmentFiles.kt`).
- **Pull to refresh** on the roster and in Settings uses the chat's indicator; iOS uses the system one.
- **`computers` / `plugins` links** open over the current tab; iOS switches to Bots first.
- **On-device voice**: languages other than English need Android 13+ and Android's on-device
  recognizer; it never falls back to online recognition (iOS may use Apple's servers).
- **Jump to latest** is an opaque disc: it sits in the scrolling content, so there's no blur under it.
- **Not ported**: the computer filter's hide toggle and *This computer was removed* (one computer
  per phone on Android, and removing it reloads the app), *Create template* (nothing opens it on iOS).
- **Multiple computers**: one per phone ([future work](../architecture/overview.md)).

## Before a Play release

- [ ] Physical upgrade keeps the saved account, pinned keys, bots and history.
- [ ] Physical portrait/landscape/foldable, keyboard, large text and TalkBack.
- [ ] Provider sign-in/switching, failed sign-out and revocation.
- [ ] Firebase project and the push Worker's `FCM_SERVICE_ACCOUNT` secret (owner's go-ahead).
- [ ] Host release gating for Android: like iPhones and the App Store, a release that raises
  `minApp` must wait while paired Android phones can't get that version from Play
  ([compatibility](../reference/compatibility.md)).

## Shared deployment

The host accepts Android pairing (older hosts refuse it, and the app asks to update
Codync on the computer) and counts Android event streams as connected phones, so
pushes are held while one is open. The cloud accepts Android device records through
the `0004_android_devices.sql` migration. The push Worker needs the matching Firebase
service account as an external secret; the client configuration contains no private key.

![Android roster](../screenshots/android-roster.png)

![Android account](../screenshots/android-account.png)
