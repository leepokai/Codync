# Environments and deployment

Reviewed against repository configuration on 2026-09-26; migration log updated 2026-10-07. Checked-in configuration does not prove that a deployment is healthy or that an external dashboard is configured.

See [side-by-side Dev and production apps](dev-app.md) for schemes, identifiers, data isolation and the running-host/Offline troubleshooting checklist.

## Configuration ownership

| Layer | Source | Selection |
| --- | --- | --- |
| Apple app | `apps/shared/Config/dev.plist`, `main.plist` | `project.yml` copies the selected file to bundled `AccountConfig.plist`; `iOS Dev` uses dev; `iOS` uses main |
| Account SDK | `apps/ios/App/AccountSession.swift` | Public Clerk configuration; development environment overrides are supported |
| Host | `host/src/remote/cloud_url.rs` | `CODYNC_CLOUD=off` or stored disable wins; then `CODYNC_CLOUD_URL`, then a saved relay address (`cloud_url`), then the build's cloud (`CODYNC_ENV` at compile time) |
| Cloud Worker / D1 / DO | `cloud/wrangler.toml` | Root production, `dev`, or `local` environment |
| APNs Worker | `relay/wrangler.toml` | Separate deployment and secrets; see [relay README](../../relay/README.md) |

The environment is chosen when building, never stored: the host compiles in its cloud from `CODYNC_ENV` (`main` → `https://api.codync.dev`, unset or anything else → `https://dev-api.codync.dev`, debug or release alike), and the host saves only two cloud settings: on/off (**Reach from anywhere**, `codync-host cloud --enable|--disable`) and an optional [private relay](../../private-relay/README.md) (kv `cloud_url` and its registration token `cloud_relay_token`, set with **Relay address** and **Relay token** in Settings → Computers or `codync-host cloud --url … --token`). `CODYNC_CLOUD_URL` points a host at a Codync cloud you're developing or run yourself (https, or http to this computer); `CODYNC_CLOUD_TOKEN` beside it gives that URL a private relay's token and marks it as one (the saved token is never sent to an env URL). A private relay has no accounts: the host skips the state pull there, so account devices are kept but stay out until the Codync cloud renews them, and claims, approval, access requests, public webhooks and relayed screen are refused (`cloud_url.rs` `RelayKind`). `npm run dist:<platform>:dev` / `dist:<platform>:main` build the bundled host for the app's environment, so the script name picks the environment of a whole desktop build ([desktop app](../architecture/desktop-app.md#development)); the release workflows set `CODYNC_ENV=main`.

## Checked-in readiness

