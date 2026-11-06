# Testing cloud computers (Codync Cloud)

Codync Cloud runs a computer for a signed-in user on a provider (Boat first; [plan](../plans/codync-cloud.md)).
Testing it touches real machines, a cloud Worker and accounts, so it runs on two levels, like the rest of the
project ([Dev app isolation](dev-app.md)):

| Level | What it is for | Cloud | Accounts | Apps | Machines |
|---|---|---|---|---|---|
| **Worktree** (ephemeral) | a feature branch, before it merges; deleted with the worktree | a local Worker per run (own D1, own relay), never `dev-api` | test tokens, or the dev Clerk instance with a test account | iOS simulator, Android emulator and desktop sandbox of that worktree | Boat, named `codync-e2e-…`, deleted at the end of every run |
| **Dev** (persistent) | the remote side as it is on `dev`, always on | `dev-api.codync.dev` | real dev accounts (Codync Dev) | Codync Dev on a physical iPhone, Android and the shared desktop Codync Dev | Boat, named `codync-dev-…`, kept |

Production uses neither: its key, machines and accounts are separate.

## Worktree: isolated end-to-end runs

Everything a run creates is its own and goes away with it:

- **Cloud**: `cloud/test/e2e/harness.ts` starts `wrangler dev --env local` on a free port with a fresh D1 in a
  temporary folder.
- **Machines**: the local cloud gets the test Boat key (`boat api-key create codync-cloud-e2e --ttl 7d --actions
  sandbox.create,sandbox.read,sandbox.stop,sandbox.resume,sandbox.delete,snapshot.read,snapshot.write`; without
  the snapshot actions *Delete snapshots* and backups fail with `api_key_action_forbidden`) and
  `CLOUD_MACHINE_PREFIX=codync-e2e`. Each run deletes its machine when it ends, also after a failure, and first
  sweeps test machines older than two hours (a run that crashed before its cleanup). Runs take a lock
  (`~/.codync-e2e/boat.lock`), since several sessions share this computer: of two runs at once (2026-10-31), one
  timed out in its machine's first boot and the other in a wake, and each passed alone.
- **Reaching this computer**: a machine reaches the local cloud and a small file server through
  `boat forward --reverse`, at its own `127.0.0.1` with the same port numbers, so the host signs the same
  authority the Worker checks. A sleep of several minutes ends the tunnel, so it is opened again each time the
  machine comes up.
- **The host under test**: `CLOUD_HOST_BINARY_URL` makes the machine's first boot install that build instead of
  the release (the real first-boot path: claim ticket, service, claim), and `CLOUD_HOST_ENV` gives its service
  `CODYNC_CLOUD_URL`, `CODYNC_ANALYTICS_ENV=dev` (its events say `env: dev`, outside every chart; Dev machines
  set it too) and, for the sleep steps,
  a short `CODYNC_SLEEP_IDLE_SECS`.
- **Linux build**: Boat machines are x86_64 and the Lima VM is arm64, so build on a Boat build machine (a
  stopped `codync-build` sandbox resumed for builds, a few cents an hour): copy `host/`, `packaging/`, the
  language catalogs and `docs/reference/fixtures/` in, `CODYNC_ENV=main cargo build --bin codync-host`, `strip`
  the binary and copy it back. Always `CODYNC_ENV=main`: a cloud computer is a production-environment host (the
  claim ticket is in `~/.codync`, the service is `codync-host`); a development build looks in `~/.codync-dev`
  under another service and never claims itself. The updater refuses debug builds, so the update steps need
  `CODYNC_ENV=main cargo build --release` with the version in `host/Cargo.toml` lowered below a published release
  (only on the build machine).

### Scripted run

```bash
cd cloud
BOAT_API_KEY_FILE=<scoped key file> CODYNC_HOST_LINUX_BIN=<linux host> npm run e2e:cloud
# also the update steps: a production release build below a published version
CODYNC_E2E_UPDATE_TO=2.24.0 CODYNC_HOST_LINUX_BIN=<release build at 2.23.9> … npm run e2e:cloud
# the owner's own update to the latest release, from a phone (same kind of build)
CODYNC_E2E_UPDATE_NOW=1 CODYNC_HOST_LINUX_BIN=<release build below the latest> … npm run e2e:cloud
# the long run (about an hour): backups, a cold wake, a restore from the backup, deleting the snapshot history;
# a host build at the latest version, so the cron's release list offers no update before sleep
CODYNC_E2E_COLD=1 CODYNC_HOST_LINUX_BIN=<release build at the latest version> … npm run e2e:cloud
```

