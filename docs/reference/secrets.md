# Secrets

Every key the project uses: where it lives, what it does, and what to do when it leaks or is lost. The repository is
public, so this page holds names and purposes only. Values, where backups are kept, expiry dates and who may use a
cloud computer never go here: they stay in the git-ignored `.env.<env>` files (as values or comments) or the owner's
password manager. Publishable keys (Clerk `pk_…`) and the public halves of signing keys are configuration, not
secrets.

## Rules

- **One key per job.** Before adding a key, look for one here that already does the job. Where one secret has two
  ends (a sender and a verifier), it is one key stored twice, and the table says so.
- **A key leaves with the code that used it.** The change that stops using a key also deletes it everywhere it is
  stored (Worker or GitHub secret, provider console, local copy) and removes its row here. Keys made for tests
  (Boat, InstaBox) are scoped to what the test needs, short-lived, and deleted once expired or replaced.
- **Keys that can't be reissued keep one copy outside their service** (marked *no reissue*): a Worker or GitHub
  secret can't be read back, so the copy lives in `.env.development` / `.env.production`. Keys that can be reissued
  keep no copy: losing one means making a new one.
- **Each environment has its own keys.** Development and production never share one; `relay/` and `discord/` have
  a single deployment and so a single set.
- **Never** in a commit, a log, a command-line argument, an app bundle or a screenshot. Pipe a value into
  `wrangler secret put` or `gh secret set` instead of typing it on the command line.
- This page changes in the same change that adds, renames or removes a key.

## Cloud Worker (`cloud/`)

