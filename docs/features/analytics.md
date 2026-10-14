# Usage analytics

Product analytics in PostHog (US Cloud, project "Codync App", id 649427), on by default on
every device; *Share usage analytics* in Settings turns it off. It exists to spot bugs sooner
(failed or stopped turns per agent and platform) and to serve users better. The public promise
lives in `web/app/privacy/page.tsx`: keep it true. The info button beside the switch (terminal
UI: *About usage analytics…*) shows the same promise in the app, in one text per client
(desktop `views/settings/AnalyticsSheet.tsx`, `CodyncUI/AnalyticsSheet.swift`,
`host/src/tui/view/analytics.rs`): change all three with the policy.

## Rules

- **Only feature use, never content.** No message text, prompts, code, file names, paths,
  bot names, credentials or agent output. Properties are counts, booleans and public
  catalog names (agent, model, connector catalog name; a custom connector is just `custom`).
  Errors are categories and code locations, never an error's message (see *Error tracking*).
- **Setting per device.** The computer's owner decides for that computer (host kv
  `analytics_enabled`, on unless `0`); each iPhone decides for itself (`UserDefaults`
  `analyticsEnabled`, on unless `false`). Turning it off drops anything still queued. A
  device that said no to the earlier opt-in question stays off.
- **Identity.** Every device gets a person profile. Signed in to a Codync account: the Clerk
  user id. Otherwise a random install id (the host id on a computer, a UUID on the iPhone).
  The first signed-in event sends `$identify` with `$anon_distinct_id` so earlier events join
  the account.
- **Location, never the IP.** The project's GeoIP transformation turns the sender's IP into an
  approximate location (`$geoip_country_name`, `$geoip_city_name`, …), then the project's
  *Discard client IP data* drops the IP before storage.
- `env` is `main` or `dev`; filter dev out in PostHog. Debug host builds (`cargo test`, `cargo run`)
  send nothing: every e2e test starts a fresh host that would count as a new user.

## Where events come from

- **Host** (`host/src/analytics.rs`): `api::dispatch` maps successful API calls to events
  (`analytics::observe_call`), so every client is covered once: `bot_created`, `bot_updated`,
  `bots_reordered`, `bot_deleted`, `message_sent`, `turn_stopped`, `session_reset`, `permission_answered`,
  `routine_saved`, `routine_deleted`, `routine_run`, `voice_call_finished`,
  `remote_screen_opened`, `connector_installed`, `skill_installed`. A resent message (same
  `clientNonce`, e.g. a phone replaying after a reconnect) is not counted again. Bot actors
  report `turn_finished` when an agent's turn ends. `client` is `local` (desktop app, terminal UI)
  or `remote` (phones). Each event carries its own `timestamp` and `uuid` (a resent batch counts
  once) and the computer's key (`computer`, a hash of its id); batches go to `/batch/` every 30 s and once more when the
  host stops.
- **Desktop** reports app-level events through the loopback-only `track` method
  (`app_opened`, `onboarding_completed` when the welcome screen is finished, `signed_in`,
  `signed_out`) and turns sharing on or off with `setAnalytics`; `hello` carries `analytics: true | false`.
  The **terminal UI** reports only `app_opened` (it has no welcome screen or sign-in).
- **iPhone** (`CodyncKit` analytics client, no SDK) sends its own app-level events
  (`app_opened`, `onboarding_completed`, `computer_paired`, `signed_in`, `signed_out`,
  `voice_call_started`) straight to PostHog. Widgets and the notification extension never send.

## Event properties

Besides `env`, `app_version`, `$os`, `$lib`:

