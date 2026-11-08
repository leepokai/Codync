# OpenBot Mobile inventory on Codync Android

Started 2026-11-07 from OpenBot Mobile's changelog 1.0–1.4.7 (Android releases; iOS-only
entries get an Android adaptation instead of the iPhone solution). Codync already covers most
of that inventory; this page tracks only what is being added or adapted, in the owner's order,
and never by rewriting screens that already work.

Rules: additive only, Codync Android components first, OpenBot non-UI code second, and a host
method when the phone would otherwise own logic. No APK is built while the owner holds it; the
gate is `./gradlew :core:test :app:testDebugUnitTest :app:compileDebugKotlin`.

## 1. Resilience and message actions (M9, M3)

Already present, verified, no work: message delete (all entries, owner-only host model),
forward to a channel, quote on the user's own swipe, per-chat draft text, mirror restored
before connect (`openMirror`), sign-out erasure (`LocalStore.erase`), bounded image decode.

| Item | Status | Files |
| --- | --- | --- |
| Delete a message from its long-press menu | Present | `ChatRows.kt`, `AppStoreBots.kt`, host `deleteEntry` |
| Mark unread: the message and those after it, until the chat is opened | Done | host `markUnread`, `Hub::mark_unread`, `Store::set_read_rev`, `Bot.readRev`, `AppStore.markUnread` |
| NEW line above the first unread message, in the chat color, growing from the label | Done | `ChatItem.NewLine`, `NewLineRow`, `withUnreadLine`, thread view |
| Read aloud and Stop reading, speeds 0.75–2× at the pitch kept | Done | `MessageSpeech.kt`, `ReadingSpeedSheet.kt`, shared `VoiceSettings.rate` |
| Several pictures as a stack and a paged full-size viewer, one clip per player | Done | `MediaMosaic.kt`, `MediaViewer.kt`, `AttachmentList.kt` |
| SVG shown as an image (Coil SVG decoder, decoded as vector, saved as file) | Done | `Conversation.kt`, `AttachmentList.kt`, `MainActivity.kt` |
| MP4 played in the viewer (system player, poster tile) | Done | `VideoPlayer`, `MediaViewer.kt` |
| Unread counts include thread replies until the thread is read | Done | host `Store::unread`, `Hub::mark_unread` compatibility |

## 2. Reconnect, memory and battery

| Item | Status | Files |
| --- | --- | --- |
| Retry with 20 % positive jitter | Done | `AppStoreConnection.kt` |
| A new attempt at once when the app returns | Done | `AppStore.setForeground` |
| 15 s cap on the greeting, the roster, history and a thread, then the error | Done | `AppStoreConnection.kt`, `AppStoreChat.chatRead` |
| Large images stay bounded (decode by target size; never a full bitmap) | Present | `AttachmentFiles.thumbnail`, `ComposerAttachment.kt`, `PhotoAnnotation.kt` |
| Rotation, unfold and window resize keep the screen (no recreation) | Present | `AndroidManifest.xml` `configChanges` |
| The blinking launch mark draws no frames between blinks | Done | `LaunchSplash.kt`, `CharacterAvatar(blink)` |
| Resting faces stay still (only working and needs-you faces move) | Present | `CharacterAvatar.kt` |
| Encrypted copy of agents and chats, deleted on sign-out | Partial: the mirror lives in `noBackupFilesDir`, which is private but not encrypted; the key is Keystore-owned | `LocalStore.kt`, `MirrorDatabase.kt` |

## 3. Support (M4)

Adapted, not copied: iPhone and Android had no Support page, so Android leads it. The log is the
app's own events, kept on the phone (app cache, never backed up), redacted, capped at 5,000 and
paged 100 at a time when read; Apple-style crash reporting was already in `CrashReports.kt`.

| Item | Status | Files |
| --- | --- | --- |
| Settings → Support with Discord and GitHub links | Done | `SettingsView.kt`, `SupportView.kt`, `AnalyticsEvent.kt` |
| App event log: connection steps, each request with its result and errors | Done | `SupportLog.kt`, `EncryptedChannel.onCall`, `AppStoreConnection.kt` |
| Show log, newest first, 5,000 events, Show older events | Done | `SupportView.kt` |
| Save log file through the share sheet, and Clear log with a confirmation | Done | `AppStoreSupport.kt`, `SupportView.kt` |
| Log kept across restarts, deleted only from the page | Done | `SupportLog.kt` |

Parity owed: the iPhone app has no Support page yet (`apps/ios`).

## 4. Queue, then the rest

Not started, in order:

- **M2**: workflow editor (Condition model and effort, WhatsApp numbers), channel Cancel task.

Done from M2:

| Item | Status | Files |
| --- | --- | --- |
| A routine can start from a WhatsApp message or group mention | Done | host `routines::events`, `RoutinesView.kt` (catalog from the host) |
| A WhatsApp chat as an output | Done | host `routines::outputs`, `Routine.kt`, `RoutinesView.kt`, desktop `routine.ts` |
- **M5/M6**: Voice pause slider and reply speed choice, Plush/Flat avatar style, calmer resting
  faces and group float.
- **M1/M7**: marketplace connectors (Coinbase, RevenueCat, TickTick, Proton Pass, Higgsfield),
  the Apps connector list with status, marketplace Load more, the spent usage window under a
  model.
- **M8 and X1–X7**: queue panel (Edit, Steer, Delete), pins shared with the desktop, paragraph
  direction, task-thread member names, and the Android adaptations of the iOS-only entries.

## Validation

Each merge-ready chunk runs the Gradle gate plus emulator walks (phone, tablet, foldable) and
TalkBack on the X items; regressions touch chat, push, channels, voice, marketplace, profiles
and settings. APK builds wait for the owner.