A fake phone (`test/e2e/phone.ts`) walks: create → first boot → self-claim → access without a code → chat → idle
sleep → a mailbox message wakes it → an app that needs a newer host updates it first → the owner updates it to the
latest release from the phone → delete. Results so far:
[Boat trial log](../plans/codync-cloud-boat-trial.md).

### Real apps against a local cloud

`npm run cloud:stack` starts the same local cloud, accepting **dev Clerk** sessions (it reads the dev issuer from
`wrangler.toml` and its public key from the instance's JWKS), allows every account (`CLOUD_COMPUTER_USERS=*`,
local only), forwards each new machine, and prints how to point each app at it:

- iOS simulator: `SIMCTL_CHILD_CODYNC_CLOUD_URL=<url> xcrun simctl launch <udid> com.pokai.Codync.ios.dev`, on a
  simulator no other session uses.
- Desktop: `CODYNC_CLOUD_URL=<url> npm run dev` in the worktree (its sandbox host, signed in to its Clerk test
  account; a development build takes the local `http://127.0.0.1` cloud, a release build only HTTPS).
- Android: `adb reverse tcp:<port> tcp:<port>` on its own emulator, built with `-PcodyncCloudUrl=<url>`.
- Sign in with a Clerk test address (`anything+clerk_test@example.com`, code `424242`; the instance asks for a
  password at sign-up, so the test hook sets a random one): no mail, no personal account. The iPhone app's debug
  build signs it in at launch with `SIMCTL_CHILD_CODYNC_TEST_SIGN_IN=<address>`, creating it the first time; it
  stays signed in on that simulator.

Set `CODYNC_STACK_PORT` to keep the URL stable across runs. Ctrl-C deletes the machines.

What to walk with the apps: Codync Cloud hidden for an account that isn't allowed; create → "Preparing your cloud
computer…" → claimed → each app (desktop included) gets access by itself → chat with a bot on it, and create and
set up bots there from each app; on the desktop, hide the window → the channel closes and the computer can sleep; idle → "Sleeping · wakes when you
message" → send → it wakes and answers; an app with a newer `minHost` → "Your cloud computer is updating…" →
back; `APP_FLOOR_*` → *Update this app*; delete snapshots (awake: done; asleep: refused); delete the computer →
gone from every app. Backups and a cold wake (stopped over 30 minutes) are in the scripted long run
(`CODYNC_E2E_COLD=1`; backups need a paid Boat plan). A restore from a backup happens only once Boat drops a
stopped machine's own snapshot (14 days), so the long run has a pass-through to Boat answer one resume with
`409 snapshot_data_expired`; everything else, the new machine included, is the real Boat.

## Dev: the persistent remote environment

Dev is where the remote side lives: `dev-api.codync.dev`, the relay, dev accounts, and Codync Dev on real
devices. Its cloud computers stay, so sleep, wake and updates are seen over days, not minutes.

Setup (once, then kept up with `dev`):

1. Apply `cloud/migrations/0007_cloud_machines.sql` and `0008_app_versions.sql` to `codync-dev` (note the restore
   point; [deployment procedure](environments-and-deployment.md#deployment-procedure)) before deploying Worker code
   that reads them: `/v1/computers` joins `cloud_machines`.
2. Secrets on `--env dev`: `BOAT_API_KEY` (its own scoped key with the test key's actions, not the test key), `CLOUD_COMPUTER_USERS` (the dev
   Clerk ids allowed; a secret, the repository is public).
3. Vars in `[env.dev]`: `CLOUD_MACHINE_PREFIX = "codync-dev"`, `CLOUD_HOST_ENV = "CODYNC_CLOUD_URL=https://dev-api.codync.dev
   CODYNC_ANALYTICS_ENV=dev"` (released hosts are built for the production cloud and would report as `main`).
4. Deploy with `npm run deploy:dev` from `dev`.
5. Its machines boot the released host, so Dev runs cloud computers fully once a release carries the host side
   (self-claim, sleep, cloud updates). Until then, branch hosts are tested at the worktree level.
6. Create one cloud computer for the owner's dev account and keep it; check it from the iPhone over days.

Boat machines named `codync-dev-…` belong to Dev and are never swept by test runs.
