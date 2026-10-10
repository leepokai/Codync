# Usage analytics

Opt-in product analytics in PostHog (US Cloud, project "Codync App", id 649427). Every
device asks once and sends nothing until the user agrees; the answer can be changed in
Settings anytime. The public promise lives in `web/app/privacy/page.tsx`: keep it true.

## Rules

- **Only feature use, never content.** No message text, prompts, code, file names, paths,
  bot names, credentials or agent output. Properties are counts, booleans and public
  catalog names (agent, model, connector catalog name; a custom connector is just `custom`).
- **Consent per device.** The computer's owner decides for that computer (host kv
  `analytics_enabled`, `null` until asked); each iPhone decides for itself
  (`UserDefaults` `analyticsEnabled`). Turning it off drops anything still queued.
- **Identity.** Signed in to a Codync account: the Clerk user id, with a person profile.
  Otherwise a random install id (the host id on a computer, a UUID on the iPhone) and no
  person profile. The first signed-in event sends `$identify` with `$anon_distinct_id` so
  earlier events join the account.
- **No IP or location**: the project discards client IPs and every event sets `$geoip_disable`.
- `env` is `main` or `dev`; filter dev out in PostHog.

## Where events come from

- **Host** (`host/src/analytics.rs`): `api::dispatch` maps successful API calls to events
  (`analytics::observe`), so every client is covered once: `bot_created`, `bot_updated`,
  `bots_reordered`, `bot_deleted`, `message_sent`, `turn_stopped`, `session_reset`, `permission_answered`,
  `routine_saved`, `routine_deleted`, `routine_run`, `voice_call_finished`,
  `remote_screen_opened`, `connector_installed`, `skill_installed`. `client` is `local`
  (desktop app, terminal UI) or `remote` (phones). Batches go to `/batch/` every 30 s.
- **Desktop and terminal UI** report app-level events through the loopback-only `track`
  method (`app_opened`, `onboarding_completed`, `signed_in`, `signed_out`) and set consent
  with `setAnalytics`; `hello` carries `analytics: true | false | null`.
- **iPhone** (`CodyncKit` analytics client, no SDK) sends its own app-level events
  (`app_opened`, `onboarding_completed`, `computer_paired`, `signed_in`, `signed_out`,
  `voice_call_started`) straight to PostHog. Widgets and the notification extension never send.

## Asking

All clients show the same prompt ("Help improve Codync", *Share usage* / *Don't share*)
when onboarding is done and the device hasn't answered, which covers new users right after
the welcome screen and existing users once after updating. The App Store privacy label
declares *Product Interaction*, linked to the user, not used for tracking.
