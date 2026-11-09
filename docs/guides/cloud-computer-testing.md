# Testing cloud computers (Codync Cloud)

Codync Cloud runs a computer for a signed-in user on their own provider account, with the key they give it (Boat or
InstaBox; [plan](../plans/codync-cloud.md)).
Testing it touches real machines, a cloud Worker and accounts, so it runs on two levels, like the rest of the
project ([Dev app isolation](dev-app.md)):

| Level | What it is for | Cloud | Accounts | Apps | Machines |
|---|---|---|---|---|---|
| **Worktree** (ephemeral) | a feature branch, before it merges; deleted with the worktree | a local Worker per run (own D1, own relay), never `dev-api` | test tokens, or the dev Clerk instance with a test account | iOS simulator, Android emulator and desktop sandbox of that worktree | Boat or InstaBox, named `codync-e2e-…`, deleted at the end of every run |
| **Dev** (persistent) | the remote side as it is on `dev`, always on | `dev-api.codync.dev` | real dev accounts (Codync Dev) | Codync Dev on a physical iPhone, Android and the shared desktop Codync Dev | Boat, named `codync-dev-…`, kept |

Production uses neither: its sealing keys, machines and accounts are separate.

## Worktree: isolated end-to-end runs

Everything a run creates is its own and goes away with it:

- **Cloud**: `cloud/test/e2e/harness.ts` starts `wrangler dev --env local` on a free port with a fresh D1 in a
  temporary folder.
- **Machines**: the test user gives the local cloud a Boat key made the way users make theirs, limited to the
  `codync` environment (`boat env new codync`, `boat env set codync --safe-for-third-parties true`, then
  `boat api-key create codync-cloud-e2e --env <its id from boat env info codync> --ttl 7d --actions
  sandbox.create,sandbox.read,sandbox.stop,sandbox.resume,sandbox.delete,snapshot.read,snapshot.write`; without
  the snapshot actions *Delete snapshots* and backups fail with `api_key_action_forbidden`), through
  `PUT /v1/cloud-credentials/boat`, and the cloud runs with `CLOUD_MACHINE_PREFIX=codync-e2e`. Each run deletes its machine when it ends, also after a failure, and first
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

A fake phone (`test/e2e/phone.ts`) walks: no computer without a key → the user's key is checked and kept sealed →
create → first boot → self-claim → access without a code → chat → idle
sleep → a mailbox message wakes it → an app that needs a newer host updates it first → the owner updates it to the
latest release from the phone → delete. Results so far:
[Boat trial log](../plans/codync-cloud-boat-trial.md).

### InstaBox

