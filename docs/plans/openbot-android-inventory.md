# OpenBot Mobile on Codync Android: what stays and what is missing

Inventory of the owner's `PROMPT ANDROID · INVENTARIO COMPLETO` (changelog 1.0–1.4.7, source
`C:\Users\Edwin Villeda\Documents\openbot-nightly\apps\mobile`, 1.4.7). Two lists, checked against
this tree on 2026-11-09: **A** is already in Codync Android and is never rewritten, **B** is the
milestones in the owner's order, landed or missing, with the Android shape of each, **C** is the
iOS-only entries and how each one is done on Android instead. `openbot-mobile-parity.md` keeps the
running record of what is landed.

Rules that hold for every item: Android only, additive, Codync Android code first and OpenBot's
non-UI code second (`modules/android-notifications`, `app-lock`, `composer-paste`,
`quick-actions`, `share-files`, `speech/SpeechCapture.kt`; `src/app/**` and `src/features/*.tsx`
are rewritten), host method when the phone would otherwise hold logic, Codync components and a
motion on every open, no native control outside the documented exceptions. OpenBot's origin is
left as a one-line comment where code is copied.

Order of work: A9 and A3 are already landed; the list below is the owner's: M2 and M4, then M5
and M6, then M1 and M7, then M8 and the X adaptations. M2 (threads) landed on 2026-11-09; M4 is
next.

## A. Already in Codync Android: keep

Verified by code search on 2026-11-09; the files named are the ones that must not be replaced.

| Kept | Where |
| --- | --- |
| Agent and channel chats, attachments, reactions, replies, message queue, dictation | `ChatScreen.kt`, `ChatRows.kt`, `QueuedMessages.kt`, `Composer*.kt` |
| Voice conversation and reading with the phone's engine | `VoiceChat*.kt`, `MessageSpeech.kt` |
| Push, per-agent notifications, widget, launcher shortcut | `PushService.kt`, `AlertNotification.kt`, `Widgets/`, `LauncherShortcuts.kt` |
| Marketplace with apps, agents, skills and groups; CLIs; per-agent accounts with mail | `Marketplace*.kt`, `AgentAccountsSection.kt`, `AgentAccountPicker.kt` |
| Browser profiles per bot, message delete, chat colour, WhatsApp with QR and bot | `BrowserProfile*`, `AppStoreBots.kt` (`deleteEntry`), `ChatColor*`, `WhatsAppSheet.kt` |
| Real usage, n8n, rail, canvas with manual routines, settings | `Usage*`, `N8n*`, `Rail*`, `RoutineFlowCanvas.kt`, `SettingsView.kt` |
| Translations, base RTL and shared styles | `L10n.kt`, `LanguageRoot.kt`, `design/` |

Message delete, mark unread, the NEW line, send to a channel, read aloud with speed, image stacks
with a viewer and mosaic, SVG, MP4, text selection and quoting your own message on swipe are also
in already (M3, done); drafts per chat and the encrypted mirror deleted on sign-out are in (M9,
done). See `openbot-mobile-parity.md` §1–2 for each file.

## B. Missing: to add

### M2 Threads, done

Threads are a chat's side conversations: one per message (Slack's shape: the message *is* the
thread's root), or one with a root of its own opened from the list. Their lines stay out of the
chat, which shows a chip under the message ("3 replies", who, when, how many are new).

| Item | Status | Files |
| --- | --- | --- |
| The agent menu with all threads and a new thread | Done | `ThreadsSheet.kt`, `ChatScreen.kt` |
| A thread's sheet: title, rename, archive, delete, attachments | Done | `RepliesView.kt`, `ThreadHeader.kt`, host `renameThread`/`archiveThread`/`deleteThread` |
| An unread count per thread, cleared on open | Done | host `thread_reads`, `Store::thread_unread`, `Hub::mark_thread_read` |
| "Branch from here" and "Branched from…", with a tap back to the chat | Done | `ChatRows.kt`, `RepliesView.kt` |
| The chip under a message with a thread | Done | `ChatRows.kt` `ThreadChip`, host `Store::decorate_thread` |

Host: thread roots stopped being prefix-bound (a message id, or `thread:<uuid>` for one opened
from the list), schema 9 adds `threads`; `thread`, `threads`, `createThread`, `renameThread`,
`archiveThread` and `deleteThread` answer them, `send` takes a `threadId`, and a live `thread`
event carries each chip's summary. Every client has the same feature in its own shape: iOS
(`ThreadSheet`, `ThreadsView`), desktop (`RepliesView`, `ThreadsCard`) and the TUI (`t` on a
picked message, `T` for the list).

