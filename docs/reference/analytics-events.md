# Analytics events

Every event Codync records, who sends it and what it carries. The rules (feature use only,
never content; one switch per device; identity) are in [usage analytics](../features/analytics.md).
A property is a count, a boolean, a public catalog name or one of the values listed here; a
value outside the list is `other`. Add an event here in the same change that sends it.

Common properties on every event: `env`, `app_version`, `$os`, `$lib`, `$session_id`.

## Senders

| `$lib` | Sender | Path |
|---|---|---|
| `codync-host` | The host, for every client's API calls, its own start, phone pairing and connections, and the events the desktop app and terminal UI report through `track` | `host/src/analytics.rs`, `host/src/analytics/` |
| `codync-ios` | The iPhone app | `apps/ios/Kit/Sources/CodyncKit/Analytics/` (`AnalyticsEvent.swift`), `CodyncUI/AnalyticsTracking.swift` |
| `codync-android` | The Android app | `apps/android/.../Analytics.kt`, `AnalyticsEvent.kt` |
| `codync-desktop` | The desktop app's main process, only for host setup failures (no host to report through) | `apps/desktop/src/main/analytics.ts` |
| `web` | codync.dev, cookieless | `web/app/analytics.tsx` |

Host events also carry `computer` (the computer's key) and `arch`. Host events from API calls
carry `client`: `local` (desktop app, terminal UI, CLI) or `remote` (a phone), and for
`remote` the phone's `platform` (`ios`, `android`, `macos`).

## `track`: what the desktop app and terminal UI report

`POST /api/track` (loopback only) takes `{"event": "<name>", ...properties}`. The host accepts
only the events and properties below (`analytics::Reported`, a typed enum) and drops anything
else, so a client can't send content by mistake. Every reported event carries `app`
(`desktop`/`tui`, `desktop` when missing). The desktop app keeps what it reports before the
host answers and sends it once the host is up.

## Onboarding and conversion

| Event | Sent by | When | Properties |
|---|---|---|---|
| `app_opened` | host `track` (desktop, TUI), iOS, Android | Desktop: each launch, once the host answers. TUI: each start. Phones: each return to the foreground. | desktop `at_login` (opened by the login item); phones `notifications` (`allowed`/`denied`/`not_asked`), `widgets` (count on the Home Screen) |
| `window_opened` | host `track` (desktop) | The chat window is shown after being hidden (tray, dock, shortcut, second launch). A long-running menu bar app opens it many times per launch. | — |
| `host_started` | host | Each start of the host process | `first_run` (the first start of a new install; `open_store` marks a new host id with an empty `analytics_version`), `updated_from` (the previous version, when it changed), `service` (started by launchd, systemd or the Windows runner: `XPC_SERVICE_NAME`, `SYSTEMD_EXEC_PID`, `CODYNC_SERVICE`) |
| `host_setup_failed` | desktop main | The desktop app couldn't get a working host | `stage` (`missing_binary`/`install`/`verify`/`not_responding`/`environment`) |
| `onboarding_step_viewed` | iOS, Android | A first-run screen appears | `step` (`welcome`/`install`/`scan`) |
| `onboarding_completed` | host `track` (desktop), iOS, Android | Desktop: the welcome screen is finished. Phones: the first-run screens are finished and the tabs appear (a computer paired, signed in to an account, or skipped). | desktop `signed_in`; phones `via` (`paired`/`account`/`skipped`) |
| `computer_access_finished` | host `track` (desktop) | The *Computer access* step is left | `ready` (screen recording and control granted), `onboarding` |
| `sign_in_started` | host `track` (desktop), iOS, Android | A sign-in button is pressed | `provider` (`apple`/`google`/`hosted`) |
| `sign_in_failed` | host `track` (desktop), iOS, Android | Sign-in ended without an account | `provider`, `reason` (`cancelled`/`timeout`/`verification`/`error`) |
| `signed_in` | host `track` (desktop), iOS, Android | Signed in to a Codync account | `provider` |
| `signed_out` | host `track` (desktop), iOS, Android | Signed out | — |
| `pairing_code_created` | host (`pairing`) | A pairing QR code is made (desktop, terminal UI, `codync-host pair`), including each new one an open pairing screen makes when its code expires: count persons, not events | `client` |
| `pairing_failed` | iOS, Android | A pairing attempt fails | `reason` (`not_pairing_link`/`outdated_host`/`invalid`/`expired`/`unauthorized`/`unreachable`/`timeout`/`upgrade_required`/`other`) |
| `computer_paired` | iOS, Android | The phone reached a computer | `via` (`qr`: scanned or pasted on the scan screen; `link`: a pairing link opened from outside; `account`), `computer` |
| `device_paired` | host | A phone was added: a pairing code redeemed or an account request approved | `via` (`qr`/`account`), `platform`, `automatic` (approved without a tap) |
| `access_requested` | iOS, Android | The phone asks an account computer for access | — |
| `access_request_failed` | iOS, Android | The request ended without access | `reason` (`denied`/`expired`/`cancelled`) |
| `access_request_denied` | host (`decideAccessRequest`) | The computer's owner denied a phone | `client` |
| `device_revoked` | host (`revokeDevice`) | A phone's access was removed | `client` |
| `phone_connected` | host | A paired phone connects, at most once per phone per UTC day | `platform` |
| `notifications_answered` | iOS, Android | The system notification prompt was answered | `granted`, `source` (`settings`/`widgets`/`automatic`) |
| `notification_opened` | iOS, Android | A Codync notification was tapped | `kind` (`done`/`needs_input`/`failed`) |
| `star_prompt_answered` | host `track` (desktop) | The GitHub star sheet was answered or closed | `ask` (`first`/`second`), `starred` |
| `link_opened` | host `track` (desktop), iOS, Android | An outbound link was opened (`install_command`: an install command copied) | `link` (`github`/`docs`/`issues`/`discord`/`app_store`/`demo`/`tailscale`/`privacy`/`support`/`install_command`/`product_hunt`) |

## Using Codync

| Event | Sent by | When | Properties |
|---|---|---|---|
| `screen_viewed` | host `track` (desktop, TUI), iOS, Android | A screen, sheet or tab is shown | `screen` (see *Screens*) |
| `bot_created` | host | `createBot` | `kind` (`bot`/`group`), `agent`, `model` or `members` (count) |
| `bot_updated` | host | `updateBot` | `fields`: the settings that changed, as the bot config's own camelCase keys (`model`, `permission`, `pinned`, `modelOptions`, `fastMode`, …), never their values |
| `bots_reordered` | host | `reorderBots` | — |
| `bot_deleted` | host | `deleteBot` | — |
| `message_sent` | host | `send` (a resend counts once) | `kind`, `agent`, `reply` (whether it quotes a message), `attachments` (count) |
| `turn_stopped` | host | `stop` | — |
| `turn_finished` | host | An agent's turn ends: once per agent it ran on, so a turn that fell back once and then answered sends a `failed` and a `done` | `agent`, `outcome` (`done`/`stopped`/`failed`/`needs_setup`/`incomplete`; `needs_setup` is a failure whose `error` is `sign_in` or `not_installed`: the user has a step to take, nothing broke), `stop_reason`, `source` (`user`/`routine`/`group`/`bot`/`host`: the lead opening a call), `seconds`; when failed or needs_setup, `error` (`not_installed`/`timeout`/`exited`/`no_model`/`sign_in`/`agent_error`/`io`/`other`) and, for `agent_error`, `rpc_code` and `agent_reason` (the adapter's own category, else one the host reads from the message, which is never sent: `session_ended`/`rate_limit`/`quota`/`overloaded`/`context`/`model`/`network`/`provider`/`unknown`) |
| `agent_fell_back` | host | A turn moves on to the bot's next agent ([agent fallbacks](../features/agent-fallback.md)) | `agent` (the one that couldn't answer), `reason` (`usageLimit`/`outOfCredits`/`rateLimited`/`overloaded`/`signedOut`/`notStarting`/`modelMissing`) |
| `session_reset` | host | `newSession` | — |
| `permission_answered` | host | `respondPermission` | `choice` (`allow_once`/`allow_always`/`reject_once`/`reject_always`/`other`) |
| `reaction_added` | host | `react` | — |
| `handoff_answered` | host | `editHandoff`, `keepConversation` | `action` (`edit`/`keep`) |
| `routine_saved` | host | `saveRoutine` | — |
| `routine_toggled` | host | `setRoutineEnabled` | `enabled` |
| `routine_deleted` | host | `deleteRoutine` | — |
| `routine_run` | host | `runRoutine` | — |
| `routine_retried` | host | `retryRoutineRun` (the retry button on a failed run's notice) | — |
| `voice_call_started` | host `track` (desktop), iOS, Android | A voice call starts, with the engine it really uses after any fallback | `engine` (`on_device`; `openai`/`gemini` realtime; `cloud`: cloud speech to text and back) |
| `voice_call_quality` | host `track` (desktop), iOS, Android | One engine of a call is replaced or the call ends | `engine` (as `voice_call_started`), `ready_ms` (milliseconds until the provider confirmed the session and the microphone delivered; left out if it never was), `reconnects`, `stuck` (`none`/`connecting`/`reconnecting` over 20 s/`speaking`: playback stalled after the voice's turn), `failure` (`none`/`microphone`/`connection`/`provider`) |
| `voice_call_finished` | host | `endCall` (older apps `logCall`) | `seconds` |
| `remote_screen_opened` | host | `screenPrepare` (each connection, reconnects included) | — |
| `screen_taken_over` | host | `screenTakeover` (taking control, not handing it back) | — |
| `connector_installed` | host | `installConnector`; a Composio app once its connection is `active` | `connector` (catalog name, `composio` or `custom`) |
| `connectors_imported` | host | `importConnectors` | `count` |
| `connector_removed` | host | `removeConnector` | — |
| `connector_signin_started` | host | `connectorSignIn` | — |
| `skill_installed` | host | `installSkill` | `custom` |
| `skill_removed` | host | `removeSkill` | — |
| `skill_written` | host | A bot's `save_skill` (`chatCall`) | `change` (`new`/`update`) |
| `artifact_published` | host | A bot's `publish_artifact` (`chatCall`) | `change` (`new`/`update`), `card` (whether its card went into the bot's chat), never its title |
| `credential_saved` | host | A key, login or vault is saved (clearing one isn't) | `kind` (`login`/`onepassword`/`connector`/`composio`/`voice`/`agent_env`) |
| `agent_setup_started` | host | `agentSetup` (an agent's install or sign-in terminal) | `agent` |
| `agent_signed_in` | host | `agentAuthenticate` | `agent` |
| `setting_changed` | host, iOS, Android | A setting is switched | `setting` (host: `remote_access`/`remote_access_url` (`on`: a relay address is saved, never the address)/`approval` (`on`: approved without the code)/`automatic_updates`/`remote_screen`/`cloud_assistants`; phones: `push`/`live_activities`), `on` |
| `host_update_installed` | host | `installHostUpdate` | — |
| `network_joined` | host | `networkCreate` / `networkJoin`: the computer is in a network of the user's computers | `via` (`create`/`invite`), never a name, invite or connector address |
| `network_changed` | host | `networkInvite` / `networkPromote` / `networkRemove` / `networkRecover` / `setNetworkRules` / `networkLeave` | `action` (`invite`/`promote`/`remove`/`recover`/`rules`/`leave`), never a computer's name, the invite, the recovery code or a rule |

## Screens

`screen_viewed` names one screen per value, the same on every client that has it:

| `screen` | iOS / Android | Desktop | Terminal UI |
|---|---|---|---|
| `bots` | Bots tab | — (always shown) | — |
| `state` | State tab | — | — |
| `chat` | a conversation opened | a conversation opened | a conversation opened |
| `trace` | full conversation | full conversation | full trace |
| `bot_details` | bot details | details panel | settings page |
| `memory` | bot memory (Android: the card in bot details, once it scrolls into sight) | memory tab | memory overlay |
| `routines` | routines (iPhone: a routine's editor, the list being part of bot details) | a routine's editor | routines overlay |
| `marketplace` | extensions | extensions | marketplace overlay |
| `settings` | Settings tab | settings | — |
| `computers` | Computer management, from Manage computers or a link | computers & devices | — |
| `usage` | usage | usage | usage overlay |
| `account` | account sheet | account panel | — |
| `pairing` | pair a computer after setup (first run: `onboarding_step_viewed`) | pair a phone | pair overlay |
| `voice_settings` | voice chat settings | voice chat settings | — |
| `remote_screen` | remote screen | — | — |
| `computer_access` | — | computer access | — |
| `help` | — | — | keys overlay |
| `library` | a chat's Library (iPhone: the details tab; Android: its sheet) | library tab | library overlay |
| `artifact` | an artifact's viewer | an artifact's viewer | — (opens in the browser) |
| `network` | network sheet (Network on a computer's card in Manage computers) | — (part of `computers`) | network sheet |

## Funnels

The *Onboarding & conversion* dashboard in PostHog reads these steps, each per `$os`:

- **Desktop**: first `app_opened` (or `host_setup_failed`) → `onboarding_completed` →
  `computer_access_finished` → `pairing_code_created` → `device_paired` → `message_sent` with
  `client = remote` (Activated).
- **iPhone / Android**: `onboarding_step_viewed` `welcome` → `install` → `scan` →
  `computer_paired` → `message_sent` with `client = remote` on that `computer`.
- **Star**: `star_prompt_answered` with `starred` ÷ all `star_prompt_answered`, per `ask`.
- **Website**: `$pageview` → `download_clicked` (per `platform`) → the first `app_opened` that day
  (counts only, no link between a visitor and an install).

## Website

codync.dev counts visits without cookies or anything stored on the visitor's device
(posthog-js `cookieless_mode: 'always'`, which needs the project's *cookieless server hash mode*;
without it PostHog drops these events): `$pageview`, `download_clicked` (`platform`:
`mac`/`windows`/`linux`/`ios`; `linux` and the Homebrew `mac` are copies of the install command),
`link_opened` (`link`). One document-level click listener in `web/app/analytics.tsx` reads each
link's `href`, so a new link is counted without code; the film counts as `demo` when focus moves
into its frame.

## iPhone App Store review invitations

These events use the existing iPhone sender, identity, environment, version and analytics
opt-out. No conversation content, message nonce, bot ID or review text is included.

| Event | When | Properties |
| --- | --- | --- |
| `review_opportunity` | An eligible, viewed result is followed by returning to Bots | `completed_interactions` (count), `policy: v1`, `trigger: return_to_bots` |
| `review_cancelled` | An opportunity is abandoned before StoreKit | `reason: background / interaction / unavailable`, `policy: v1` |
| `review_requested` | The persisted cooldown is consumed and StoreKit is about to be called | `completed_interactions` (count), `policy: v1`, `trigger: return_to_bots` |
| `review_link_opened` | The person taps Rate on the App Store in Settings | `source: settings` |

`review_requested` does not mean the system displayed a prompt or the person submitted a
rating. StoreKit does not provide those outcomes. The [invitation policy](../features/ios-review-invitation.md)
describes timing, exclusions, and cooldowns.

The iPhone also reports `link_opened` with `link: issues` for its independent **Report an issue** action.