- **Development:** app configuration and Worker configuration target `https://dev-api.codync.dev`; dev Clerk issuer and D1 binding are configured in source. Exercise the live path before claiming readiness.
- **Production:** `main.plist` targets `https://api.codync.dev` with the Clerk production instance (`clerk.codync.dev`, Google sign-in through the `codync-auth` Google Cloud project, published; GitHub sign-in not enabled yet: it needs its GitHub OAuth App before the release that ships the button, see [GitHub sign-in](accounts-and-ssh.md#github-sign-in)). The root Worker is deployed with D1 `codync`, `CLERK_SECRET_KEY` and `CLERK_WEBHOOK_SECRET` (Clerk webhook endpoint `https://api.codync.dev/v1/webhooks/clerk`, `user.deleted`), and `RELEASE_GATE_TOKEN` (dispatches the release gate; renewal in the [cloud README](../../cloud/README.md)).
- **Cloud assistants (MCP connector):** the Worker also serves `mcp.codync.dev` (main) and `dev-mcp.codync.dev` (dev), deployed 2026-10-26. Each Clerk instance lists its URL under Agents → MCP server (public checks pass) with CIMD on and DCR off, and the zone's "no Browser Integrity Check" rule covers both hosts ([cloud assistants](../features/cloud-assistants.md)).
- **Local:** Wrangler's `local` environment supports the isolated integration test and its test issuer. It is not a real Google sign-in test.
- **Dev release channel:** `dev-releases.codync.dev` serves the R2 bucket `codync-releases-dev` (public read, custom domain), which CI fills from `dev` ([dev release channel](development.md#dev-release-channel)); the zone's "no Browser Integrity Check" rule covers it, since hosts and `curl` download from it.
- **Host names:** a dev host is the production name with a `dev-` prefix (`api` → `dev-api`, `mcp` → `dev-mcp`), never a `-dev` suffix: the suffix position is for regions (`relay-na`, so its dev host is `dev-relay-na`). Keep every host one level under `codync.dev` (a hyphen, not `dev.api.codync.dev`) so Cloudflare's universal certificate for `*.codync.dev` covers it.

## Troubleshooting: a running host appears offline on another computer

Check the environment before resetting keys, pairing again, or removing an account computer. A healthy local host and an active relay connection only prove connectivity to **that host's configured cloud**. Development and production have separate account directories, grants and presence; signing in with the same Apple account does not join those environments.

Observed on 2026-10-09: MacBook-Pro-3 ran a local development build connected to `dev-api.codync.dev`, while the Mac mini ran a production build connected to `api.codync.dev`. Both hosts reported `registered: true` and `connected: true`. The MacBook's same `computerId` was online in development D1 and offline in production D1. The Mac mini's account list therefore correctly showed the old production record as **Offline**. This was an environment mismatch, not evidence of a stopped host or a duplicate identity.

Before testing account discovery or access approval:

1. Run `codync-host cloud` on each computer and compare `url`, `registered`, `connected`, and `last error`. Use the installed app's bundled executable if the shell's host binary is not the one being tested.
2. Check `http://127.0.0.1:19222/health` (or the configured development port) for the running binary path, version and `computerId`.
3. Verify the desktop app and its host target the same environment, and the iPhone build targets that environment too. An identical version number or Apple login is not sufficient.
4. Use matching builds across the devices participating in the test, then refresh the account list. Local or SSH access working does not establish that their account clouds match.

### Separate Codync Dev app

Development and production now use separate app identities, data directories,
ports, service labels, URL schemes and notification destinations. They can run
side by side without sharing credentials, pairing grants or host keys. See
[Dev app isolation](dev-app.md) for build commands, resource names and signing
requirements. Use matching environments on every device in a cross-device test.

## Deployment procedure

Use [cloud/README.md](../../cloud/README.md) for exact Wrangler commands and binding names. Select the environment before changing anything:

1. Confirm the intended Cloudflare account, Worker, route, D1 database and Clerk issuer.
2. Create the database only if it does not exist; use its actual ID in the correct environment.
3. Apply migrations from `cloud/migrations/` to that environment.
4. Run cloud unit tests, type checking and the local integration test.
5. Deploy using `npm run deploy:dev` or `npm run deploy:main` from `cloud/` when deployment is intended.
6. Check the deployed health endpoint, then complete the device scenarios in [Cloudflare testing](cloudflare-testing.md).

Production also needs completed Apple native application registrations in Clerk and populated main app configuration. Publishable keys are public configuration; Clerk secret keys and APNs credentials do not belong in app bundles or documentation. Configure APNs using the separate relay guide.

A migration that changes what the database accepts goes out before the Worker code that relies on it, in each environment: dev first, then main. `wrangler d1 migrations list <db> --remote` shows what an environment still lacks. Before applying one to main, note the restore point (`wrangler d1 time-travel info codync`) and compare the live schema with `cloud/migrations/`.

## Releasing cloud changes

`RELAY_NEEDS_ACCOUNT` is on for dev and off for main. It keeps the relay and public webhooks to computers in an account ([optional sign-in](../plans/optional-sign-in.md#compatibility-and-rollout)). Turn it on for main with a `[vars]` entry in `wrangler.toml` once both stores have the apps from 2.41.0.

CI only tests `cloud/`; nothing deploys it. A change to `cloud/` (routes, bindings, migrations, secrets) goes out in this order:

1. On its branch, it runs only on that worktree's local Worker ([which cloud](cloudflare-testing.md#which-cloud-a-change-is-tested-on)).
2. Right after it is merged into `dev` and pushed: create what it needs in dev (R2 bucket, rate limit, secret, then the D1 migrations), `npm run deploy:dev` from a clean tree at `origin/dev` (the merged worktree right after its push: `git rev-parse HEAD` is `origin/dev` and `git status --porcelain -- cloud` prints nothing; never the shared main checkout, where other agents' uncommitted work would ship too), and check its routes on `dev-api.codync.dev`. `dev-api` then always runs what `dev` has, so every worktree that uses the dev cloud meets the code it was branched from. If that can't happen yet (a secret only the owner can make), say so when merging and deploy as soon as it can.
3. Before a dev → main PR that changes `cloud/` merges: test the changed flows end to end against `dev-api.codync.dev` with the dev apps.
4. Right after the merge into `main`, before or with the release tag: create the same bindings for main (`wrangler.toml` names them, e.g. `codync-icons` beside `codync-icons-dev`; keep R2 buckets private), apply migrations, `npm run deploy:main`.
5. Check main: each new route on `api.codync.dev` answers (401 without credentials, not 404), and `npx wrangler deployments list` shows the new version.

Clients and host that call a new route must not reach users before `api.codync.dev` has it (step 4). In 2026-10, custom icons shipped in v2.33.2 while production cloud was days old and its R2 bucket missing, so every upload failed.

## Secrets audit

The repository is public, so its whole history counts as published. Last scan: 2026-10-07 (`gitleaks git .` plus pattern grep over `git log -p --all`).

- **Rotated:** a push Worker bearer secret committed from 2026-03-19 (`96f7118`) until 2026-03-25 (`72837e2`). The value in history is dead; rotated by the owner before 2026-10-07.
- **Public by design:** the PostHog project key (`phc_…`, ingest only) and Clerk publishable keys (`pk_test_…`, `pk_live_…`).
- **Not secrets:** test vectors in `docs/reference/fixtures/remote-relay-vectors.json` and keys in test fixtures.
- No PostHog personal keys (`phx_…`), Clerk secret keys, cloud provider keys or private key files were ever committed.

## Migration log

| Migration | dev (`codync-dev`) | main (`codync`) | Restore point taken just before |
| --- | --- | --- | --- |
| `0001_init.sql` | applied | applied | — |
| `0002_windows_computers.sql` | 2026-10-07 | 2026-10-07 | dev `00000b79-00000000-000050fc-e6cb8fa431592b1c3d7824f3cf64afb9`, main `0000041d-0000085c-000050fc-fda9fcdf91e3498c08fa7c332a989e24` |
| `0003_desktop_devices.sql` | 2026-10-07 | 2026-10-07 | dev `00000cb2-00000002-000050fd-8600044ac4775783e20c8ff9b4d5fa14`, main `00000420-000012d2-000050fd-12871b6db16548f336c1c9221d28c556` |
| `0004_android_devices.sql` | 2026-11-01 | 2026-11-01 | dev `00003428-00000000-00005115-cc5e0e1311eab7918b2c9470a0e71611`, main `00000509-00063149-00005115-8caa0ba65c5566a0616c65bafa82e27f` |
| `0005_pairing_codes.sql` | 2026-11-01 | 2026-11-02 | dev not recorded, main `00000519-0005d251-00005117-8acfe05ed62b5a5d6a6a8e47e17c98b9` (also before `0006`) |
| `0006_signed_out_devices.sql` | 2026-11-02 | 2026-11-02 | dev not recorded, main as `0005` |
| `0007_cloud_machines.sql` | 2026-11-09 | 2026-11-09 | dev not recorded, main `0000055e-0001fe5a-0000511e-b86080df83e6b9cbcbf226d32764ed40` (also before `0008` and `0009`) |
| `0008_app_versions.sql` | 2026-11-09 | 2026-11-09 | as `0007` |
| `0009_cloud_credentials.sql` | 2026-11-09 | 2026-11-09 | as `0007` |
| `0010_cloud_analytics.sql` | 2026-11-10 | 2026-11-11 | dev not recorded, main `0000056e-0000efb2-00005120-aea6bef89bac4652c5b06937eba57a47` |

`0010` adds `cloud_machines.analytics`, whether a cloud computer's owner shares usage analytics (additive). On main, right after the 2.44.1 merge, the counts were unchanged (6 cloud machines, 3938 devices, 4728 computers) with a clean `foreign_key_check`; after `deploy:main`, `POST /v1/host/analytics` and `/v1/host/sleep` answered 401 without a host signature.

`0007`–`0009` add Codync Cloud: the `cloud_machines`, `cloud_credentials` and `host_releases` tables, and `devices.app_version`, `min_host` and `app_seen_at`; all additive (`0009` rebuilds only the new `cloud_machines`). On main, right after the 2.43.1 merge, the counts were unchanged (3803 devices, 4570 computers, 828 grants, 1130 access requests) with a clean `foreign_key_check`. The main Worker got its own `CLOUD_SECRET_KEYS` (copy in `.env.production`) before `deploy:main`; every new route then answered 401 without credentials.

`0005` adds the `pairing_codes` table (short codes like `QRTX-4821`) and `0006` adds `devices.signed_out_at`; both are additive. On main, right after the 2.36.0 merge, the counts were unchanged (2381 devices, 3233 computers, 534 grants, 739 access requests) with a clean `foreign_key_check`. The main Worker got its `DEVICE_CERT_KEY` secret (from `.env.production`, the public half matching `host/src/environment.rs`) before `deploy:main`; every new route then answered 401 without credentials.

`0002` lets `computers.platform` be `windows` (Windows hosts register with `std::env::consts::OS`). SQLite can't change a CHECK, so it rebuilds `computers` together with `access_requests` and `grants`, the tables that reference it: dropping a referenced table while foreign keys are enforced would count every referencing row as a violation. Live schemas matched `0001` beforehand; row counts were unchanged afterwards (dev 24 computers, 8 grants, 14 access requests; main 101, 38, 48) with no orphaned rows. A signed `POST /v1/host/register` with `platform: "windows"` returned 200 on both (the probe rows were deleted). Main's Worker answered with the previous version for about 20 seconds after `deploy`.

`0004` lets `devices.platform` be `android`, rebuilding `devices` with `grants` and `access_requests` the same way. Until it ran, every signed-in Android phone's `POST /v1/devices` failed with a CHECK constraint (`internal`), so it saw no account computers. On dev the live schema matched `0003` beforehand. Afterwards the row counts were unchanged (70 devices, 12 grants, 22 access requests, 232 computers), with no orphaned rows and a clean `foreign_key_check`. An Android emulator then registered, was approved and revoked through the account. On main (right after the 2.34.0 merge) the live schema also matched `0003`, and the counts were unchanged (1968 devices, 435 grants, 627 access requests, 2687 computers) with a clean `foreign_key_check`.

The Worker accepting `windows` was deployed to both environments from the `windows` branch on 2026-10-07, ahead of its merge; its cloud code differed from `main` only in that platform list.

`0003` lets `devices.platform` be `linux` or `windows`. Accounts were built when only the iPhone and the SwiftUI Mac app signed in; the Electron app brought sign-in to Linux (and later Windows), whose device registration the cloud rejected (`400 Invalid platform`), so main held no Linux devices at all. It rebuilds `devices` with `access_requests` and `grants` the same way as `0002`. Row counts were unchanged (dev 14 devices, 8 grants, 14 access requests; main 128, 41, 53) with no orphaned rows; a `windows` device row inserted on dev was accepted and removed. The Worker accepting both platforms was deployed to dev and main the same day from the `device-platforms` branch, which differed from `main` only there. Windows apps up to 2.9.0 register as `linux`; later ones send `windows`.

A `0004_computer_order.sql` (an account-wide computer order) reached only dev before ordering became device-local. On 2026-10-11 its column was dropped from `codync-dev` and its row deleted from `d1_migrations` (restore point `00001447-00000002-00005101-ce9dbb4b6e3847fef79f63887a5d89af`), and the file was removed, so both environments match `0001`–`0003`.

Rolling back a migration restores the whole database to that point, losing every write since: `wrangler d1 time-travel restore <db> --bookmark=<restore point>`.

A healthy HTTP endpoint proves Worker reachability only. It does not prove host registration, encrypted channel traffic, account approval, notification delivery or background reconnection.