### M4 Voice, expanded

- Reading with a local engine (Supertonic 3) or ElevenLabs with a key, samples and deletion.
- Recognition with Parakeet v3 (670 MB, 25 languages) or Scribe, downloadable.
- Pause before sending (1–10 s) and speed: present, keep. Send with an empty box starts and ends
  the conversation.

Nothing of the engines exists yet (`supertonic`, `elevenlabs`, `parakeet` appear in neither the
app nor the host).

### M5 Agent settings on the server

Advanced: parallel threads, start new chats, browser profile, backups, folders and exclusions,
plan before long tasks, compact. Editing instructions with a change warning. Fast mode and the
per-model limit in Runtime.

Present in part: fast mode (11 sites), browser profile. Missing: `parallel threads`, `backups`,
`exclusions`, `compact`.

### M6 Server from the phone

Multiple provider accounts, Kimi and Hermes, custom providers (Ollama, LM Studio, OpenAI-
compatible), ACP registration and own ACP agents. Vault. Server connectors and own MCP. Audit,
key rotation with double signature, import agents, transfer a server. Create and delete local
servers; folders and moving chats.

Missing today: `rotate key`, `import agent`, `transfer server`.

### M1 Workflows on the phone

Step editor (name, trigger, add, move, duplicate, delete), GitHub repo/event/labels/filters/action
with recent events and "run with the last", Slack and Telegram for owners and admins, Discord
settings, per-step and loose run, Stop per step, versions, make one current, pin to the rail.

Codync's workflow is the flow canvas, so a step is a canvas node/link: the link model and per-link
model exist; the trigger surface, versions, duplicate and per-step run do not.

### M7 Marketplace and connectors

The named apps (Higgsfield, TickTick, Coinbase, Robinhood, RevenueCat, ClickUp, Proton Pass,
Trello, Neon, Prisma, Netlify, Railway, Atlassian, Todoist, Hugging Face), server connectors under
Apps with state and MCP, "Load more" on filtered lists, Hermes with Download.

Already landed from this list: Coinbase, RevenueCat, TickTick, Proton Pass, Higgsfield, the Apps
list with status and paging, the spent usage window.

### M8 Interface

Arabic/Hebrew mirroring per paragraph with LTR code: done. Flat and plush avatars and colours:
done. Missing: compact bubbles, the day separator, row actions in a sheet on long press, nested
folders with their path, synced drag and pin, unread counter, server settings in short pages,
Support with Discord and the log, long-press confirmations with the link and options inside the
card.

## C. iOS-only, with the Android adaptation

| iOS entry | Android adaptation |
| --- | --- |
| X1 iPad, iPhone Duo, wide screens | WindowSizeClass: two panes from 600 dp, draggable border, SlidingPaneLayout if it fits; Jetpack WindowManager puts list and chat on either side of a fold; rail only in width. |
| X2 Live Activity, Dynamic Island, lock screen | Progressive NotificationCompat notice with the agent's face, promoted with setShortCriticalText on Android 16+, kept on the lock screen, relaunched by a ForegroundService if removed and cut at 8 h, unread count on the notice. |
| X3 Native bar and swipe back | Own Material header with the previous screen's hierarchy, Predictive Back on 14+ through OnBackPressedDispatcher with the view following the finger; swiping a message still replies. |
| X4 Share from other apps | ACTION_SEND and ACTION_SEND_MULTIPLE with ChooserTarget of recent agents, text, image and file, opens the chat with the attachment or sends straight if the agent is pinned; with the lock on, opens with the content pending. |
| X5 Accessibility and confirmations | TalkBack labels and order, the same long press, and with TalkBack a tap asks again inside the card; card buttons with roles and state. |
| X6 Push and watch | FCM token per device with retry, face on the notice; Wear OS notice with image, two lines and count, never relaunching activities. |
| X7 Icon and system gestures | Monochrome icon with `android:monochrome` and dynamic theme; own BottomSheet with drag, no system contextual menus. |

Already present: X2 (live update chip and ongoing notice), X3 (launcher shortcuts), X4 (Direct
Share), X5 (app lock), X6 (foldable master-detail), X7 (face as the notice's large icon). The
Arabic and RTL entry is done.

## Validation

`./gradlew :core:test :app:testDebugUnitTest :app:compileDebugKotlin` with no APK; emulator walks on
phone, tablet and foldable; TalkBack on the C items; regressions across chat, push, channels,
voice, marketplace, profiles and settings.