`codync-cloud` (main, `api.codync.dev`) and `codync-cloud-dev` (dev, `dev-api.codync.dev`), set with
`npx wrangler secret put <name> [--env dev]`. Local runs (`[env.local]`) get throwaway values from the test runner.
Bindings and plain vars: [cloud/README.md](../../cloud/README.md#deploy).

| Secret | Environments | What it does | Lost or leaked |
|---|---|---|---|
| `CLERK_SECRET_KEY` | dev, main | Verifies session tokens (Clerk's JWKS) | Roll it in Clerk, put the new one |
| `CLERK_WEBHOOK_SECRET` | dev, main | Verifies Clerk's `user.deleted` webhook | Roll it on the Clerk webhook endpoint |
| `DEVICE_CERT_KEY` | dev, main | Signs device certificates; hosts compile in its public half (`host/src/environment.rs`) | *No reissue*: a new key needs a host release with the new public half. Copy: `CODYNC_DEVICE_CERT_KEY` |
| `CLOUD_SECRET_KEYS` | dev, main (since 2026-11-09) | Seals the provider keys (Boat, InstaBox) users give Codync Cloud and each cloud computer's keeper vault key (`src/secrets.ts`, `src/cloud-machines.ts`); the first entry seals, any entry opens | *No reissue*: without it every saved provider key is unreadable (users paste theirs again) and every existing cloud computer's vault is locked (delete and recreate those machines, then enter their secrets again). Rotate by putting a new entry first; keep an old one until every machine created under it is deleted and every provider key sealed with it has been saved again. Copy: `CODYNC_CLOUD_SECRET_KEYS` |
| `RELEASE_GATE_TOKEN` | main | GitHub token (*Actions: write*) that dispatches the release gate | Make a new fine-grained token |
| `TURN_KEY_ID`, `TURN_KEY_API_TOKEN` | dev, main | Issue Cloudflare Realtime TURN credentials for remote screen | Make a new TURN key in Cloudflare |

## Push relay (`relay/`)

One deployment, `codync-relay`, for both environments ([relay/README.md](../../relay/README.md)).

| Secret | What it does | Lost or leaked |
|---|---|---|
| `APNS_TEAM_ID`, `APNS_KEY_ID`, `APNS_SIGNING_KEY` | The APNs Auth Key that sends iPhone pushes | Revoke it in Apple Developer, make a new one |
| `FCM_SERVICE_ACCOUNT` | The `codync-fcm` service account that sends Android pushes | Delete its key in Google Cloud, make a new one |
| `TICKET_KEY` | Seals the push tickets phones give hosts | Put a new random one; phones register again on launch |

## Discord Worker (`discord/`)

One deployment, `codync-discord` ([discord/README.md](../../discord/README.md)).

| Secret | What it does | Lost or leaked |
|---|---|---|
| `DISCORD_CLIENT_SECRET`, `DISCORD_BOT_TOKEN` | The Discord app's sign-in and the bot that gives roles | Reset them in the Discord developer portal |
| `GITHUB_CLIENT_SECRET` | The GitHub OAuth app members link their account with | Make a new client secret |
| `GITHUB_TOKEN` | Classic token without scopes, for public reads | Make a new one |
| `GITHUB_WEBHOOK_SECRET` | Verifies the repository's webhook | Change it on both ends |
| `SESSION_SECRET` | Signs the Worker's sign-in sessions | Put a new random one; members sign in again |
| `MENTIONS_SECRET` | Verifies release notes posted to `/mentions`; the same key as GitHub's `DISCORD_MENTIONS_SECRET` (one key, two ends) | Change it on both ends |

## GitHub Actions (`leepokai/Codync`)

Set with `gh secret set <name>`. The workflow that reads each one is in `.github/workflows/`.

| Secret | Workflows | What it does | Lost or leaked |
|---|---|---|---|
| `APPLE_CERTIFICATE_P12`, `APPLE_CERTIFICATE_PASSWORD` | `release-desktop` | The Developer ID certificate that signs the Mac app | Revoke it, make a new one in Apple Developer |
| `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_PRIVATE_KEY` | `release-desktop`, `ios-submit`, `ios-dsyms` | App Store Connect API key: notarizing, submitting to App Review, fetching dSYMs | Revoke it, make a new one in App Store Connect |
| `HOST_UPDATE_SIGNING_KEY` | `host` | Signs host update manifests, for tags and the dev release channel alike (a manifest names its channel's URL); the public half is `packaging/updates/host-public-key.txt` | *No reissue*: a new key breaks updates for every installed host. Copy: `CODYNC_HOST_UPDATE_SIGNING_KEY` |
| `DEV_RELEASES_R2_KEY_ID`, `DEV_RELEASES_R2_SECRET` | `host` | R2 API token (*Object Read & Write* on the bucket `codync-releases-dev` only) that uploads the [dev release channel](../guides/development.md#dev-release-channel) | Delete the token under R2 → Manage API tokens, make a new one, set both again |
| `TAP_TOKEN` | `auto-tag`, `release-gate` | Pushes the version tags and the Homebrew tap | Make a new token |
| `ANDROID_UPLOAD_KEYSTORE`, `ANDROID_UPLOAD_KEYSTORE_PASSWORD` | `android-submit` | The Play upload key that signs Android builds | Ask Play Console for an upload key reset (takes days); keep the file outside the repository |
| `ANDROID_GOOGLE_SERVICES_JSON` | `android-submit` | Firebase configuration for the Android builds | Download it again from Firebase |
| `PLAY_SERVICE_ACCOUNT` | `android-submit` | Uploads builds to Google Play | Delete its key in Google Cloud, make a new one |
| `POSTHOG_CLI_API_KEY` | `ios-dsyms` | Uploads iPhone dSYMs to PostHog | Make a new personal API key |
| `DISCORD_RELEASES_WEBHOOK` | `auto-tag` | Posts release notes to Discord | Delete the webhook, make a new one |
| `DISCORD_MENTIONS_SECRET` | `auto-tag` | Signs the release notes it sends to the Discord Worker's `/mentions`; the same key as its `MENTIONS_SECRET` | Change it on both ends |
| `CODYNC_MEM_DEPLOY_KEY`, `CODYNC_NETWORK_DEPLOY_KEY` | `memory-crate`, `network-crate` | Deploy keys that mirror the crates to their own repositories | Make a new deploy key on that repository |
| `CLAUDE_CODE_OAUTH_TOKEN` | `issue-triage` | Labels new issues and pull requests | `claude setup-token`, then set it again |

## Sign-in providers (in Clerk)

Clerk holds these as an SSO connection's custom credentials ([accounts and SSH](../guides/accounts-and-ssh.md)).

| Credential | Clerk instances | Lost or leaked |
|---|---|---|
| Sign in with Apple key (P8) | dev, main | Revoke it in Apple Developer, make a new one, paste it into both instances' Apple connection |
| Client secret of the GitHub OAuth App "Codync" | main (dev can use Clerk's shared ones) | Make a new client secret on the app, paste it into Clerk's GitHub connection; the host's connector sign-in uses only the app's public client id |
| Google OAuth client | main | Make a new client secret in Google Cloud, paste it into Clerk's Google connection |

## Local files

| Where | What | Template |
|---|---|---|
| `.env.development` (main checkout's root, git-ignored; every worktree reads it) | `CODYNC_CLERK_SECRET_KEY` (signs worktree sandboxes into their test accounts, [Worktree sandboxes](../guides/dev-app.md#worktree-sandboxes)); the dev copies of `CODYNC_DEVICE_CERT_KEY` and `CODYNC_CLOUD_SECRET_KEYS` | `.env.development.example` |
| `.env.production` (same place) | The production copies of `CODYNC_DEVICE_CERT_KEY`, `CODYNC_CLOUD_SECRET_KEYS` and `CODYNC_HOST_UPDATE_SIGNING_KEY` | `.env.production.example` |
| `~/.codync-e2e/` | The provider keys cloud computer tests use: a Boat key limited to its `codync` environment and 7 days, and an InstaBox key ([cloud computer testing](../guides/cloud-computer-testing.md)) | none |

How the env files load: [local secrets](../guides/dev-app.md#local-secrets).
