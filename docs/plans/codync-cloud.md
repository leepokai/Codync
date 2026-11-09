# Codync Cloud

Status (2026-10-31): steps 1–3 of the [build order](#build-order-on-boat) and both update paths are built on
`feat/cloud-boat`, synced with `dev`, and passed end to end on Boat and the iPhone app.
What's left before each release: [Delivery](#delivery). Owner decisions:

- **Free, and users bring their own cloud** (owner, 2026-11-06; how, 2026-11-08): Codync charges nothing; each user
  connects their own Boat account with an API key they paste once, which Codync keeps sealed:
  [Bring your own cloud](#bring-your-own-cloud). Paragraphs below that put the machine on Codync's own provider
  account (one Worker key, Codync paying, TOS-8) predate this; the BYOC section wins where they differ.
- **Backend: Boat (boat.dev) first** (owner, 2026-10-24): cheapest sleeping machine with the clearest terms
  (see [Boat first](#boat-first)). Asian VPSs for always-on tiers and Fly Machines stay the next adapters;
  InstaBox and Cloudflare are on hold.
- **Signed-in users only** (owner, 2026-10-24): Codync Cloud is offered only to people signed in to a Codync account;
  the cloud computer is claimed to that account.
- **A VPS you run yourself already works today**: [Linux servers](../guides/linux-servers.md).

## Bring your own cloud

Built 2026-11-08 on `feat/cloud-boat`. The user's cloud computer is a machine on **their own account with a cloud
provider** (Boat or InstaBox): the provider bills them, Codync bills nothing and has no provider account for users.
Codync still runs it: creates it, claims it, puts it to sleep and wakes it.

### Where each secret lives

Rule: a secret lives in exactly one place, the one that has to use it while nobody is looking.

| Secret | Lives in | Why there | Never seen by |
|---|---|---|---|
| The provider's API key (Boat, InstaBox) | the Worker, sealed in D1 (`cloud_credentials`) | wakes for routines, webhooks, mailbox messages and `wakeAt` happen with every device closed and the machine stopped | the machine, the relay, the apps once it's saved, logs |
| Agent sign-ins and API keys (Claude Code, Codex…) | the cloud computer: the agent's own folders, or the host vault (`agent/auth.rs`) | the agent uses them; they go there over the end-to-end channel | Codync's servers |
| Voice keys (OpenAI, Gemini) | the host vault on the cloud computer (already built) | the host mints per-call credentials | Codync's servers |
| Device keys | each device's Keychain / Keystore (already built) | they sign the device's requests | everyone else |

Places rejected for the provider key: the phone (nothing could wake the computer for a routine or a webhook while the phone
is closed); the machine itself (it's off exactly when it must be woken, and anyone with a shell there is root, agents
included); Boat's own scoped-key minting from an admin key (Codync would receive an admin key, which Boat says never to
give a third party, and `POST /api-keys/scoped` may not be enabled).

### The key the user makes

**Boat**: an environment `codync` and a key limited to it, with only the actions Codync uses and no expiry:

```bash
boat env new codync
boat env set codync --safe-for-third-parties true
boat env info codync   # its "id" goes after --env
boat api-key create codync --env <id> --ttl never \
  --actions sandbox.create,sandbox.read,sandbox.stop,sandbox.resume,sandbox.delete,snapshot.read,snapshot.write
```

`--env` takes the environment's id, not its name (Boat answers `api_key_sandbox_forbidden` to the name; checked
2026-11-08). The apps show these lines as the Worker words them (each provider's `setup` in `GET /v1/cloud-computers`),
with a link to Boat's dashboard, which does the same with a form.

- It can't run commands, read or write files, SSH or stream the desktop (no `exec`, `file.*`, `ssh`, `desktop`),
  reach the user's sandboxes outside `codync`, read billing, or make keys and webhooks.
- Codync creates every cloud computer (and a restore from its backup) in environment `codync` (`GROUP` in
  `cloud-credentials.ts`), still with `noEnv` and monitoring off.
- `--ttl never`: an expiring key would leave the computer unable to wake with no warning a year later; revoking it at
  Boat is the off switch and takes effect at once. Boat recommends expiring keys; this is the trade-off.

**InstaBox**: a key with only the machine scopes, no expiry (`instabox keys create --name codync --scope
machines:read,machines:write,machines:delete`, or *API keys → Create Key → Restricted* in its dashboard). InstaBox
can't limit a key to some machines: it reaches every machine of its workspace, and `machines:write` includes `exec`,
which is how Codync installs the host (below). The owner chose to offer it anyway (2026-11-08); the setup lines say so,
and a user who wants it narrower makes the key in a workspace of its own. A personal workspace's keys can never be a
workspace admin's.

### Saving it (`PUT /v1/cloud-credentials/:provider {key}`)

1. Only on a cloud that can seal keys (`CLOUD_SECRET_KEYS`, every signed-in account there); at most 5 saves a minute
   per account (`CLOUD_KEY_LIMITER`), so the endpoint can't be used to test other people's keys.
2. Shape (the driver's `keyPattern`): Boat `boat_…` (or `box_…`), letters, digits, `_`, `-`; InstaBox `ibk_` and 52
   of a–z, 2–7.
3. A read must succeed (`checkKey`): Boat `GET /sandboxes?limit=1` (the key works and reads sandboxes), InstaBox
   `GET /v1/me`.
4. The key must not run the whole account (`checkAdmin`): Boat `GET /webhooks` must be refused as out of the key's
   scope (403 `api_key_*`, or 404), since a key that can read the webhooks is an admin key (or one from before Boat
   had scopes); InstaBox `GET /v1/orgs/<the key's workspace>/invites` must be refused `missing_scope` (a workspace
   admin's key gets it). Any other answer (429, 5xx) says nothing, so the key isn't kept: `503 providerUnavailable`,
   try again.
5. With a cloud computer on that provider already: reading it must succeed, so a replacement from another account or
   environment can't lock the computer out. A 404 is taken only when the saved key can't reach the computer either
   (deleted at the provider, or that key revoked): nothing is lost, and the new key lets the user delete the computer
   here.

Then it is sealed and replaces the earlier key. Each refusal says what to fix, in sentences the apps translate.

### Storage

- `cloud_credentials(user_id, provider, sealed, key_id, hint, problem, …)`, one row per user and provider.
- AES-256-GCM (WebCrypto, `src/secrets.ts`), a random 96-bit IV per seal, and the user id and provider as
  associated data, so a row copied to another account doesn't open.
- Keys: the Worker secret `CLOUD_SECRET_KEYS`, `id:base64` entries; the first seals, all open, and each row records
  its `key_id`. Rotating = put a new key first. A row is sealed again only when its user saves their key again, so an
  old entry stays while any row's `key_id` names it; a re-seal pass in the cron comes with the first real rotation.
  `wrangler secret put` can't be read back: keep each environment's value in its `.env` file
  ([local secrets](../guides/dev-app.md#local-secrets)), since losing it leaves every key unopenable.
- `hint` (`boat_…9abc`, as Boat lists keys) is the only part ever returned or logged.
- A D1 export, Time Travel copy or SQL read gives ciphertext. The running Worker can open it, as it must to wake a
  computer unattended.

### Using it

- Every provider call goes out with the owner's key (`providerSend(env, userId, provider, req)`), opened for that call
  only.
- Each driver turns its provider's refusals into Codync's `Failure`s (`cloud/machines/src/types.ts`): `keyRejected`,
  `keyExpired`, `keyMissingAction`, `keyCantReach`, `noGroup`/`groupForbidden` (Boat's environment), `planRequired`,
  `machineLimit`, `freeHoursUsed`, and the ones that aren't the owner's to fix (`busy`, `unavailable`, `notFound`, …).
- When the provider turns the key down for one the owner must fix, that `Failure` is kept on the key (`problem`, only
  on the key that call used); `GET /v1/cloud-computers` shows it as a sentence (`PROBLEMS`: no provider named, since it
  shows under the provider's name; Boat's environment sentences are Boat's own), the apps show the key row in red with
  *Replace*, and wakes stop retrying. The next call that shows it fixed clears it: any call that works, except that a
  missing action, the plan or the account's limits are cleared only by a write that works (a read says nothing about
  them). *Create* stays offered (it tries again, for an owner who fixed their plan there) unless the key itself was
  refused (`blocking`: rejected or expired), which only another key fixes.
- A host asking to sleep is recorded `stopping` before its relay takes it offline (and put back as it was if the relay
  keeps it, a device being connected, or can't be reached); a wake that read the machine before that and finds it
  running retries instead of taking the host back, when the relay took the host offline meanwhile. One whose stop is
  turned down (the key, busy, any refusal) is recorded `running` again, so the first wake takes the host back; a
  refused key leaves no wake able to read the machine, so that host is put back online at once (`/internal/awake` on
  its relay). A provider failing (5xx) may have stopped it all the same, so it stays `stopping`.
- For two minutes after a host asked to sleep, a machine the provider still reports running stays recorded
  `stopping` (`refresh`): its stop hasn't taken effect yet, so its host keeps waiting (`GET /v1/host/sleep`) and a
  wake keeps trying. After that, running means the stop failed: it is recorded running, and the host is taken back
  online.
- Write-only API: `GET /v1/cloud-computers` adds `providers`, each with its `credential` (`{hint, problem,
  blocking, updatedAt}` or `null`), never the key. `DELETE /v1/cloud-credentials/:provider` is refused while any cloud computer
  runs on that provider: without the key Codync couldn't put it to sleep or delete it, and it would keep billing its
  owner. Saving a replacement checks that it reaches every one of them.
- Deleting a cloud computer keeps the key (for another one). Deleting the Codync account stops each computer on every
  provider, then forgets them and the keys: the machines stay in the user's provider accounts for them to keep or delete. While the
  provider is busy the webhook fails, so Clerk sends it again and the stop is retried with the key still there.
- Audit rows: `cloudCredential.save`, `cloudCredential.delete` (never key material).

### More than one provider

Boat came first; InstaBox (InsForge) is the second (owner, 2026-11-08: "不要直接在 UI 上面綁定死只有這一個", then
"幫我支援一下這個 cloud 服務"). Nothing in the apps names a provider:

- `GET /v1/cloud-computers` lists `providers`, each `{provider, name, region, keyPrompt, setup: {page, commands},
  credential, offer?: {label, detail, url}}`, from `PROVIDERS` in `cloud-credentials.ts`. `offer` is the provider's own
  free plan (InstaBox only; older clouds leave it out). A new provider shows up in every app with no release.
- Keys are per provider (`PUT/DELETE /v1/cloud-credentials/:provider`, one `cloud_credentials` row each).
  `POST /v1/cloud-computers {provider, name}` creates a computer on one the user gave a key; `provider` may be left
  out when there is only one.
- Several per account (owner, 2026-11-09: "一個帳號支援加入多台 cloud computer…使用者可以為不同電腦命名", one on Boat
  and one on InstaBox at once): up to `CLOUD_COMPUTER_LIMIT` (3), each on the provider the user picks and with the
  name they give it (`cloud_machines.name`, renamed with `PATCH /v1/cloud-computers/:id`). The name is the computer's
  in the account and its host's own (`/v1/host/state` carries it; a re-register keeps it), so every app shows it: the
  host tells apps connected to it with a `name` event, the others read it in their next `hello`.
- Apps: Computers → Codync Cloud lists the account's cloud computers (provider logo, name, provider · region · state,
  or what to fix in red while the provider turns its key down; a `⋯` menu with *Rename…*, *{provider} API key*,
  *Delete snapshots*, *Delete cloud computer*). The card's **+** opens *New cloud computer*: a name (suggested "Cloud
  computer", "Cloud computer 2", …) and every provider with its logo, where it runs and its key (or *Add a key first*).
  A provider is reached only where it is picked (owner, 2026-11-09: no separate Cloud providers row; picking a provider
  shows only that provider's options): a row without a working key, the `⋯` inside a row with one (*{provider} API key*,
  and *Remove key* while no cloud computer runs there), or a computer's `⋯` opens *{provider} API key*, a sheet with just that provider: its key (last four, replace, remove) or a field to
  paste one, the provider's free plan when it has one (InstaBox: *Get a free sandbox*, its pricing page; its row in
  New cloud computer says *Free sandbox available* until it has a key), and *How to make a key* folded under it; saving one goes back to New cloud computer with it picked. A key
  the provider turned down for its own sake (revoked, expired) leaves its row unpicked; one turned down for the plan or
  the account still creates, which tries again. At the limit **+** still opens, with *Create* off and why, so every
  provider's key stays in reach.
- The Worker speaks to providers only through a `Driver` (`cloud/machines/src/types.ts`; `drivers` in its index): the
  same verbs for each (`create`, `get`, `sleep`, `wake`, `destroy`, `checkKey`, `checkAdmin`, and readers that turn
  answers into `Machine`/`Outcome`), plus members present or not for what differs in kind, which the Worker decides
  from, never from a provider's name:
  - `setup`: Boat has none and takes the claim ticket and setup script in the create call; InstaBox takes nothing at
    create, so once the machine runs (`setUp`, on the apps' reads while it prepares and the cron's) one `exec` writes a
    new ticket (the host has its whole 30 minutes to install and claim) and starts the script detached. One InstaBox
    paused before it got them is started again for it.
  - `backup`: Boat drops a stopped sandbox's own snapshot after 14 days, so it gets a never-expiring backup on day 10
    (counted from its sleep request, however late the stop is seen) and comes back from it; InstaBox keeps a stopped
    or paused machine's disks until it is deleted, so it has none. Deleting a computer deletes its backup first: one
    that can't be deleted yet keeps the computer listed, so the delete can be tried again.
  - `deleteHistory`: Boat keeps automatic snapshots of earlier states (*Delete snapshots* erases them); InstaBox keeps
    none, so there is only the backup (if any) to erase.
  - Sleep: Boat stops (the disk is kept, memory isn't); InstaBox pauses (memory and processes kept, back in seconds).
    InstaBox also pauses a machine idle for 30 minutes on its own (a free machine can't turn that off): the next wake
    starts it, as after Codync's own sleep.
- Adding a provider: a driver beside `boat.ts` and `instabox.ts`, registered in `drivers`, a `PROVIDERS` entry (name,
  region, key prompt, how to make the key, its free plan (`offer`) if any, with `label` and `detail` added to every
  language catalog) and its base URL override for tests in `sendWith`. No migration: no CHECK
  names providers. `cloud/machines/test/` holds each driver's request and answer tests.

### What Codync can still do

The Worker, and so whoever controls Codync's Cloudflare account, can open the key and create, stop, resume and delete
sandboxes in the user's `codync` environment on Boat, including a new one from the backup with its own setup script:
that's how it provisions. It can't reach their other Boat sandboxes or their account. On InstaBox it can do the same
to every machine of the key's workspace, and run commands there (`exec`). It also keeps each cloud computer's vault
key (sealed with `CLOUD_SECRET_KEYS`, [below](#agents-and-the-hosts-secrets)): together with that access to the
machine (InstaBox `exec`, or a Boat sandbox made from the backup with its own setup script) it could open every secret
that computer's keeper holds. Say so in the privacy policy. Follow-up: a refused key also means an idle computer can't
be stopped and keeps billing; tell the owner by push, not only in the apps.

## What the user gets

A Codync computer in the cloud that is there when the phone asks for it: bots keep working with the
laptop closed, routines fire on time, the iPhone, Android and desktop apps reach it exactly like a Mac at
home. The user signs in to their own agents (Claude Code, Codex…) on it with their own subscriptions.

Entry points: the welcome screen of a device with no computer ("Start a cloud computer"), and Computers
→ add. One progress screen, then the bot list.

## Verified on a real InstaBox machine (2026-10-24, host 2.24.0)

| Check | Result |
|---|---|
| Machine | Debian 13, Firecracker kernel 6.18, x86_64, 4 vCPU / 8 GiB (free tier), systemd 257, user `user` with passwordless sudo, lingering already on, home on the persistent `/data` disk |
| Install | `install.sh --host-only` + `codync-host install`: service running on the first try, no changes needed |
| Relay | registered with `api.codync.dev` and `relay ready` 4 s after start; no inbound port |
| Network | outbound IPv4 + IPv6 direct, no proxy; DNS 1.1.1.1 |
| Preinstalled | Node 22, Claude Code, Codex, Docker, git, gh |
| Remote screen | `codync-screen` can't start (`libgstapp-1.0.so.0` missing on the desktop template) and the host restarted it every 30 s (87 times in 45 min): follow-up 1. The desktop-less `basic` template avoids it |
| Public ports | requests reach the VM from `127.0.0.1` with `X-Forwarded-For` set: publishing 19222 would defeat the host's loopback-only check. The managed design needs no public port |
| Idle pause | **never happened**: with the host running, the free machine stayed up 40 min after the last SSH, port visit and dashboard terminal (limit: 30 idle min). InstaBox's idle heuristic can't be the sleep switch: Codync must pause machines itself |
| Pause / resume | pause took ~5 s; resume to `running` 8.3 s from the dashboard click. Processes survived. The guest's monotonic clock and uptime don't count the pause (192 s paused, uptime +10 s); wall clock is corrected |
| Reconnect after resume | **102 s** from `running` to `relay ready`: the host only noticed the dead socket after 90 s of silence on its monotonic clock (`SILENT` in `host/src/remote/relay.rs`), which skipped the pause; pings on the dead socket raised no error. Fix in the host (below), target: relay ready ~2 s after resume |

## Boat first

Why: a stopped Boat sandbox keeps its disk free for 14 days, so a user awake 1.7 h a day costs about
$0.93/month on `small` (2 vCPU, 4 GB, 12 GB disk, $0.018/h; about $2.9 at 5.3 h every day), and TOS-8
lets a product give each of its users a sandbox without asking. Boat's own guide calls
this pattern "stop and resume around usage, $1 to $5 per month for a typical user".

What changes against the generic design below:

| Topic | On Boat |
|---|---|
| Image | Boat's stock image (it already has systemd with linger, Node, Claude Code, Codex); each user's sandbox is created `noEnv` (no account secrets inside), without auto-stop or monitoring, with the one-time claim ticket written to `~/.codync/managed.json` by Boat's `files`, and a setup script that runs the public host install and `codync-host install`. A saved template snapshot can replace the setup script later if the ~9 s install matters |
| Sleep | no memory-keeping pause. When the host reports idle over the relay, the Worker calls `stop` (disk snapshot); Boat has no idle timer of its own |
| Wake | the relay DO asks the Worker, which calls `resume`: a reboot in "a few seconds" for any amount of data (lazy hydration), then systemd starts the host, which registers and connects. The 102 s reconnect bug doesn't apply (a reboot, not a pause); boot-to-`relay ready` is the number to measure |
| Start budget | create, fork and resume each count as a start, account-wide: 12/min, 60/h, 200/day on the $20 plan up to 90/600/2,400 on $2,000, raised for committed accounts. Budget = users × wakes per user per day; jitter routine wakes off the top of the hour; on `429`, keep the mailbox and retry |
| Long sleep | Boat's automatic snapshots aren't permanent: a stopped sandbox's newest one expires 14 days after the stop (email on day 11); only saved snapshots never expire. On day 10 of a stop the Worker saves a saved snapshot: plans include 10 per $20 of plan (50 on $100, 250 on $500), then $1.70/month each whatever the size, and a new sandbox starts from one in seconds. R2 is cheaper per GB ($0.015/GB-month, so ~$0.18 for 12 GB, free egress) but needs an export (a resume, a start), an upload and a slower restore: add it only once long-idle users outnumber the included saved snapshots |
| Capacity | `--fail-fast` answers in ~1.5 s when no machine is ready (`503 no_ready_machine`, nothing billed, no start counted): retry, then tell the phone "starting…" |
| Region | EU only (Germany, Finland, France); fine for chat from Asia, slower for live terminals |
| Plan | $20/month (paid back as compute) covers the prototype and the first users; trial: 7 days, card, 25 h total, 2 sandboxes, auto-stop ≤2 h |

### Build order on Boat

1. **Provision for a signed-in user** (no app UI yet): Worker `POST/GET/DELETE /v1/cloud-computers` (Clerk),
   D1 `cloud_machines`, the Boat calls in `cloud/machines/`
   (sans-I/O), and in the host a first-boot claim from `~/.codync/managed.json` (written by Boat's `files` before
   the sandbox is ready, deleted once used) plus `Approval::Auto`. The computer then appears in the account's
   Computers list on every app, as *Codync Cloud* with a cloud icon (`device: "cloud"`; the machine's own hostname
   changes on every wake, owner 2026-11-02); "Ask for access" is approved by the host itself. Tested end to end against a local
   Worker that the sandbox reaches through `boat forward --reverse`, so nothing is deployed (passed 2026-10-24:
   [trial log §11](codync-cloud-boat-trial.md#11-phase-1-end-to-end-2026-10-24)).
2. **Sleep and wake**: host idle → `stop`; relay DO → Worker → `resume` on a device connection, mailbox item, hook
   or `wakeAt`; 14-day snapshot policy; start-limit budget. Sleep and wake are built and passed on Boat on
   2026-10-24 ([trial log §12](codync-cloud-boat-trial.md#12-phase-2-sleep-and-wake-2026-10-24)), with the day-10
   backup and the update before sleep (`POST /v1/host/sleep` answers `{update}`); backups, a cold wake and a
   restore from a backup passed on Boat's paid plan on 2026-10-31 ([trial log
   §15](codync-cloud-boat-trial.md#15-updates-backups-and-a-cold-wake-on-the-paid-plan-2026-10-31)). Still open:
   a start-limit budget beyond retrying (decided once real use shows the need, owner 2026-10-31).
3. **App UI** on iOS, Android and desktop (provisioning entry, "preparing your cloud computer" for the ~10 s until
   it is claimed, Cloud badge with provider and region, an automatic access request for the account's cloud
   computer instead of the "Ask for access" tap, a sleeping computer shown as ready ("wakes when you message")
   rather than offline, delete), user manual.

The app UI talks to the Worker, never to a provider: the Worker lists the offers it can provision (provider,
region, wake speed, size) and the apps show them as choices, so adding InstaBox or Fly to `codync-cloud`
later shows up as new options (for example "Europe, wakes in seconds" on Boat beside "Asia, wakes in under a
second" on Fly) with no app release needed for the provider itself.

### Updates on Boat

- **Codync's host updates itself.** The host installed by the public script is a standalone release, so its
  automatic updater is on (checked on the trial VM: `Host 2.24.0 · upToDate · Automatic updates: on`): once a
  day, while awake and no bot is working, it fetches the signed release, installs it and restarts under systemd.
  The release gate already keeps any version the App Store iPhone app can't talk to from becoming "latest", so
  cloud hosts follow the same compatibility rules as every other host.
- **Sleeping machines**: a cold resume starts a new host process, which checks about a minute after waking and
  waits while bots work. Step 2 adds "update, then sleep": the host checks for a release when it decides it is
  idle, before asking to be stopped, so a wake never lands on an old host or restarts mid-conversation.
- **New machines** install the latest release at creation (the setup script), so there's no stale template.
  A per-release saved-snapshot template can come later to cut the ~8 s setup.
- **The rest of the machine**: a Boat snapshot keeps only what the user changed; the OS and the preinstalled
  tools come from Boat's current image on every cold resume (the trial VM had Claude Code 2.1.310 and Codex 0.164.0
  from it). Copies a user installs in their home stay theirs and update themselves.
- Fleet view: each cloud host's version is already in `computers.version` (sent at registration); a staged
  rollout or a forced update can be added if a bad release ever needs it.

### Snapshots, backups and deleting them

- Boat keeps snapshots of a sandbox's disk as it runs and at every stop; a stopped sandbox's newest one is the
  computer itself and expires 14 days after the stop. After 10 days asleep the cron saves a backup (a Boat saved
  snapshot, kept until deleted; saved again if the computer woke since); a wake whose own snapshot expired starts a
  new sandbox from the backup.
- **Users can delete them** (owner, 2026-10-24): *Delete snapshots* erases the saved history (earlier snapshots
  and the backup) and keeps the computer, only while it is awake (asleep, its snapshot is all it has).
  *Delete cloud computer* erases the machine, all its snapshots and the backup (Boat keeps saved snapshots after
  their sandbox is deleted, so the Worker deletes the backup itself).
- A provider that keeps no history of earlier states and no backup (InstaBox: a stopped or paused machine keeps its
  disks until it is deleted) has nothing for *Delete snapshots* to erase: the cloud computer's `snapshots` is false and
  the apps don't offer it.

### Versions the user's apps can handle

A self-updating cloud host must never outrun the user's own apps: if a release raises `MIN_APP` past the app on
their phone, the phone shows *Update this app* and stops syncing. Two layers:

1. Already there: the release gate keeps a release that raises `MIN_APP` from becoming "latest" until the App
   Store has that iPhone app, and `MIN_APP` only rises on renames and removals.
2. For cloud computers, do what MausBot does with its homes: Codync's cloud owns the host version instead of the
   host's own updater (`managed.json` turns automatic updates off). Every signed-in request from an app to the
   Worker carries the app's version (MausBot: `x-openmausbot-version`), so the Worker knows which versions an
   account's devices run; it upgrades that account's cloud host (Boat exec `codync-host update`, ideally right before
   it sleeps) only to a release every device used in the last 30 days can talk to, rolls a release out machine by
   machine and stops on a failed health check. Contracts stay additive, and an app too old for a feature gets an
   explicit "update the app" answer instead of a broken screen.
3. Later, for self-hosted computers too: each client sends its app version in `hello` (additive), the host keeps
   the last version per device, the signed release manifest carries the release's `minApp`, and the automatic
   updater installs a release only when every device that used this computer in the last 30 days meets it.
   Otherwise it waits and the apps of the devices holding it back show "Update Codync to get your computer's newest
   version" (not blocking). A device unused for 30 days doesn't hold updates; when it returns too old, it gets the
   usual *Update this app*.

### How the cloud learns each app's version

- Every request an app makes to the Worker carries `Codync-App: <platform>/<version>; minHost=<version>`
  (`ios/2.25.0; minHost=2.20.1`, `android/…`, `desktop/…`; the terminal UI runs on the host and needs none). That
  includes the relay WebSocket upgrade every app opens to reach a computer (`/v1/relay/device/…`), so the Worker sees
  it on every connection, not only on sign-in. The app names its own `minHost`, so the Worker never has to look it up.
- Those requests are signed by the device key, so the Worker stores the version on that device's row
  (`devices.app_version`, `min_host`, `app_seen_at`): per account it knows which versions its devices ran in the
  last 30 days.
- Apps released before this send nothing. Unknown never holds an update or blocks an app (the compatibility rule:
  an unknown is not a reason to lock someone out).
- Each release's `minApp` comes from the release itself: the Worker's cron reads its channel's latest
  `codync-host-compat.json` (the version and `MIN_APP` from `host/src/compat.rs`; GitHub on main, the
  [dev release channel](../guides/development.md#dev-release-channel) on dev) and keeps each version it sees
  (`host_releases`). It is its own asset, not a field in the signed manifest, which would break older hosts
  (`release.rs` parses that with `deny_unknown_fields`). A release that is *latest* for less than one cron interval
  (15 minutes), or that the gate skips past, is never seen, so the soft update can't pick it; one more release
  between two others is rarely what an old app needs.

### Cloud soft update (雲端弱更新, the default)

- Trigger: a new host release reaches GitHub "latest" (the release gate already passed).
- Target: the newest release whose `minApp` every device seen in the last 30 days meets (the oldest app version
  decides), not only "latest". Example: host on 2.24, latest 2.27 needs app 2.26, 2.25 and 2.26 need 2.21, oldest
  app 2.22 → the host goes to 2.26 and still gets the fixes in between. `codync-host update --target <version>`
  installs a given release.
- When the target is older than "latest", the devices holding it back see a non-blocking "Update Codync to get
  your cloud computer's newest version".
- `minHost` too: an app can need a newer host than the cloud has (a desktop app that updates ahead of a slow
  phone, or a phone that updates past the host). The target must also be at least the highest `minHost` among the
  devices of the last 30 days.
- **An app that needs a newer host updates the cloud computer first** (owner, 2026-10-24): as soon as a device
  connects with a `minHost` above the cloud host, the Worker tells the host (`cloud.changed`), and the host installs
  exactly that version at once, even mid-task (bots' sessions resume). Exactly that version is the oldest release
  the new app works with, so the account's older apps keep working. When `dev` bumped past it before it was released,
  the order is the oldest release in `host_releases` above it. Measured on Boat: the app connected, and the host
  was back online on the new version **4.6 s** later. A sleeping computer wakes for the app's connection first.
- When (the regular soft update): when the host is idle and asks to sleep, `POST /v1/host/sleep` answers
  `{update}` instead of stopping it; the host installs, restarts, and sleeps after its next idle period, so a wake
  never lands on a restart. Cloud hosts have their own automatic updater turned off (the `cloud_managed` mark
  from the claim ticket).
- Rollout: internal accounts first, then batches; a host that isn't `relay ready` within a few minutes of updating
  stops the rollout. (Not built yet: every cloud computer takes its target when it next sleeps.)
- Nothing is forced on anyone.
- The owner can skip ahead: *Update* on the cloud computer's Codync row installs the latest release now, even past
  what the oldest app can handle ([Delivery](#delivery)).

### Cloud hard update (雲端強更新, emergencies only)

- Trigger: an operator decision for a security hole or a critical bug: Worker settings `HOST_FLOOR` (minimum host
  version) and `HOST_FLOOR_REASON`, and only if needed `APP_FLOOR_IOS` / `APP_FLOOR_ANDROID` / `APP_FLOOR_DESKTOP`
  with `APP_FLOOR_REASON`.
- Hosts: every cloud host below `HOST_FLOOR` gets an update order with `force` in its state; the cron tells awake
  ones within 15 minutes, and a sleeping one reads it when it next wakes and connects (seconds before devices do a
  lot; the relay doesn't refuse them). For self-hosted computers the Worker refuses registration below the floor
  (`426 hostUpdateRequired` with the reason), their apps show the existing *Update Codync on <computer>*, and the
  host starts an update check as soon as it is refused.
- Apps: below their platform's floor, every Worker request and relay connection is answered `426
  appUpdateRequired` with the reason; the apps show the blocking *Update this app* card with it (App Store
  expedited review, Google Play, the desktop updater).
- Prefer fixes that keep compatibility, so only hosts move; raise `appFloor` only when the app itself is vulnerable
  or the fix can't be compatible. Tell affected users by push and email.

A planning agent also wrote an alternative, [always follow *latest*](codync-cloud-updates.md) (no app versions in
the cloud; apps ask the host to catch up). Not adopted: the soft update above stays (owner, 2026-10-24).

### Sign-in

- **Codync account**: required for Codync Cloud (signed-in users only). A cloud computer accepts only that
  account's devices (`Approval::Auto`); no QR pairing. Since account access then means computer access, require
  MFA or a passkey on accounts that have a cloud computer (Clerk can enforce it).
- **Agents**: like MausBot's Cloud welcome, a cloud computer with no agent signed in shows an agent sign-in first
  (Claude Code's paste-the-code flow, Codex's `codex login --device-auth`, or an API key), and every bot shows it
  instead of a chat that fails its first turn. Under it is the existing agent
  setup terminal (built: Claude Code prints a URL and takes the code back; Codex needs its device-code login). The
  terminal runs over the end-to-end channel, so the tokens never pass through Codync's servers; they live on the
  machine and can be revoked at Anthropic/OpenAI at any time. Deleting the cloud computer erases them with it.

### Encryption

- **In transit**: already end to end (X25519, ChaCha20-Poly1305): the relay sees ciphertext only.
- **At rest, to add**: keep `~/.codync` (transcripts, identity key) and the agent sign-in folders (`~/.claude`,
  `~/.codex`, …) in an encrypted folder (fscrypt or gocryptfs) whose key is never on the disk. The Worker keeps one
  key per machine (sealed with a Worker secret) and writes it to `/tmp` through Boat's files API on every start or
  resume (`/tmp` isn't in snapshots); the host unlocks the folder before it starts. A snapshot copy, wherever Boat
  stores it, is then ciphertext. Cost: one extra API call per wake.
- **For comparison**: MausBot's homes get encryption at rest from Fly volumes (on by default), pass machine secrets
  to their server over a pipe so no process can read them from its environment, and encrypt API keys typed on the
  phone to the computer's QR-pinned key (HPKE). Codync's end-to-end channel already covers the last one; Boat
  documents no disk encryption, so the encrypted folder above is what closes the gap.
- **Honest limit**: while the machine runs, its operator (Boat) can in principle read its memory and open files; no
  software encryption prevents that without confidential VMs (AMD SEV-SNP, Intel TDX), which Boat doesn't offer.
  Say so in the privacy policy.

### Agents and the host's secrets

Before this, a cloud computer's agents ran as the host's own Linux user, with passwordless sudo on InstaBox's and
Boat's images: they could read the vault and its key, the database and the computer's identity, and so could an
InstaBox key holder through `exec`. Decided (owner, 2026-11-09, after comparing [prior art](#prior-art-agent-sandboxes)):
**agents on a cloud computer have no sudo**, and the secrets move to a process they can't read. Built the same day on
`feat/cloud-boat`; a computer of the user's own is unchanged (everything below is gated on the cloud's claim ticket,
`remote::managed::is_managed`).

- **Three users.** The provider's own user (`user`, passwordless sudo) is left alone: the provider's machine agent
  runs as it and needs it. On Boat that agent (`ascii-agents-server`) uses `user`'s sudo for its snapshots (taking it
  away made `SNAPSHOT-INC` fail), keeps its credentials in `/opt/ascii-agent/.env` readable by `user`, and runs with
  `CAP_DAC_READ_SEARCH` (it reads any file): anything running as `user` could drive it. So Codync's host and its
  agents run as **`codync-agent`**, a user the first-boot setup makes (`cloud/src/cloud-setup.ts`, as `user` while it
  has sudo), with no group but its own, never in sudoers, its home beside `user`'s on the disk the provider keeps
  (`/data/codync-agent` on InstaBox, `/home/codync-agent` on Boat); the setup moves the claim ticket the provider left
  in `user`'s home into it, turns on linger and installs the host's user service there. The secrets go to **the
  keeper**, the same binary run as `codync-host keeper` under the system user `codync` (`host/src/keeper/`): a systemd
  system unit with its state in `/var/lib/codync-keeper` (0700, its own copy of the binary there too, which only it
  replaces) and its socket at `/run/codync-keeper/keeper.sock`. The setup also sets `kernel.yama.ptrace_scope = 1`
  where the kernel has Yama (InstaBox's has none), and the managed host makes itself non-dumpable, so its agents, the
  same user, can't attach to the running host or read its memory. **Limit**: the host is its agents' user, so that
  guards only the process already running. An agent (a prompt injection, say) can change what runs as the host next
  (its binary in `~/.local/bin`, its user unit, its user manager's environment) and restart it, and then gets every
  secret the user saves after that in plain text on its way to the keeper; secrets saved before stay safe, since the
  keeper never hands a proxied one back. Follow-up: run the host under a user of its own that starts agents as
  `codync-agent` (through the keeper or a small root helper), or have the apps seal new secrets to the keeper's key so
  the host relays only ciphertext. The host finds the keeper's socket at its fixed path only, never from its
  environment. Agents install into their home only (`npm`/`pnpm`, `uv`/`pip` in a venv, `cargo`, `go install`,
  `~/.local/bin`) on top of what the image ships in `/usr`; system packages: not for now (owner: "先暫時還不需要 codync-apt"), later a narrow Debian-only installer or a
  root command the owner approves, if users need them. Whoever can run commands as `user` (the provider; on InstaBox
  any holder of the workspace key, Codync included, through `exec`) can still become root: that is the provider's side
  of the line, as before. A managed host that can `sudo -n true` logs a warning and reports it (`cloud_keeper_checked`).
- **The vault key off the disk.** The Worker makes one key per cloud computer at create, sealed with
  `CLOUD_SECRET_KEYS`; the first claim names the keeper's Ed25519 key (`keeperKey`, once), and only a request that key
  signs gets the vault key (`GET /v1/keeper/vault-key`). The keeper fetches it at each start (retrying every 2 to 5 s
  for its first ten minutes, while a new computer's claim may land, then up to every minute) and keeps it in memory;
  its `secrets.json` (XChaCha20-Poly1305, the slot and kind as associated data) is ciphertext on disk and in every
  snapshot. The keeper's signing key is on the disk too (`identity.json`), so a copy of the disk (a snapshot, a
  backup) can still fetch the vault key, until deleting the computer erases it. Follow-up: tie the request to
  something off the disk (the provider's attestation, a per-boot secret the Worker hands over through the provider's
  API). Until it unlocks, a host operation that needs a secret waits up to 15 s, then says the keeper is still getting
  its key.
- **Use without seeing.** A secret is *proxied* or *visible*. A proxied one never leaves the keeper: a client asks
  for a grant and gets `http://127.0.0.1:19230/t/<token>`, and the keeper forwards each request to the record's fixed
  upstream with the real header added (client auth headers and named query parameters dropped; only the methods an API
  takes, so no `TRACE` echoes the key back; redirects returned, not followed, so a key never follows one; responses
  streamed; OAuth access tokens refreshed by the keeper itself, before expiry and once on a 401, and the sign-in
  dropped once it can't be renewed, which the host then shows as signed out). Proxied: agents'
  `ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY` (the agent gets `VAR=codync-keeper` and its base-URL variable pointed at a grant,
  unless the user set that base URL themselves), voice keys, the Composio key, and remote connectors' headers and
  OAuth tokens (the `codync-host mcp remote` bridge, which runs as the agent user, only ever gets a grant). Visible
  (a process has to receive them, so agents can read them): website logins, the 1Password token, stdio connectors'
  environment, other agent environment variables, `op://` references, OAuth client registrations, remote connectors'
  URLs (a token in their query or user part too), `http://` connectors' headers. An agent's grants last as long as its process; any local process may ask the keeper for a grant
  to any proxied secret (it can use it, never read it).
- **Keeper updates.** A managed host whose keeper is older asks it to `update`: the keeper takes only a newer, signed
  release, replaces its own binary and exits for systemd to restart it.

Still readable by agents, by design: their own subscription sign-ins (`~/.claude`, `~/.codex`: the CLIs need them),
the visible secrets above, and the workspace. No outside service: the Worker is the key custodian; Cloudflare AI
Gateway is the candidate if spend limits or one-tap revocation are wanted later.

#### Prior art: agent sandboxes

Read 2026-11-09 from their source:
- **OpenMausBot**: each engine process in a bubblewrap box (own user/pid/net namespaces, seccomp, the data folder
  replaced by an empty one, egress only through a proxy to public addresses); the image ships no sudo; a broker swaps
  base-URL-capable keys for per-process `omb_t0_` tokens. Headless installs keep API keys in plaintext 0600 files.
- **CopilotKit/OpenBot**: secrets only on the server (AES-GCM, key in the server's environment); the model loop has no
  shell and acts through the server. Where shell and server share a container: separate uids and sudo limited to one
  apt wrapper (a plain `sudo apt-get` was escalated with `-o APT::Update::Pre-Invoke`).
- **bloks**: plaintext 0600 `config.json`, readable by agents (it says so); GitHub tokens through a per-turn proxy
  handle with an allowlist and an audit log.

### Security on Boat

- Each user gets a separate VM (Boat's security boundary is between sandboxes and accounts); sandboxes are created
  with `noEnv`, so no Codync secret is inside. The claim ticket is single-use, short-lived and deleted after use.
- Create user sandboxes with `monitoring: false`: by default Boat keeps command output for 90 days and other
  process output for 72 hours. Never pass secrets on a command line (Boat keeps command history); use `files`.
- Each user's Boat key is limited to the `codync` environment and the actions Codync needs
  (`sandbox.create/read/stop/resume/delete`, `snapshot.read/write` for *Delete snapshots* and backups), and sealed in
  D1 ([Bring your own cloud](#bring-your-own-cloud)).
- Disks: Boat snapshots the disk every minute while running and at every stop; resume restores the newest one.
  Boat documents no encryption at rest for sandbox disks, and snapshot copies may be stored outside the EU: say so
  in the privacy policy. SOC 2 is in progress at Boat; uptime 99.95%.
- With `Approval::Auto`, whoever controls the Codync account (or Codync's cloud) can add a device: recommend MFA on
  the account; signed-in only keeps every machine tied to a person.
- On their own Boat account, users answer to Boat for what they run; Codync's part is deletion on request (`DELETE`
  erases the sandbox and its snapshots).

## Architecture

```
            ┌──────────── Codync cloud Worker (cloud/, Cloudflare) ────────────┐
            │ accounts (Clerk) · cloud_machines (D1) · provisioning workflow   │
            │ provider token (secret) · rollout · backups · health             │
            └───────┬──────────────────────────────┬───────────────────────────┘
       creates/updates/deletes             claim ticket at first boot
                    ▼                              ▼
  ┌── one per user: Fly app (own private network) or InstaBox machine ──┐
  │  machine: codync-cloud image, `codync-host serve`, HOME=/data        │
  │  volume /data: ~/.codync, repos, agent sign-ins                      │──outbound WSS──▶ relay DO ◀── phone / desktop
  │  (Fly only) https://<app>.fly.dev → /health, /channel                │◀──wake: GET /health (Fly) or Worker resume (InstaBox)
  └──────────────────────────────────────────────────────────────────────┘
```

| Piece | Fly Machines | InstaBox |
|---|---|---|
| Isolation | one Fly app per user, created with its own private network | one machine per user |
| Machine | Firecracker; root disk reset on every cold start; no systemd: the image's entrypoint is `codync-host serve` (under `tini`), `HOME=/data` on a volume | Firecracker; full Debian VM with systemd and a persistent root disk; `codync-host install` as today |
| Image | `codync-cloud` Dockerfile in this repo: static host, Node 22, Claude Code, Codex, git, gh; published per release; the Worker rolls it out machine by machine and reverts on a failed health check | snapshot built from the same script; hosts self-update or the Worker re-images |
| Size / region | shared-cpu-2x 2 GB (suspend works up to 2 GB; bigger machines stop instead), region nearest the user (Tokyo/Singapore for Taiwan) | 2 vCPU / 4 GiB, US only |
| Sleep | the host decides it is idle, tells the relay when to wake it, then suspends itself through the machine's local `/.fly/api` socket (no token); stop above 2 GB. Fly's own auto-stop stays off: it counts only proxy traffic, and the host's relay socket is outbound | the host tells the relay; the relay asks the Worker, which calls InstaBox suspend |
| Wake | the relay DO sends `GET https://<app>.fly.dev/health`; Fly's proxy resumes the machine (hundreds of ms per Fly) or boots it | the relay asks the Worker, which calls InstaBox resume (8.3 s measured from the dashboard) |
| Backups | daily volume snapshots (Fly) | InstaBox snapshots |
| Terms | standard "internal use, no sublicensing" wording; Fly's own blueprint recommends one app per user for user environments, and MausBot sells this on Fly: low risk, a confirmation email is cheap | no reselling access; its terms also cover the end users of services you run (§4c): same reading, confirm with InsForge |

Both keep the data path unchanged (host ↔ relay Durable Object ↔ devices, end-to-end encrypted) and keep
every provider call inside one file of the Worker (`cloud/src/fly.ts` or `cloud/src/instabox.ts`), with
`provider` stored in `cloud_machines`. No interface until a second provider ships.

On Fly the machine is also reachable at `https://<app>.fly.dev`. Requests arrive from Fly's proxy, not
loopback, so the host's local API stays closed; only `/health` and the end-to-end encrypted `/channel`
answer, which also gives phones a direct path (advertised with `CODYNC_EXTERNAL_URL`) whose first
request wakes a sleeping machine by itself.

### Flows

1. **Provision** (Worker, Cloudflare Workflows, ~1 min): create the app/machine and volume in the
   user's region → start it with `CODYNC_MANAGED=1`, `CODYNC_APPROVAL=auto` and a one-time claim
   ticket (claim ID + nonce for this user, as a provider secret) → on first boot the host makes its
   identity, signs the claim itself and completes it with a new host-signed call → D1 `cloud_machines`
   row. No remote command execution needed, so this works the same on any provider.
2. **Access.** The owner's devices are signed in to the same account: the computer shows up in
   Computers, the app asks for access automatically for a Codync Cloud computer, the host approves on its
   own (`Approval::Auto`, already built). No QR, no 6-digit code.
3. **Agent sign-in.** From any device, in the agent's setup terminal (already built, runs on the host):
   Claude Code prints a URL and takes the code back. Agents that only sign in with a browser on the same
   computer need their device-code/terminal method: follow-up 3.
4. **Sleep and wake.** The host owns the idle decision (no turn or routine running, no device use for
   10 minutes, awake at least 2 minutes, next routine at least 5 minutes away, as MausBot does), sends
   the relay its `wakeAt`, and goes to sleep (table above). The relay DO wakes it when a device connects,
   a mailbox item or routine webhook arrives, or `wakeAt` comes; the host reconnects (host fix 1) and the
   mailbox drains.
5. **Delete.** "Delete my cloud computer" in Settings deletes the machine and its volume at once.

## Changes

| Part | Change |
|---|---|
| `cloud/` | `routes/cloud-computers.ts` (create, status, delete), one provider file (~150 lines), a provisioning workflow, `wake.ts` (`relay.ts` is past 500 lines), a host-signed claim-completion route, D1 migration (`cloud_machines`), crons for rollout, backups and abuse checks |
| Relay DO | accept `wakeAt` from the host; wake a sleeping Codync Cloud computer when a device opens, a mailbox item or hook arrives, an access request comes in, or `wakeAt` passes (rate limited) |
| Host | (1) reconnect as soon as the wall clock jumps ahead of the monotonic clock (resumed VM; maybe a Mac waking too) instead of after 90 s of silence; (2) managed mode from the environment: claim ticket on first boot, auto approval, self-update off; (3) idle decision + `wakeAt` + self-sleep (Fly socket); (4) memory admission for agent processes; (5) a request carrying `X-Forwarded-For` is never loopback; (6) follow-up 1 |
| Image | `codync-cloud` Dockerfile + release workflow (Fly), or the snapshot script (InstaBox) |
| iOS, Android, desktop | provisioning progress, Cloud badge and sleeping state on the computer row ("Sleeping, wakes when you message"), Computers → Codync Cloud (status, delete) on all three. Each app is a full client of it (chats, bots and their settings): the desktop app reaches it over the relay's encrypted channel like the phones (owner, 2026-11-02), connected only while its window is in use so the computer can still sleep. |
| Terminal UI | none: it always runs on a host, it doesn't create computers |

## Trust and privacy

Self-hosted Codync keeps the cloud unable to grant access. Codync Cloud can't: Codync's Worker provisions the
user's machine (with their key, limited as in [What Codync can still do](#what-codync-can-still-do)), the machine
runs `Approval::Auto`, and the provider operates the hardware. Say so in the product and the privacy policy: the
user's code, agent sign-ins and transcripts live on a machine in their own account with the provider that Codync
provisions. Transit stays end-to-end encrypted. What the key Codync holds can do differs by provider: a Boat key can't
run commands on or read the running machine; an InstaBox key can run commands on every machine of its workspace,
which is how Codync installs the host there.

## Costs

Boat by the hour: `small` (2 vCPU, 4 GB, 12 GB) $0.018, `default` (4 vCPU, 8 GB, 50 GB) $0.036, `large` (8 vCPU, 16 GB,
125 GB) $0.072.

- **Measured usage** (PostHog, 2026-10-18..24, env main, internal and test computers excluded; hours with an agent
  turn running plus the 10 minutes before sleep, in 10-minute slots, scaled to a month): computers used 5–7 days
  of the week (64): average 149 h, median 73 h, p75 201 h, p90 358 h, max 669 h; used 3–4 days (68): average 43 h;
  used 1–2 days (349): average 9 h. Time the phone is open without a turn isn't counted, so real awake time is
  somewhat higher. Measure again once cloud computers have users.

InstaBox unit prices (2026-10-24): vCPU $0.004/h, memory $0.00225/GiB·h, disk $0.00011/GiB·h,
egress $0.05/GB. Disk counts root + `/data` (16 + 20 GiB) whether running or paused.

| Machine | Always on | Asleep when idle, 4 h active/day | 8 h active/day |
|---|---|---|---|
| 2 vCPU / 4 GiB | $15.3/month | $5.0 | $7.0 |
| 4 vCPU / 8 GiB | $27.7 | $7.0 | $11.2 |

Per machine per month, list prices 2026-10-24 (InstaBox pricing page; fly.io/pricing.md; Cloudflare
Containers pricing), 20 GB disk, "asleep" = awake 4 h a day:

| Machine | Always on | Asleep when idle | Fixed cost |
|---|---|---|---|
| InstaBox 2 vCPU / 4 GiB (US) | $15.3 | $5.0 | Team $500/month (usage included); Pro $20 for ≤5 machines |
| Fly shared-cpu-2x 2 GB, Tokyo | $20.5 | ~$6.4 (suspend) | none |
| Fly shared-cpu-2x 4 GB, Tokyo | $36.2 | ~$9 (stop, cold boot) | none |
| Cloudflare Containers standard-1 (½ vCPU, 4 GiB) | ~$28 + CPU | ~$6 | Workers Paid $5 (already paid) |
| Cloudflare Containers custom 1 vCPU / 3 GiB + R2 backup | ~$20 + CPU | ~$6 (cold wake) | same |

Cloudflare bills memory while running at about $6.6/GiB·month (custom sizes need 3 GiB per vCPU) and starts
every container on a fresh disk: files survive only through R2 backups or beta container snapshots, and
every wake is a cold start (see the [provider survey](codync-cloud-providers.md)). Below
~70–100 users InstaBox's $500 floor makes Fly the cheapest; above it InstaBox's unit prices win (US only).
Fly reserved compute is 40% off for always-on machines.

Fly Machines (estimates from Fly's pricing page, to verify): shared-cpu-2x with 2 GB about $13.4/month
always on in Virginia, ×1.31 in Tokyo; volumes $0.15/GB·month; a suspended or stopped machine pays
only storage. MausBot Starter ($15) is this machine always on.

- InstaBox Team plan: $500/month including $500 of usage, 100 machines running at once. Below ~70–100
  sleeping-when-idle users the fixed fee dominates.

## Prerequisites outside the code

1. **Provider confirmation.** Confirm in writing that Codync may run one machine per user (Fly:
   matches their per-user blueprint; InstaBox: also ask for Team pricing, a raised concurrency limit and
   a `codync` template like their `orca` and `herdr` ones).
2. **Policies.** Terms of service for Codync Cloud (acceptable use, no mining), privacy policy update,
   data deletion.
3. **Abuse.** Codync answers for every machine on its account: CPU/egress checks per machine from InstaBox metrics, suspend on sustained abuse.

## Other backends

Full survey (isolation technology, persistence, run limits, sleep/wake, prices, terms, regions for ~25
services, including Vercel Sandbox, E2B and Cloudflare): [providers](codync-cloud-providers.md).

Researched 2026-10-24 from each provider's own docs and terms (not legal advice). Almost every
standard agreement forbids reselling access or running the service "for the benefit of a third party".
That wording targets handing the provider's platform to others; building your own product that your
customers use (they never get a provider account or API) is the normal case, and Fly's own docs
recommend exactly this layout. The gray area is that each customer gets a whole machine, so confirm
with the chosen provider; the table keeps each provider's literal wording.

| Provider | Terms for serving end users | Fit for a managed always-reachable Linux box |
|---|---|---|
| InstaBox | forbidden without permission | full VM, systemd, persistent disk, pause keeps memory, resume ~1 s, Team 100 running |
| Boat (boat.dev) | **allowed, no approval needed** (TOS-8), not as plain compute resale | full Ubuntu VM; disk free for 14 days while stopped; resume reboots in seconds; $0.018/h for 2 vCPU/4 GB; account-wide start limits; EU only |
| Blaxel | **allowed** (§6 End Users) | standby after ~15 s without connections, resume <25 ms, billing API; 7/30-day sandbox expiry on low tiers, no Asia region, memory $0.0414/GB·h |
| Fly.io Sprites / Machines | "solely for internal use… not for the benefit of a third party", yet Fly documents per-user dev environments: ask in writing | Sprites: sleep after ~30 s, wake on URL in 0.1–2 s, idle ≈ storage only, no systemd, no suspend or per-machine usage API. Machines: start/stop/suspend API, Tokyo/Singapore, reserved −40%, root disk wiped on restart (state on a volume) |
| E2B | forbidden unless authorized in writing | paused sandboxes kept forever and free, resume ~1 s, metrics + lifecycle webhooks; 24 h continuous-run cap (Pro $150/month) forces pause/resume cycles |
| exe.dev | resale explicitly forbidden; enterprise only | cheapest always-on ($15/month for 2 vCPU / 4 GB), systemd, Tokyo/Singapore |
| Google Cloud | allowed as your own "Customer Application", bare resale not (§1.1, §3.3) | Taiwan region (asia-east1); e2-medium $28.3/month, ~$12.7 on a 3-year commitment; stopped VMs bill disk/IP only, but stop is a reboot (no memory-keeping pause), so sleep/wake means a cold boot (time unmeasured) |
| AWS EC2 | end users allowed, bare resale not (§6.4) | Taipei region (opt-in); t4g.medium $28.4; stopped bills disk only; new accounts capped at 5 vCPU until raised |
| Azure | explicit managed-service clause | Taiwan North; must deallocate to stop billing; 20 vCPU per region by default |
| DigitalOcean | **resale to end users explicitly allowed** (§4.4), partner program | $24/month, Singapore only, stopped servers still bill, new accounts limited to 3 servers; best OAuth for BYO later |
| Hetzner | end users allowed with full liability | cheapest, but small sizes out of stock, Singapore expensive, stopped servers still bill |
| Daytona, Vercel, Cloudflare, Modal, Runloop, Namespace, Codespaces, Cube, Railway, Render, Scaleway, Linode, Vultr | ruled out: resale/commercial bans, ephemeral disks or session caps |

Two kinds of backend: agent-machine startups (InstaBox, Fly, Blaxel, E2B) pause with memory and wake in
about a second but need written permission or have short track records; big clouds (Google Cloud, AWS,
Azure) allow a managed product under their standard terms and have Taiwan regions, but sleeping means a
full stop and boot. Fallback order if InsForge says no: Fly.io (ask for a written OK), then Google Cloud
asia-east1 (no permission needed; measure cold-boot wake first), then Blaxel.

## Prior art: MausBot Cloud

OpenMausBot (github.com/milind-soni/OpenMausBot, Apache-2.0, ~5.5k stars, a Grok Bot–style bot app on
Electron) already sells this product: **MausBot Cloud**, Starter $15 / Pro $49 / Max $99 a month
(Starter: 2 shared CPUs, 2 GB, 10→20 GB disk, always on), no AI included, the user signs in with their
own Claude/ChatGPT. Read 2026-10-24 from its `docs/cloud-pro.md` and openmausbot.com/llms.txt. The
provisioning service ("the Admin", `openmaus-cloud`) is a separate private repo; the machine side is
public.

| Their choice | What it means for us |
|---|---|
| **Fly.io, one Fly app per customer** with one always-on machine, a volume at `/data` (`HOME=/data`), TLS at `<app>.fly.dev`, each app in its own private network | Fly Machines deserve a prototype next to InstaBox: mature company, Tokyo/Singapore, a production precedent for exactly this product (Fly's terms still say "internal use": ask anyway) |
| Sleeping plans: the Admin **suspends** the machine (memory kept, first request wakes it, measured 0.72 s) or stops it (~20 s to wake) | Same shape as our sleep/wake; Fly also wakes a machine on any inbound request, so a phone's direct `/channel` connection to `https://<app>.fly.dev` could wake it with no relay change. Fly documents suspend for machines up to 2 GB, likely why Starter is 2 GB; Fly's root disk is wiped on restart, so everything (agent sign-ins, repos) lives on the volume |
| **Sleep handshake**: the Admin reads a signed `power` endpoint (busy codes: turn, routine run, call, maintenance, recent use in the last 10 min, just woke, next run within 5 min; `nextDueAt`) and asks `sleep`; the machine answers 409 while busy. The Admin starts a sleeping machine a minute before `nextDueAt`; resume is noticed as a clock jump | Copy this for host change (3): the host owns the idle decision, the Worker only carries it out. Our channel for it is the relay socket the host already holds |
| One Docker image (`cloud-home`, published to GHCR per commit) with the agent CLIs baked in; the Admin rolls a new image out one machine at a time and reverts on a failed health check | Managed machines should get updates from us in a staged rollout, not each host's self-updater |
| Server on loopback, Caddy the only listener; proxied requests are never the loopback owner; secrets passed over a pipe, never in the environment agents inherit | Matches our finding on InstaBox ports; adopt "a request with `X-Forwarded-For` is never loopback" in the host |
| **Memory admission**: a new agent process waits until memory fits one more (~700 MB each, measured 361–623 MB per OpenCode thread), idle agents stop first, the newest busy one is stopped before the box freezes, and the chat says it was memory, not the provider | Required for us too: a 4 GiB box fits ~4 busy bots. Add to the host before launch |
| Pairing: the Admin signs (HMAC with a per-machine bootstrap secret) a request that opens one single-use pairing window; only the owner's devices may connect | Our equivalent is the Worker-driven claim + `Approval::Auto`; same trust trade-off, stated the same way |
| Health: the Admin polls a signed `health` read every 5 min: fixed codes and counts only (failed turns by cause, routines, engine sign-in state, memory, disk), never content | Worth copying for support |
| One-time **move** of the desktop's bots to the Cloud, local copy frozen ("one home"); optional "Let my bots use this Mac" (folders, apps, home internet for sites that block data centres) | Move fits our one-computer focus (add in Beta); lending the Mac is multi-computer work, later |
| Bundled extras through relays with scoped tokens (cloud desktops, voice, a Claude trial credit) | Not for v1; the pattern works for our voice later |

## Risks

- InstaBox launched 2026-10-14; pricing and free-tier limits changed within its first week; sign-up is
  invite-only. Keep the seam thin and a second provider evaluated.
- One provider account for all users: its suspension takes every cloud computer down.
- InstaBox is US-only for now: fine for chat from Asia, slower for heavy terminal use. Fly has Tokyo and Singapore.
- Fly resets the root disk on a cold start: anything a user installs outside `/data` disappears; point package managers' user prefixes at the volume and say so.
- Wake latency is the product: measure it end to end before launch.

## Phases

0. **Now, in parallel:** ask InsForge for written terms, Team pricing and a `codync` template, and
   Fly.io for a written OK; run the same pause/resume/reconnect test on a Fly Machine (one app per user,
   as MausBot does) and pick the backend on the numbers; ship the two host fixes, they help self-hosted
   users too.
1. **Prototype** (no store, internal accounts): Worker provisioning + claim + auto-approve, one test
   machine per developer, agent sign-in from the phone, Codync-driven sleep/wake with measured latency.
2. **Beta**: abuse checks; InsForge terms signed.
3. **Launch**: release with the client UI on every platform, App Review.
4. **Later**: more sizes.

## Delivery

The Worker gates it: with `CLOUD_SECRET_KEYS` unset the apps show no Codync Cloud section, so shipping the code
changes nothing; with it set, every signed-in account has Codync Cloud (no allowlist, the owner's call on 2026-11-09).

1. **Dark merge into `dev`**: the branch split into reviewable commits that pass the pre-commit checks, the
   `after-change` workflow with full e2e (it touches the host, every client and the Worker), `npm run deploy:dev`
   with migrations `0007`–`0009` on the dev D1 and dev's own `CLOUD_SECRET_KEYS`, then `npm run e2e:cloud` and the iPhone and
   Android apps against dev.
2. **Launch on `main`**: production's own `CLOUD_SECRET_KEYS` opens Codync Cloud to every signed-in account at once
   (without an allowlist there is no smaller beta), so first: terms, acceptable use and privacy policy, encryption at
   rest, memory admission, MFA required for accounts with a cloud computer, Codex device-code sign-in, the welcome
   screen's "Start a cloud computer", the user manual page. Then `deploy:main` right after the release merge
   (migrations up to `0009`) and that key; each account brings its own provider key.

The cloud picks a cloud computer's version on its own (the soft and hard updates), and its owner can also update
it to the latest release from any device, like any computer (owner, 2026-10-31): *Update* on its Codync row
(`installHostUpdate`, offered through `hello.canUpdate` and `latestVersion`). A device whose app is older than that
release's `minApp` then gets *Update this app*.

## Acceptance (end to end)

Start a cloud computer on a real iPhone → bots on the cloud computer within 2 minutes → Claude Code
sign-in from the phone → a bot turn → push notification. Let it sleep, message it, measure time to the
first reply. A routine fires while it sleeps. The same computer reaches Android and desktop.

## Follow-ups found while testing

1. Host restarts `codync-screen` every 30 s forever on a machine with an X display but no GStreamer
   libraries (`supervise_linux_helper`, `host/src/screen/helper.rs`): stop after a few immediate exits
   and say why once.
2. Remote screen can't work on InstaBox's Xvnc desktop (no portal); InstaBox has its own desktop stream.
3. Check each agent's sign-in on a headless machine (Codex: device-code login).
