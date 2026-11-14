# Codync Cloud: versions and updates

Proposed 2026-10-24, part of [Codync Cloud](codync-cloud.md). How a cloud computer's host stays current, what
happens when the user's phone or desktop app is older or newer than it, and what Codync does in an emergency.
Self-hosted computers keep today's rules ([compatibility](../reference/compatibility.md)).

## Decision

- **A cloud host always runs the newest release with GitHub's *latest* mark.** It never waits for the user's
  oldest app.
- **An app too old for that host is made to update**, with the blocking *Update this app* screen that exists today
  (the host's `minApp` in `hello`). Only releases that raise `minApp` do this, which happens on renames and
  removals (three times so far: 2.18.0, 2.20.1, 2.21.0). Any other release only shows the existing
  "newer version" reminder.
- **The host updates itself** with the signed updater it already has. The Worker never picks versions in normal
  operation; it only steps in for emergencies (*hard update* below).
- So the cloud doesn't need to know which app versions a user runs: each app already checks itself against the
  host's `hello`.

## Why *latest* never strands the user's apps

Two existing rules make *latest* safe for every app a user can install:

- **`minApp`**: the release gate gives a release the *latest* mark only when the App Store's iPhone app meets its
  `MIN_APP`. So an iPhone told to update can always do it right away; the App Store already has that version.
- **`minHost`**: a floor never exceeds the release that ships it, so an app's `minHost` is at most its own version.
  The desktop app updates from *latest*, and the App Store's iPhone version V is itself a release the gate can mark,
  so *latest* ≥ V. Every app's `minHost` is at most *latest*: a host on *latest* satisfies every app, phone or
  desktop.

The only gap is timing: right after *latest* moves, an app can update before the cloud host does (see the next
section).

## What the user sees

| Situation | Phone | Desktop app | Cloud host |
|---|---|---|---|
| New release, apps current | nothing | updates itself | updates the next time it goes to sleep |
| Release raises `minApp`, phone older | blocking *Update this app*, one tap to the App Store | — | updates as usual |
| An app updated first and needs a newer host (its `minHost` rose past the host) | *Your cloud computer is updating*, with *Update now* | same | the app asks it to update to at least its `minHost`; it updates as soon as no bot is working (or at once on *Update now*), waiting for *latest* to get there if needed |
| Phone unused for months comes back | *Update this app* if the host's `minApp` moved past it, otherwise the reminder | — | — |
| Emergency fix (hard update) | *Update this app* only if the app itself is affected | same | updates within minutes, even mid-task |

## Cloud soft update (雲端弱更新, the default)

The existing updater (`host/src/update/`: signed manifest, health check, rollback within 30 s) with a schedule for
cloud computers:

- **Check** about a minute after each wake and then hourly while awake, timed with the wall clock. tokio's monotonic
  clock doesn't count a paused VM (InstaBox pauses with memory; Boat reboots, so a fresh process does the
  wake-time check anyway), and today's daily timer would almost never fire on a machine awake 1–2 hours a day.
- **Install** right before the host asks to sleep, never in the middle of a conversation: the host goes idle,
  installs, waits until the new host is healthy, then asks to be stopped. A wake then always lands on the new
  version. While sleep isn't built yet (build step 2), it installs once no bot has worked for 10 minutes.
- **When an app needs a newer host**: the phone or desktop app updates on its own schedule, and a release can raise
  its `minHost` past the cloud host (several devices: the highest `minHost` counts; each device asks for its own).
  The app then calls `installHostUpdate { atLeast: <its minHost> }` once per connection (owner's devices only; on a
  computer at home it stays local-only). The host installs *latest* once no bot is working, or at once with
  `force` (*Update now*). If *latest* is still below `atLeast` (the App Store, TestFlight or Google Play got the app
  before the gate marked its release, a window of up to ~5 minutes for the App Store), the host keeps the target
  and checks every 2 minutes, for up to a day, until *latest* reaches it. A newer target replaces an older one.
  Waking a sleeping cloud computer for this costs nothing extra: the app's connection wakes it, it updates right
  away (no bot is working yet), and the app reconnects to the new host, about half a minute in all.
- **Automatic updates can't be turned off** on a cloud computer: nobody sits at it, and Codync keeps it current.
- **New machines** install *latest* at creation (the setup script), as today.

## Cloud hard update (雲端強更新, emergencies only)

For a security hole or a critical bug, an operator sets `HOST_FLOOR` (a version no higher than *latest*) and
`HOST_FLOOR_REASON` on the Worker:

- **Running cloud machines**: the Worker's cron finds cloud computers whose `computers.version` is below the floor
  and runs `codync-host update --force` on each through the provider (`codync-cloud` `exec`). This works with
  any host version, including ones from before this design. Working bots are interrupted; their sessions resume.
- **Sleeping cloud machines** update on their next wake, before the relay lets any device in: the relay refuses
  device sockets to a host below the floor.
- **Self-hosted computers** (optional, same floor): the relay refuses their registration with
  `426 upgradeRequired`. Hosts from this design start an update check as soon as they're refused; older ones get it
  from their daily check.
- **Apps**: an app fix ships the normal way. Raise `MIN_APP` in the fixed release and ask App Review to expedite
  it. The gate holds that release until the App Store has the fixed app, then cloud hosts take it and old apps get
  *Update this app*. No Worker-side app check is needed.
- Tell affected users by push and email.

## Pulling back a bad release

- If a release doesn't start, the updater's health check rolls it back on each machine by itself.
- If it starts but misbehaves, move *latest* back with the release gate's `allow_back` run, which stops every
  updater, then ship a fixed patch. If the problem is urgent, set `HOST_FLOOR` to the fix.
- A staged rollout (internal accounts first) can be added once the fleet is large: the Worker would answer cloud
  hosts' "what is latest" instead of GitHub. Add it when needed.

## Changes

| Where | Change |
|---|---|
| Host, `remote/claim.rs` | Applying the claim ticket also stores `managed` (store key, like `APPROVAL`): the ticket file is deleted after the claim, and the host must keep knowing it's a cloud computer |
| Host, `update/mod.rs` | Managed hosts: wall-clock check on wake and hourly, install before sleep (until then, after 10 idle minutes), automatic updates always on |
| Host, `api/devices.rs`, `api/host.rs` | `hostUpdateStatus`, `checkHostUpdate`, `installHostUpdate` allowed for account devices on a managed host; `installHostUpdate` takes `atLeast` and keeps it as a pending target (checked every 2 minutes, up to a day) while *latest* is below it |
| Host, `remote/` | On `426 upgradeRequired` from the relay, start an update check |
| Worker | `HOST_FLOOR` / `HOST_FLOOR_REASON` vars; cron `exec` for cloud machines below it; relay refuses devices to cloud hosts below it and (optionally) self-hosted registration with 426 |
| `codync-cloud` | `exec(id, argv)` (Boat runs commands over its API; the scoped key needs the exec action) |
| iPhone, Android, desktop | On a cloud computer the *Update Codync on <computer>* state calls `installHostUpdate { atLeast: <its minHost> }` itself and reads *Your cloud computer is updating*, with *Update now* (`force`) while bots work (the cloud flag comes with the provider name in `/v1/computers`). The terminal UI runs on the host and is unaffected |
| Android | *Update this app* opens Google Play's in-app update (immediate) instead of text, before Android goes public |
| Release gate | Also require Google Play's version ≥ `MIN_APP` before Android goes public (today it reads only the App Store) |

## Dropped from the earlier plan

- The `Codync-App: <platform>/<version>` header and `devices.app_version` / `app_seen_at`.
- A per-release `compat.json` and picking a target per account ("newest release every device of the last 30 days
  can use"), with a version argument for `codync-host update`.
- Worker-run soft updates (`exec` before `stop`) and a Worker `appFloor`.
- Holding the host back with a non-blocking "Update Codync to get your cloud computer's newest version".

## Rejected alternatives

- **Block every phone on every release** (phone version must equal *latest*): each release would need its own
  iPhone build through App Review (1–2 days), so phones would be blocked from release to approval, or the gate would
  have to hold every host release behind App Review. App Store auto-updates also lag, so users would hit a blocking screen
  several times a week. 2.18.0 showed what a surprise block costs: 33 new users locked out in a day.
- **Hold the host for the user's oldest app**: it needs app versions in the cloud, per-release compatibility data
  and per-account targets, and the newest app's `minHost` can still conflict with the oldest app's `minApp`. When
  that happens the old phone has to update anyway, so the end result is the same as always following *latest*,
  just with more moving parts.

## Known edges

- **TestFlight and Android ahead of *latest***: Xcode Cloud ships every tag to TestFlight, and Google Play doesn't
  wait for the gate. A build that raised `minHost` sees *Your cloud computer is updating* until the gate marks that
  release; the host's pending target picks it up within 2 minutes after that. A target that never reaches *latest*
  within a day (a withdrawn release) is dropped, and the notice stays like on any computer that's too old.
- **Android before the gate reads Google Play**: an Android phone below `MIN_APP` might not have an update on Play
  yet. Android is internal-only today; fix the gate before it goes public.