| Event | Properties |
|---|---|
| `bot_created` | `kind` (`bot`/`group`), `agent`, `model` or `members` (count) |
| `message_sent` | `kind`, `agent`, `in_thread`, `attachments` (count) |
| `turn_finished` | `agent`, `outcome` (`done`/`stopped`/`failed`/`incomplete`), `stop_reason` (ACP's, else `other`), `source` (`user`/`routine`/`group`/`bot`), `seconds`; when failed, `error` (`not_installed`/`sign_in`/`timeout`/`exited`/`agent_error`/`io`/`other`) and, for `agent_error`, the agent's JSON-RPC `rpc_code` |
| `voice_call_finished` | `seconds` |
| `connector_installed` | `connector` (catalog name or `custom`) |
| `skill_installed` | `custom` |
| `computer_paired` | `via` (`qr`/`account`), `computer` |
| every host event | `client` (`local`/`remote`), `computer`, `arch` |

## Error tracking

Crashes and uncaught errors go to PostHog Error Tracking as `$exception` events with the same
switch, identity and common properties as everything else, plus `process`. No SDK: each client
builds PostHog's generic `custom` frames (oldest first, the failing one last).

- **Host** (`host/src/analytics/errors.rs`): its own panics (a panic hook queues them for the
  analytics task), and `reportError`, loopback only, for the computer's apps. Process `host`.
- **Terminal UI**: its panics, through `reportError` from the panic hook. Process `tui`.
- **Desktop** (`apps/desktop/src/main/error-reports.ts`, `shared/error-report.ts`): uncaught
  errors and rejections in the main process and every window, and windows or helpers that
  crashed (`RendererGone`, `GPUGone`, … with Electron's reason), through `reportError`.
  Processes `desktop-main`, `desktop-renderer`. The renderer is built unminified so function
  names stay readable.
- **iPhone** (`CodyncKit` `CrashReports`): MetricKit's crash diagnostics, delivered on the next
  launch, straight to PostHog with the version that crashed. Frames are `binary + offset`
  (unsymbolicated; Xcode Organizer has the symbolicated crash). Process `ios`.

What leaves the device: the error's type (`Panic`, `TypeError`, `EXC_BAD_ACCESS`), how it was
caught, function names, and file names and line numbers inside Codync's code; an absolute path
keeps only its file name. Never the error's message: only a panic message written as a literal
in the code, an Electron crash reason or a signal name. Release host builds are stripped, so a
host panic is its location only.

Alerts email the owner, from insights on the *Codync users* dashboard (production only): *Any
crash or error in a day* (`$exception` > 0, checked daily) and *Failed agent turns over 15 in an
hour* (`turn_finished` with `outcome = failed`, checked hourly; a normal hour has a few). Error
tracking's own issue alerts (new, reopened, spiking) need a Slack or Discord destination.

## Metrics

Shared filters: `env = 'main'`, internal accounts excluded, UTC days. A *user* is a PostHog
person: the account when signed in, otherwise one install, so an anonymous iPhone and an
anonymous computer are two users until both sign in. Messages are recorded by the computer,
so an anonymous iPhone's chats count for its computer. To count people rather than devices,
join an iPhone's `computer_paired` to the computer's events on `computer` in SQL.

| Metric | Definition |
|---|---|
| Active user (DAU/WAU/MAU) | Sent ≥1 `message_sent` in the period. Any event at all counts as *present*, not active (`app_opened` on the iPhone fires on every return to the foreground). |
| New user | First event of any kind in the period. |
| Activated | Sent ≥1 message from a phone (`message_sent`, `client = remote`) within 7 days of first seen. In the first week of data, 4 of 5 such users came back after 24 h and none of the 6 computer-only ones did. |
| Successful turn rate | `turn_finished` with `outcome = done` ÷ all `turn_finished` with `source = user`, per agent and per `$os`. |
| Frustration rate | (`turn_stopped` + `session_reset`) ÷ `message_sent`, per `$os`. |
| Retained (D1, D7) | Active again on day 1 / any of days 7–13 after their first active day (PostHog retention, `message_sent` both ways). |
| Power user | Active on ≥4 of the last 7 days (stickiness). |
| Feature adoption | Share of WAU with ≥1 of: `voice_call_finished`, `remote_screen_opened`, `connector_installed`, `routine_saved`, `bot_created` with `kind = group`. |
| Up to date | Share of computers active in the period whose last `app_version` is the latest release. |

## Turning it off

No client asks: sharing starts with the first launch. Every client has the same switch,
*Share usage analytics* (iPhone and desktop Settings → Privacy, terminal UI `^k`). The App
Store privacy label declares *Product Interaction* and *Coarse Location*, linked to the user, not used
for tracking (the same two in `apps/ios/Resources/PrivacyInfo.xcprivacy`).