The InstaBox key lives in `~/.codync-e2e/instabox-key` (machine scopes only, the owner's free personal workspace:
never billed, 4 vCPU/8 GiB, pauses after 30 idle minutes). Its machines can't be reached by a reverse forward
(InstaBox's SSH forwards one way only), so `INSTABOX_API_KEY_FILE=~/.codync-e2e/instabox-key npm run cloud:stack`
also opens a Cloudflare quick tunnel to the local cloud (`cloudflared`, run with an empty config so a
`~/.cloudflared/config.yml` of another tunnel can't answer for it) and gives the hosts its `https://….trycloudflare.com`
URL: the Worker sees that host, which is the one the host signs. They boot `CODYNC_HOST_LINUX_BIN` through a second
tunnel to the file server (x86_64, built on an InstaBox machine: its template has Rust), else the released host. This Mac's resolver may cache the new tunnel name as missing for a minute; the machines resolve
it at once. The scripted run (`npm run e2e:cloud`) is Boat's only.

### The keeper and the agents' user

A machine set up by this branch runs the host and its agents as `codync-agent` and the keeper as the system user
`codync`; the provider's own `user` keeps its sudo ([Agents and the host's secrets](../plans/codync-cloud.md#agents-and-the-hosts-secrets)).
Check on the machine (InstaBox `exec` and `boat exec` run as `user`, so test the agents' side with
`sudo -u codync-agent …`): `systemctl is-active codync-keeper`; as `codync-agent`, `sudo -n true` asks for a password,
`id` shows no group but its own, `ls /var/lib/codync-keeper` and (Boat) `cat /opt/ascii-agent/.env` are refused, and
`/proc/<host pid>/environ` is refused (the host is non-dumpable); the keeper's socket `/run/codync-keeper/keeper.sock`
answers `{"op":"status"}` with `"unlocked": true` once the claim named it (the Worker's `GET /v1/keeper/vault-key`); a
key saved from an app (voice, an agent's API key, a connector's header) is in the keeper, not in `~/.codync`, and an
agent gets only a `http://127.0.0.1:19230/t/…` URL for it. Machines set up before the keeper have none: delete and
recreate them.

### Real apps against a local cloud

`npm run cloud:stack` starts the same local cloud, accepting **dev Clerk** sessions (it reads the dev issuer from
`wrangler.toml` and its public key from the instance's JWKS), and prints how to point each app at it. With
`BOAT_API_KEY_FILE` or `INSTABOX_API_KEY_FILE` it also seals keys (`CLOUD_SECRET_KEYS`), so every account gets Codync
Cloud, and forwards each new machine; without one it is the plain local cloud a worktree sandbox runs, with Codync
Cloud hidden:

- iOS simulator: `SIMCTL_CHILD_CODYNC_CLOUD_URL=<url> xcrun simctl launch <udid> com.pokai.Codync.ios.dev`, on a
  simulator no other session uses.
- Desktop: `CODYNC_CLOUD_URL=<url> CODYNC_DEVICE_CERT_KEY=<certPub> npm run dev -- --cloud` in the worktree (its
  sandbox host, signed in to its Clerk test account; a development build takes the local `http://127.0.0.1` cloud, a
  release build only HTTPS).
- Android: `adb reverse tcp:<port> tcp:<port>` on its own emulator, built with `-PcodyncCloudUrl=<url>` together with
  `-PcodyncClerkPublishableKey=<dev pk_test>` and `-PcodyncPushRelayUrl=<relay>` (the build refuses one override alone).
- Sign in with a Clerk test address (`anything+clerk_test@example.com`, code `424242`; the instance asks for a
  password at sign-up, so the test hook sets a random one): no mail, no personal account. The iPhone app's debug
  build signs it in at launch with `SIMCTL_CHILD_CODYNC_TEST_SIGN_IN=<address>`, creating it the first time; it
  stays signed in on that simulator.

Set `CODYNC_STACK_PORT` to keep the URL stable across runs. Ctrl-C deletes the machines.

What to walk with the apps: Codync Cloud hidden on a cloud without `CLOUD_SECRET_KEYS` (`cloud:stack` with no provider
key, or a worktree sandbox's own cloud); a key that isn't one, an admin key
and a revoked key refused with what to fix; the test key (`BOAT_API_KEY_FILE`, or `INSTABOX_API_KEY_FILE` under
InstaBox) pasted and shown as its hint; "+" → New cloud computer lists every provider with its logo (one without a
working key opens that provider's key sheet, and is picked once its key is saved; a computer's `⋯` opens its
provider's key too) → a name → create → "Preparing your cloud
computer…" → claimed, under that name → each app (desktop included) gets access by itself → chat with a bot on it, and create and
set up bots there from each app; on the desktop, hide the window → the channel closes and the computer can sleep; idle → "Sleeping · wakes when you
message" → send → it wakes and answers; an app with a newer `minHost` → "Your cloud computer is updating…" →
back; `APP_FLOOR_*` → *Update this app*; delete snapshots (awake: done; asleep: refused; preparing: off); *Rename…* → the card, the account's
computers and the computer's own name follow, in the other apps already connected to it too; a second one on the
other provider beside it, and neither key removable while its computer exists; at the limit (3) "+" opens with Create
off and why, its keys still in reach; a key turned down for the plan still lets Create try again, one revoked or
expired doesn't; delete the computer (*Delete {name}?*) → gone from every app and
at its provider. Backups and a cold wake (stopped over 30 minutes) are in the scripted long run
(`CODYNC_E2E_COLD=1`; backups need a paid Boat plan). A restore from a backup happens only once Boat drops a
stopped machine's own snapshot (14 days), so the long run has a pass-through to Boat answer one resume with
`409 snapshot_data_expired`; everything else, the new machine included, is the real Boat.

## Dev: the persistent remote environment

Dev is where the remote side lives: `dev-api.codync.dev`, the relay, dev accounts, and Codync Dev on real
devices. Its cloud computers stay, so sleep, wake and updates are seen over days, not minutes.

Setup (once, then kept up with `dev`):

1. Apply `cloud/migrations/0007_cloud_machines.sql`, `0008_app_versions.sql` and `0009_cloud_credentials.sql` to `codync-dev` (note the restore
   point; [deployment procedure](environments-and-deployment.md#deployment-procedure)) before deploying Worker code
   that reads them: `/v1/computers` joins `cloud_machines`.
2. Secret on `--env dev`: `CLOUD_SECRET_KEYS` (its own, never production's); with it, every signed-in dev account
   gets Codync Cloud. Each dev account pastes its own provider key in the app.
3. Vars in `[env.dev]`: `CLOUD_MACHINE_PREFIX = "codync-dev"`, `CLOUD_HOST_ENV = "CODYNC_CLOUD_URL=https://dev-api.codync.dev
   CODYNC_ANALYTICS_ENV=dev"` (hosts are built for the production cloud and would report as `main`), and
   `HOST_RELEASES_URL = "https://dev-releases.codync.dev"`.
4. Deploy with `npm run deploy:dev` from `dev`.
5. Its machines install and update from the [dev release channel](development.md#dev-release-channel), which CI
   fills from every `dev` push that changes the host: Dev runs what `dev` has, before it is released. A computer
   already running gets a host change once `dev`'s version is bumped past its own (before it sleeps, or *Update*).
6. Create one cloud computer for the owner's dev account and keep it; check it from the iPhone over days.

Moving the dev cloud to the dev release channel (once): wait for the first `dev` host run to fill it
(`https://dev-releases.codync.dev/latest/download/install.sh` and `…/codync-host-compat.json` answer 200), and only then
`npm run deploy:dev`: before that, a new computer's setup can't download the installer. A computer created before
the channel still follows GitHub, while the dev cloud's soft update offers it versions only the channel has: its
updates fail and it never sleeps. Delete and recreate it, or add
`Environment=CODYNC_RELEASES_URL=https://dev-releases.codync.dev` to its
`~codync-agent/.config/systemd/user/codync-host.service.d/codync-cloud.conf` and to
`/etc/systemd/system/codync-keeper.service`, then restart both.

Boat machines named `codync-dev-…` belong to Dev and are never swept by test runs.
