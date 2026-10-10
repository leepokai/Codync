# Environments and deployment

Reviewed against repository configuration on 2026-09-26; migration log updated 2026-10-07. Checked-in configuration does not prove that a deployment is healthy or that an external dashboard is configured.

See [side-by-side Dev and production apps](dev-app.md) for schemes, identifiers, data isolation and the running-host/Offline troubleshooting checklist.

## Configuration ownership

| Layer | Source | Selection |
| --- | --- | --- |
| Apple app | `apps/shared/Config/dev.plist`, `main.plist` | `project.yml` copies the selected file to bundled `AccountConfig.plist`; `iOS Dev` uses dev; `iOS` uses main |
| Account SDK | `apps/shared/AccountSession.swift` | Public Clerk configuration; development environment overrides are supported |
| Host | `host/src/remote/cloud.rs` | `CODYNC_CLOUD=off` or stored disable wins; then `CODYNC_CLOUD_URL`, then the build's cloud (`CODYNC_ENV` at compile time) |
| Cloud Worker / D1 / DO | `cloud/wrangler.toml` | Root production, `dev`, or `local` environment |
| APNs Worker | `relay/wrangler.toml` | Separate deployment and secrets; see [relay README](../../relay/README.md) |

The environment is chosen when building, never stored: the host compiles in its cloud from `CODYNC_ENV` (`main` → `https://api.codync.dev`, unset or anything else → `https://dev-api.codync.dev`, debug or release alike), and the only cloud setting a host saves is on/off (**Reach from anywhere**, `codync-host cloud --enable|--disable`). `CODYNC_CLOUD_URL` points a host at a cloud you're developing (https, or http to this computer). `npm run dist:mac:dev` / `dist:mac:main` build the bundled host for the app's environment, so the script name picks the environment of a whole desktop build ([desktop app](../architecture/desktop-app.md#development)); the release workflows set `CODYNC_ENV=main`.

## Checked-in readiness

- **Development:** app configuration and Worker configuration target `https://dev-api.codync.dev`; dev Clerk issuer and D1 binding are configured in source. Exercise the live path before claiming readiness.
- **Production:** `main.plist` targets `https://api.codync.dev` with the Clerk production instance (`clerk.codync.dev`, Google sign-in through the `codync-auth` Google Cloud project, published). The root Worker is deployed with D1 `codync`, `CLERK_SECRET_KEY` and `CLERK_WEBHOOK_SECRET` (Clerk webhook endpoint `https://api.codync.dev/v1/webhooks/clerk`, `user.deleted`).
- **Local:** Wrangler's `local` environment supports the isolated integration test and its test issuer. It is not a real Google sign-in test.

## Troubleshooting: a running host appears offline on another computer

Check the environment before resetting keys, pairing again, or removing an account computer. A healthy local host and an active relay connection only prove connectivity to **that host's configured cloud**. Development and production have separate account directories, grants and presence; signing in with the same Apple account does not join those environments.

Observed on 2026-10-09: MacBook-Pro-3 ran a local development build connected to `dev-api.codync.dev`, while the Mac mini ran a production build connected to `api.codync.dev`. Both hosts reported `registered: true` and `connected: true`. The MacBook's same `computerId` was online in development D1 and offline in production D1. The Mac mini's account list therefore correctly showed the old production record as **Offline**. This was an environment mismatch, not evidence of a stopped host or a duplicate identity.

Before testing account discovery, access approval, or cross-device computer order:

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

## Secrets audit

The repository is public, so its whole history counts as published. Last scan: 2026-10-07 (`gitleaks git .` plus pattern grep over `git log -p --all`).

- **Rotated:** the `codync-push` Worker (`codync-push.kevin2005ha.workers.dev`) bearer secret, hard-coded in `Codync-macOS/Services/APNsPushService.swift` from 2026-03-19 (`96f7118`) until 2026-03-25 (`72837e2`). The value in history is dead; rotated by the owner before 2026-10-07.
- **Public by design:** the PostHog project key (`phc_…`, ingest only) and Clerk publishable keys (`pk_test_…`, `pk_live_…`).
- **Not secrets:** test vectors in `docs/remote-relay-vectors.json` and keys in test fixtures.
- No PostHog personal keys (`phx_…`), Clerk secret keys, cloud provider keys or private key files were ever committed.

## Migration log

| Migration | dev (`codync-dev`) | main (`codync`) | Restore point taken just before |
| --- | --- | --- | --- |
| `0001_init.sql` | applied | applied | — |
| `0002_windows_computers.sql` | 2026-10-07 | 2026-10-07 | dev `00000b79-00000000-000050fc-e6cb8fa431592b1c3d7824f3cf64afb9`, main `0000041d-0000085c-000050fc-fda9fcdf91e3498c08fa7c332a989e24` |
| `0003_desktop_devices.sql` | 2026-10-07 | 2026-10-07 | dev `00000cb2-00000002-000050fd-8600044ac4775783e20c8ff9b4d5fa14`, main `00000420-000012d2-000050fd-12871b6db16548f336c1c9221d28c556` |

`0002` lets `computers.platform` be `windows` (Windows hosts register with `std::env::consts::OS`). SQLite can't change a CHECK, so it rebuilds `computers` together with `access_requests` and `grants`, the tables that reference it: dropping a referenced table while foreign keys are enforced would count every referencing row as a violation. Live schemas matched `0001` beforehand; row counts were unchanged afterwards (dev 24 computers, 8 grants, 14 access requests; main 101, 38, 48) with no orphaned rows. A signed `POST /v1/host/register` with `platform: "windows"` returned 200 on both (the probe rows were deleted). Main's Worker answered with the previous version for about 20 seconds after `deploy`.

The Worker accepting `windows` was deployed to both environments from the `windows` branch on 2026-10-07, ahead of its merge; its cloud code differed from `main` only in that platform list. Until the branch reaches `main`, deploying cloud from `main` turns Windows registration off again (the database keeps accepting it).

`0003` lets `devices.platform` be `linux` or `windows`. Accounts were built when only the iPhone and the SwiftUI Mac app signed in; the Electron app brought sign-in to Linux (and later Windows), whose device registration the cloud rejected (`400 Invalid platform`), so main held no Linux devices at all. It rebuilds `devices` with `access_requests` and `grants` the same way as `0002`. Row counts were unchanged (dev 14 devices, 8 grants, 14 access requests; main 128, 41, 53) with no orphaned rows; a `windows` device row inserted on dev was accepted and removed. The Worker accepting both platforms was deployed to dev and main the same day from the `device-platforms` branch, which differed from `main` only there. Windows apps up to 2.9.0 register as `linux`; later ones send `windows`.

Rolling back a migration restores the whole database to that point, losing every write since: `wrangler d1 time-travel restore <db> --bookmark=<restore point>`.

A healthy HTTP endpoint proves Worker reachability only. It does not prove host registration, encrypted channel traffic, account approval, notification delivery or background reconnection.
