# Codync Cloud on Boat: trial log

Hands-on test of [Boat](https://boat.dev) as the first Codync Cloud backend ([plan](codync-cloud.md#boat-first)),
2026-10-24, on the owner's Boat trial (personal account, trial ends 2026-10-31: 25 h compute, 2 sandboxes,
5/25/75 starts per minute/hour/day, auto-stop ≤2 h). CLI `boat` 1.0.68. Host `codync-host` 2.24.0 from the
public install script. Times are wall clock from this Mac unless noted.

## Steps and results

### 1. Create

`boat new --type small --no-env --name codync-cloud-test --json` → `bx_2y7zp2pe`, **ready in 1.9 s**, auto-stop
after 1 h (trial default). Public IPv6 (`2001:41d0:…`, OVH), outbound IPv4 `5.135.138.52` (OVH, France); the API
reported `machineProvider: baremetal` while running.

### 2. What the machine is

| Check | Result |
|---|---|
| OS | Ubuntu 24.04.4, kernel 6.8, x86_64 |
| Size (`small`) | 2 vCPU, 3.9 GB RAM; root disk 69 GB with 20 GB used by the OS and tools (Boat's 12 GB is the quota for your own files) |
| User | `user` with passwordless sudo, in the `docker` group; systemd PID 1 (`degraded`: some unit failed), user manager running, **linger already on** |
| Preinstalled | Node 24 (nvm), Claude Code, Codex, git, gh, Docker, curl |
| Network | relay (`api.codync.dev`) reachable; no inbound port needed |
| Secrets inside a `--no-env` sandbox | only `ASCII_TOKEN`, which can publish ports of this sandbox and nothing else ([Boat security](https://docs.boat.dev/security.md)); no account API key, no git or AI logins |

### 3. Install the host

`install.sh --host-only` + `codync-host install` (systemd `--user`) in one `boat exec`: **7.8 s** end to end.
The host started at 11:27:18 and logged `relay ready` at 11:27:20 (**2.2 s** from start to relay ready).
An X socket exists (`/tmp/.X11-unix/X0`, Boat's desktop), so the host starts `codync-screen`, which fails without
GStreamer and restarts in a loop: same as on InstaBox ([follow-up 1](codync-cloud.md#follow-ups-found-while-testing)).

### 4. Data

Wrote `~/marker.txt` and a 1 GiB random file `~/big.bin` (sha256 `af88cc22ca2e482e…`) to check persistence and
whether resume time depends on data size.

### 5. Stop

`boat stop` returned in 1.1 s (operation `pending`); the sandbox reached `stopped` after **34 s** (the snapshot,
with ~1 GiB of new data, took ~30 s).

### 6. Resume right after the stop (15 s later)

`boat resume` returned `ready` in **2.1 s**. Same boot id (up since 11:20), same host PID, relay still connected,
nothing re-logged: a quick resume re-attached the still-warm machine, processes and memory included.

### 7. Resume after a 5-minute stop

Stop took 64 s this time. After 5 minutes stopped, `boat resume` returned `ready` in **2.9 s**. The machine had
again been kept warm: same boot (up since 11:20), same host PID. The host noticed the dead relay socket 1.6 s after
the resume (its monotonic clock counted the stopped time, unlike on InstaBox), reconnected, and logged
`relay ready` **4.0 s after the resume started**. `boat usage` showed 60 billed seconds at that point: stopped
time isn't billed (the counter may lag).

### 8. Resume after a 30-minute stop

This time the machine was gone: the resume landed on a different node (new boot id, booted 12:01:56 from Boat's
warm pool) and restored the disk. `boat resume` returned `ready` in **3.8 s**; systemd started the host 3.6 s
after the resume began, the host listened 1.4 s later, registered, and logged `relay ready` at 12:06:17.97:
**7.7 s from resume to relay ready on a cold machine**. Same computer identity (it lives on the disk), `marker.txt`
and the 1 GiB file intact (sha256 matches). The disk is hydrated lazily: the first full read of the 1 GiB file took
9.9 s (~100 MB/s); files nobody touches cost nothing at wake.

### 9. Updates

After another resume: `codync-host update --status` → `Host 2.24.0 · upToDate`, `Automatic updates: on` (the
script's standalone install updates itself). Boat's image supplied Claude Code 2.1.310 and Codex 0.164.0
(`/usr/local/bin`, from the base image, refreshed on each cold resume).

### 10. Cost so far

`boat usage`: 89 billed seconds ($0.00089) for ~3 minutes of running time across four starts (`small` bills at
half rate); stopped time cost nothing. Starts used: 2 of 75 for the day (the warm resumes didn't seem to count).

### 11. Phase 1 end to end (2026-10-24)

`npm run e2e:cloud` in `cloud/` (`test/e2e/cloud-computer.e2e.ts`): a local Worker with a scoped Boat key
(`sandbox.create/read/stop/resume/delete`, expires with the trial; it also needs `snapshot.read/write`, see §14) creates the computer through
`POST /v1/cloud-computers`; `boat forward --reverse` lets the sandbox reach the local Worker; the released host
installs, a Linux dev build of the host (built on a second Boat sandbox) is swapped in for the self-claim; then a
fake phone signs in, asks for access and connects over the relay; `DELETE` cleans up.

- First run refused: `trial_auto_stop_required` (trial sandboxes need an auto-stop of 2 h or less). The Worker
  now takes `CLOUD_AUTO_STOP_SECONDS` (the test sets 3600); production leaves it unset.
- The claim took 81 s on the second run: the host backed off (up to 60 s) while it waited for its own
  registration. Waiting for registration is a local check now, without backoff.
- Final run: sandbox `running` 2.8 s after the request, setup (released host installed and active) at 9.0 s,
  the claim **2.2 s after the host started**, then phone registration, access request, automatic approval and the
  relay connection in about 4 s. Delete removed the sandbox and took the computer out of the account. Copying the
  47 MB debug host in took ~77 s; that step exists only until a release has the self-claim.

### 12. Phase 2: sleep and wake (2026-10-24)

The same test, extended: the phone creates a bot and leaves; the idle host asks to sleep (`POST /v1/host/sleep`,
idle time shortened to 20 s with `CODYNC_SLEEP_IDLE_SECS`); the phone comes back and leaves a message in the
mailbox, which wakes the machine through the relay.

- Asked to sleep 16 s after the phone left; Boat reported it stopped 1.6 s later.
- The message woke it and was **delivered 7.2 s after it was sent** (a quick, warm resume); the bot had it.
- Found on the way: (1) a stopped machine's relay socket doesn't close, so the relay kept showing the computer
  online and refused mailbox messages (`hostOnline`) until its 90 s silence timeout. The relay now counts the
  host asleep from the sleep request on. (2) A warm resume brings back the same host process with its old socket,
  which the relay counts as asleep, and timer gaps are too short to notice reliably. The host now asks
  `GET /v1/host/sleep` every 3 s after its request and reconnects once the cloud has started the machine again.
- Not measured yet: a cold resume (stopped long enough to land on a new machine), the routine `wakeAt` alarm on
  Boat (covered by the Worker tests).

### 13. An app that needs a newer host (2026-10-24)

The test host for this run was a production release build of this branch at version 2.23.9 (the updater refuses
debug builds). After sleep and wake passed again (delivered 7.9 s after the message), the phone connected with
`Codync-App: ios/9.0.0; minHost=2.24.0`. The Worker told the host, the host installed the published v2.24.0
through its signed updater, and it was **back online on 2.24.0 4.6 s after the app connected**; the app then
talked to the updated host.

### 14. The iPhone app against a local cloud (2026-10-25)

`npm run cloud:stack` with the Codync Dev iPhone app on a simulator, signed in with a dev Clerk test account, and
a production build of this branch (2.23.9, `CODYNC_ANALYTICS_ENV=dev`) as the machine's host:

- Settings → Codync Cloud → create → "Preparing your cloud computer…" → claimed in about 25 s; it joined the
  account as "Codync Cloud · Boat · EU" and the phone got access by itself, with no code.
- Chat with a bot on it (a fake agent): permission card, Allow once, reply.
- The app went to the background: the machine slept within a minute. Reopened: the app's reconnect woke it
  (Boat `idle` 7 s after launch), a message sent while the header still said *Sleeping* waited in the outbox
  ("Waiting for the computer to come online") and went through once the host was back.
- After its older snapshots were erased, the machine still slept and resumed from the one its stop took, and the
  next message was answered 4 s after sending.
- Delete cloud computer: gone from Boat and from the account; the section offers *Create* again.
- Found on the way: (1) the test key lacked `snapshot.read/write`, so *Delete snapshots* failed with Boat's
  `api_key_action_forbidden` (the snapshots were then erased with the CLI's login to check the rest); the cloud's
  key needs those actions, for backups too. (2) A sleep of several minutes ends the `boat forward --reverse`
  tunnel, so the test scripts now open it again each time the machine comes up.

### 15. Updates, backups and a cold wake on the paid plan (2026-10-31)

`npm run e2e:cloud` with release builds of this branch (synced with `dev`) on Boat's $20 plan:

- **The owner's update** (`CODYNC_E2E_UPDATE_NOW=1`, host 2.32.9): the cloud computer offered 2.33.6 (GitHub's
  latest); with a bot working, `installHostUpdate` was held back until asked with `force`; back online on 2.33.6
  6.9 s later.
- **An app that needs a newer host** (`CODYNC_E2E_UPDATE_TO=2.33.5`): updated to exactly 2.33.5, back online
  14 s after the app connected.
- **The long run** (`CODYNC_E2E_COLD=1`, host 2.33.6): asleep, *Delete snapshots* is refused; the cron saved a
  backup of the stopped machine (`POST /saved-snapshots` 202); after 31 minutes asleep a message woke it on a
  fresh machine (new boot id) and the bot had it 15 s later; with the resume answered `409
  snapshot_data_expired` (a pass-through to Boat makes that one answer up), the cloud started a new sandbox from
  the backup and the message was delivered 54 s later, the bot's earlier history included; awake, *Delete
  snapshots* erased the history and the backup; the cloud computer kept running.
- Found on the way: (1) a wake asked within a minute of the last one was dropped, so a computer that went back
  to sleep inside that minute stayed asleep until something else woke it; the relay now tries again once the
  minute is up (in production the computer stays awake at least 10 minutes, but nothing should be lost). (2) A
  bot waiting for the owner's approval counts as working, so the computer never sleeps while one waits. (3) Two
  runs at once timed out (a first boot, a wake); each passed alone, so runs keep their lock.


### 16. The desktop app as a client of the cloud computer (2026-11-02)

`npm run cloud:stack` (host 2.33.6 on Boat, `CODYNC_SLEEP_IDLE_SECS=90`) with this worktree's desktop sandbox
pointed at it, driven through its DevTools port:

- Created from the desktop's account; the desktop asked for access by itself and the computer approved it; the
  computer then showed in the sidebar and the computer filter with its lead bot, chats over the relay.
- From the desktop: opened the lead bot's chat (history, agent sign-in card), created a bot *Cloud helper* on it,
  and set its standing instructions; read back from the computer over a second stream.
- Hidden window: the channel closed, the computer slept 82 s later (Boat `archived`); shown again: it woke and the
  desktop reconnected (about 15 s once the test tunnel was up).
- Found on the way: Electron's `node:crypto` (BoringSSL) has no ChaCha20-Poly1305, so the first build crashed the
  main process at the first frame (Node's OpenSSL passed the unit tests); frames now use `@noble/ciphers`. A view
  that refreshes every 3 s (a bot's routines) kept the channel open while the window was hidden, so the computer
  never slept: the main process now closes channels whenever nobody looks at the window, whatever views ask for.
  An access request now wakes a sleeping cloud computer (it has to answer it), for every app.
- Test harness, not product: after the second wake the stack's reverse tunnel for the local cloud port didn't come
  back (`keepForwarded`), so the host couldn't reach the local cloud until it was reopened by hand.

## Findings

| Path | Time |
|---|---|
| Create `small` from the stock image | 1.9 s to ready |
| Install the host (one-line install + `codync-host install`) | 7.8 s; host start to `relay ready` 2.2 s |
| Stop | 5–64 s to `stopped` (snapshotting what changed) |
| Resume within minutes (warm: same machine, memory kept) | 2–3 s to ready, **4.0 s to relay ready** |
| Resume after 30 min (cold: new machine, disk restored) | 3.8 s to ready, **7.7 s to relay ready** |
| `POST /v1/cloud-computers` to a claimed computer (host from a release) | ~11 s: running 2.8 s, setup 9.0 s, claim 2.2 s after host start |
| Claimed to the owner's new phone connected (automatic approval) | ~4 s |

- Boat keeps a stopped machine warm for a while (minutes, not half an hour in this test), so short sleeps wake with
  memory and processes intact; longer ones reboot on a fresh machine from the snapshot. Both paths reconnect on
  their own with the released host.
- The stock Boat image already has everything the host needs (systemd user manager with linger, Node, Claude
  Code, Codex); the public one-line install works unchanged.
- Self-sleep option: Boat API keys can be limited to one sandbox and to chosen actions (`sandbox.stop`,
  `sandbox.read`, …) ([scopes](https://docs.boat.dev/api-keys/scopes.md)). A key that can only stop its own
  sandbox, passed in as an env var, would let the host put itself to sleep without a round trip through the
  Worker; waking still needs the Worker (`resume`).
