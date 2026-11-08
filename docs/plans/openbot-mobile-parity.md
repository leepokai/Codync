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
| Encrypted copy of agents and chats, deleted on sign-out | Done: every mirror payload is AES-GCM sealed under the account's Keystore key, which sign-out deletes with the folder | `ScopeKey.kt`, `MirrorSealer.kt`, `MirrorDatabase.kt`, `LocalStore.kt` |

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

Parity: the iPhone app has the same page and log (`SupportView.swift`, `SupportLog.swift`), fed by the
same two hooks in its transport.

## 4. Queue, then the rest

Kept in the owner's order:

- **M2**: workflow editor (a step's model and effort, WhatsApp numbers) — done.

Done from M2:

| Item | Status | Files |
| --- | --- | --- |
| A routine can start from a WhatsApp message or group mention | Done | host `routines::events`, `RoutinesView.kt` (catalog from the host) |
| A WhatsApp chat as an output | Done | host `routines::outputs`, `Routine.kt`, `RoutinesView.kt`, iOS `RoutineOutput` + editor, desktop `routine.ts` |
| A routine can run in a thread | Done | host `Routine::thread`, `routines::mod`, `Run in a thread` on the three clients |
| Link a number with a pairing code, with the QR code still offered | Done | host `whatsapp::pair`, bridge `pair`, `WhatsAppSheet.kt`, desktop `WhatsAppSheet.tsx`, iOS `WhatsAppSheet.swift` (marketplace pin and a computer's WhatsApp row) |
| Channels on the phone: pair someone to a Telegram bot, take them out, see Discord, Slack and GitHub | Done | `ChannelsSheet.kt`/`AppStoreChannels.kt`; iOS `ChannelsSheet.swift`, `HostClient+Channels.swift`, `BotStore+Channels.swift`, Settings row |
| Cancel task on a stopped or failed task | Done | Codync has no channel tasks: a paused turn card and a failed routine notice stand in. host `pause::close` and `routines::retry::cancel` (one `cancelTask` method), `PausedTurnCard.kt`, `RoutineNotice`, desktop `ChatRows.tsx`/`RoutineNotice.tsx`, TUI `/cancel` and `E` |
| A step's own Model and Effort | Done: a link carries a model and its options into the next bot's one turn, then the bot's own come back. Codync's canvas has no Condition step type; a link is the step. host schema 8, `chat::team::ModelOverride`, `agent::bot::model_override`, Android + desktop link panels; the iPhone has no flow canvas yet | `store/flows.rs`, `routines/flow`, `ModelOverride`, `RoutineFlowDetail.kt`, `FlowInspector.tsx` |

Every M2 item is done; the iPhone has no flow canvas yet, so a link's model is set from Android or
the desktop app.

- **M5/M6**: voice pause slider, reply speed choice, Plush/Flat avatar style, calmer resting
  faces and group float.

Done from M5/M6:

| Item | Status | Files |
| --- | --- | --- |
| Voice pause is a slider, not three fixed choices | Done | `VoicePause`, `StepSlider.kt`, `VoiceChatSettings.kt` |
| Reading speed is six stops, 0.5× to 2×, shared with read aloud | Done | `VoiceRates`, `ReadingSpeedSheet.kt` |
| Plush or Flat bot avatar, on all three clients, widgets included | Done | `AvatarStyle`, `CharacterAvatar` (`flat`), iOS `Appearance.swift`, desktop `Avatar.tsx` and its tray icons |
| Calmer resting faces and a group float | Done | half-lidded idle eyes in each `CharacterAvatar`; drift in `GroupAvatar` |

- **M1/M7**: marketplace connectors (Coinbase, RevenueCat, TickTick, Proton Pass, Higgsfield),
  the Apps connector list with status, marketplace Load more, the spent usage window under a
  model.

Done from M1/M7:

| Item | Status | Files |
| --- | --- | --- |
| Coinbase, RevenueCat, TickTick, Proton Pass and Higgsfield in the catalog | Done | `host/src/market/openbot/apps.json`, `tools/import-openbot-catalog.mjs` |
| Proton Pass's short session: its token is re-entered into a login run before each start | Done | `host/src/market/login.rs`, `Connector.login`, `install.rs`, `api/marketplace.rs` |
| An Apps connector list with its status, paged like the registry | Done | `ComposioViews.kt`, iOS `ComposioViews.swift`, desktop `ComposioSection.tsx`; key setup sheet on all three |
| Key-setup steps above the fields | Done | `MarketplaceSheets.kt`, `MarketplaceDetailSheets.swift`, `AppSheet.tsx` |
| The spent plan window under the model, with its reset, gone once it resets | Done | `UsageReport.spentWindow`, `AgentModelPicker` on all three, `spentNote` |

- **M8 and X1–X7**: queue panel (Edit, Steer, Delete), pins shared with the desktop, paragraph
  direction, task-thread member names, and the Android adaptations of the iOS-only entries.

Done from M8:

| Item | Status | Files |
| --- | --- | --- |
| A queue strip above the composer with Edit, Steer and Delete | Done | host `Cmd::{Steer,EditQueued,DeleteQueued}`, `queue.rs`, `queue_tests.rs`, `api`, `QueuedMessages.kt`, desktop `QueuedStrip.tsx` + `queued-preview.ts`, iOS `QueuedMessages.swift`, TUI `/steer` `/unqueue` `/edit` |
| Pins shared with the desktop | Already present | host pins and all clients |
| Each chat paragraph in the direction of its own text; code always left to right | Done | `MarkdownText.kt` `byText()`, `ChatRows.kt`, desktop `Markdown.tsx` `dir="auto"`, iOS `ParagraphDirection.swift` |
| Task-thread member names | Done | `ChatRows.kt` `AuthorLabel` read `name` as "A deleted bot" |

iOS-only entries, each adapted to Android:

| Item | Adaptation | Status | Files |
| --- | --- | --- | --- |
| X1 Arabic and right-to-left layout | A real Arabic interface language, the whole layout mirrored, and each paragraph in its own direction | Done | `L10n.kt` `AppLanguage.Ar`, `LanguageRoot.kt`, `ar.json`; iOS `AppLanguage.ar` + `layoutDirection`; desktop `ar` + `document.dir` |
| X2 Live Activities | Android 16 live updates with a status bar chip; an ongoing notification before that | Already present | `LiveUpdateGallery.kt`, `PushService.task` |
| X3 App-icon quick action | Launcher shortcuts to the pinned agents | Already present | `LauncherShortcuts.kt` |
| X4 Share-sheet agent targets | Long-lived Direct Share shortcuts that send into an agent | Done | `BotShares.kt`, `LauncherShortcuts.kt`, `AppStoreShare.receiveShare` |
| X5 App lock (PIN, then face or fingerprint, four-try backoff, ten → sign out) | The same lock, with the same PIN backoff and sign-out | Done | `AppLock.kt`, `SensitiveAccess.ask`, `MainActivity` |
| X6 iPhone Duo and foldable split | Master-detail from 600 × 360 dp with the rail | Already present | `MainNavigation.isExpandedWindow`, `configChanges` |
| X7 Agent picture in notifications | The face as the notification's large icon | Already present | `AlertNotification.kt` `notificationFace` |

Parity owed by this work: the iPhone app has no chat-channel UI at all (Telegram, Slack, Discord,
WhatsApp), so the pairing code waits for that screen; the Support page, routine outputs and the
`2.39.0` App Store notes are in.

## Validation

Each merge-ready chunk runs the Gradle gate plus emulator walks (phone, tablet, foldable) and
TalkBack on the X items; regressions touch chat, push, channels, voice, marketplace, profiles
and settings. APK builds wait for the owner.
