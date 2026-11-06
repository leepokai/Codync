# Topic 7: security profiles

## What changed, by file

- `host/src/agent/security_profile.rs` (new): `SecurityProfile { ReadOnly, Builder, Operator, Full }` with its id, title,
  sentence, Approvals setting, approval rules, daily token limit, `apply(cfg)` and `describe()`. Reference: Mercury Agent's
  `permission-tiers.ts` (MIT, `THIRD-PARTY-NOTICES.md`); the mapping to approval rules is Codync's.
- `host/src/hub/profile.rs` (new) and `hub.rs`: `settle_profile` applies a profile when it is new to the bot and drops its
  name when the rules or the setting are changed away from it; `set_profile_budget` sets the limit. Called from
  `create_bot` and `update_bot`.
- `host/src/store/model.rs`: `BotConfig.security_profile`. `host/src/api/bots.rs`: `securityProfiles` lists the profiles.
- Desktop: `shared/security-profiles.ts` (types, rules, preview wording; tested), `SecurityProfilePicker.tsx` in the bot's
  settings (new or existing bot), `store.securityProfiles()`.
- Android: `core/SecurityProfile.kt` (decode, rules, preview; tested), `SecurityProfileChooser.kt` in the new bot dialog
  and in the settings, `AppStore.securityProfiles()`.
- Terminal: the new bot dialog has a Profile row (`←→`), and the rules sheet `P` switches profile (saving sends the
  profile; the host writes the rules).
- Tests: `security_profile` (read-only asks to edit and refuses commands; each step up allows more; Full approves all but
  the forbidden commands; apply replaces the setting and rules; the description), terminal (profile cycling, the body, the
  sheet's save body), desktop and Android helpers, and `host/tests/security_profile_e2e.rs`.

## Acceptance

"A Read-only bot asks before editing a file and refuses commands": the e2e makes a bot with `securityProfile: readOnly`
against a real host and a scripted agent. An edit request waits for a card; `run:ls` is refused with no card. Raising the bot
to Full approves `make` but still refuses `rm -rf /`; saving a rule of its own drops the profile's name.

## Decisions where there were two options

- **Rules, not tool names.** Mercury denies named tools; here a profile is a list of per-tool-kind rules plus the setting,
  so it is visible and editable in the rules editor and tested through the one `evaluate`.
- **Operator may run plain commands.** Mercury's operator runs commands inside its path scopes; with no path scopes over
  ACP, an Operator's commands are approved but a chained command still asks, and the forbidden list still applies.
- **Daily limits are part of the profile** (300k, 1M, 3M tokens; Full leaves the bot's own), as the task asked. They are
  shown in the preview; routines pause at the limit, so a person who picks Read-only for a busy bot should know that.
- **Full access is a deliberate choice, not the new default**: new bots still start as before unless a profile is picked.
- **Path scopes do not apply to agents run over ACP** (documented), so none are written.
- **iOS is out of scope** by the user's instruction.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test -j 1 --bin codync-host -- --test-threads=1`, and the e2e tests `security_profile_e2e`, `approval_rules_e2e`, `forbidden_e2e`, `pause_e2e` | 858 passed, 0 failed, 1 ignored; the e2e tests pass (2, 4, 2, 2) |
| Desktop `npm run typecheck`, `npm test` | clean; 392 passed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, BUILD SUCCESSFUL in 4m 38s) |

## Not tested

No screen was seen. No real agent ran under a profile. The daily limits were not exercised against real token use.
