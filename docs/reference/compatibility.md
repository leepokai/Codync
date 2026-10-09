# Client and host compatibility

Clients (iPhone, desktop app on macOS, Linux and Windows, terminal client) and the host update on
different schedules: the iPhone app waits for App Review, the desktop app and standalone hosts
update when their owner (or the opt-in automatic updater) installs a release. Any client can
meet an older or a newer host.

## Rule

Each side names the **oldest version of the other side** it still works with. Versions are
the ordinary release versions (`MARKETING_VERSION`, `host/Cargo.toml`, `apps/desktop/package.json`,
always equal). There is no separate protocol number.

| Floor | Lives in | Meaning |
|---|---|---|
| `minApp` | `host/src/compat.rs` `MIN_APP`, sent in `hello` | The oldest client this host serves correctly. |
| `minHost` | `apps/ios/Kit/.../Models/Compatibility.swift` `VersionMismatch.minHost`, `apps/desktop/src/shared/compat.ts` `MIN_HOST`, `host/src/compat.rs` `MIN_HOST` (terminal client) | The oldest host this client works with. |

On every (re)connect the client reads `hello` and decides, in this order:

1. its own version is below `minApp` → **Update this app**;
2. the host's `version` is below its `minHost` → **Update Codync on <computer>**;
3. otherwise it syncs normally.

Versions compare as `major.minor.patch` (a leading `v` and any `-pre`/`+build` suffix are
ignored). An unreadable version, or a `hello` without one, never blocks: an unknown is not a
reason to lock someone out. Hosts from before 2.4.0 send no `minApp`; only the client's
floor applies to them.

## What each client does

| | iPhone and desktop (`BotStore.mismatch`) | Terminal client |
|---|---|---|
| Notice | `UpdateNeededCard` above the computer's bots and in place of the composer; status reads *Needs update* | *needs update* in the status line, with the reason |
| Update this app | iPhone: App Store page. Desktop: electron-updater check (`apps/desktop/src/main/updates.ts`) | Text only |
| Update the host | Desktop: its own host is reinstalled (**Restart host**). iPhone: instructions for that computer | `^k` → Check for updates |
| Sync while mismatched | Stopped; `hello` is asked again every 30 s and on every reconnect | Continues; sending is blocked |

Drafts are kept while sending is blocked. A host update restarts the host, so the reconnect
reads the new `hello` and the notice goes away on its own.

The iPhone and desktop apps also treat **undecodable data from a newer host** as *Update this app*: an event
that stays undecodable after the one rewind, or a `hello` whose other fields can't be decoded
(its `version` and `minApp` are still read). From an older or equal host it's only logged.

## Changing what crosses the wire

The policy is to move people forward rather than carry old formats: a breaking change ships
in **one release** that raises the floor, and updates are held back only as long as someone
couldn't get the matching side yet.

- **Additive changes need no floor change**: new fields (clients decode leniently; new
  Swift fields are optional), new methods, new enum values (kept as strings).
- **Renaming, removing or changing the meaning** of anything an older client reads or sends:
  change it, set `MIN_APP` to this release (its iPhone part ships with it: the change
  touches `apps/ios/Kit/`, so `Submit iOS` includes it), add a row to the floor history, and re-record
  the wire shape snapshot. Hosts hold that release back until the App Store has the iPhone
  app (next section), so nobody is asked for an update that isn't there yet.
- **Raise `minHost`** when the client's core flows need something older hosts lack. Hosts
  update without review, so this only asks the person to update the computer. An optional
  feature that needs a newer host hides its control instead.
- Floors never exceed the release that ships them (unit tests in both crates check this).
- The encrypted channel's handshake version (`v` in its `hello`, rejected with
  `unsupportedVersion`, close code 4400) is separate: the host must keep accepting every `v`
  a supported client sends.

### Holding host updates for the App Store

A release that raises `minApp` would lock out paired iPhones until App Review passes. Every
updater therefore asks the App Store (`itunes.apple.com/lookup?id=6760984418`) before
installing such a release:

| Situation | Host update |
|---|---|
| The release doesn't raise `minApp` (almost always) | Installs; the App Store isn't asked |
| No iPhone was ever paired with this computer | Installs |
| The App Store has that iPhone version or newer | Installs |
| The App Store has an older version (in review) | Waits; retried on the next check |
| The App Store can't be asked or has no answer | Automatic update waits; a person can install anyway |

- **Standalone hosts** (`host/src/update`): each release publishes a signed
  `codync-host-<platform>.compat.json` (`{"version", "minApp"}`, ~50 bytes, same key as the
  manifest, written by `sign-host-release.py` from `MIN_APP`), so the updater decides before
  downloading anything; for a release without one it runs the verified download
  (`codync-host compat`) instead. The signed manifest itself is unchanged, as old hosts
  expect it. Waiting shows as phase `waitingForApp` with `requiredApp` and `appStoreVersion`
  in the update status; `codync-host update --skip-app-check` (API
  `installHostUpdate {"skipAppCheck": true}`) installs anyway. With automatic updates on, a waiting host asks the App Store hourly and updates as
  soon as the app is there.
- **Desktop app** (electron-updater): the release workflow adds `minApp` to `latest-mac.yml`,
  and `Updates.available` (`apps/desktop/src/main/updates.ts`) holds the release while the
  local host's `minApp` is lower, an iPhone is paired and the App Store is behind, before
  anything downloads. A check the person starts shows why. While waiting it checks hourly. The Sparkle appcast kept for the
  SwiftUI Mac app's installs carries `<codync:minApp>` (`packaging/updates/annotate-appcast.py`);
  it is deprecated and removed after 2026-11-06 ([updates guide](../guides/updates.md#deprecated-the-sparkle-migration-appcast-remove-after-2026-11-06)).
- Hosts released before 2.5.0 don't have this gate and install any release. Before the first
  real `minApp` raise, most computers should be on 2.5.0 or newer.

On the iPhone, the *Update Codync* button of an *Update this app* notice is greyed out with
"still in App Store review" while the App Store has an older version; an unanswered lookup
keeps it enabled (the App Store page shows the truth).

### Reminders

While everything still works, a client behind a newer release gets a reminder, dismissed per
version. A host behind a client gets none: computers update themselves, and the phone usually
trails them (App Store review), so nudging a computer from it would only be noise.

| Client | This app is behind |
|---|---|
| iPhone | Card at the top of the bot list (App Store version from the lookup), *Update* opens the App Store |
| Desktop | *Update to <version>* in the menu bar's Settings → Updates and on the Settings Updates page |
| Terminal client | — (it is the host binary) |

### Wire shape snapshot

`wire_shape_matches_the_snapshot` (`host/tests/e2e.rs`) runs a full turn against a real host
(fake agent: user message, narration, tool call with a diff, plan, permission, final reply),
then records every field path and JSON type of `hello` and the event stream's catch-up (bot
and entry events, entries per kind) in `host/tests/fixtures/wire-shape.json`, together with
the host's `minApp`.

| Change | Test |
|---|---|
| Nothing on the wire | Passes |
| New field | Fails until recorded: `CODYNC_UPDATE_WIRE_SHAPE=1 cargo test --test e2e wire_shape`, no decision needed |
| Field removed, renamed or retyped | Fails, and recording is refused, until `MIN_APP` differs from the recorded one; then record |

So a breaking change can't land by accident: it either keeps the old field or raises the
floor. `null` fields match any type. Not covered: subtrees that depend on the machine rather
than the code (`backends`, `screen`, `urls`, `usage`), fields the sample turn doesn't produce
(thread replies, group chats, routines and other method responses), and what the host
accepts in requests. Changes there still follow the rule by review.

## Floor history

Update this table in the release that changes a floor, with the reason.

| Release | Host `minApp` | Client `minHost` | Why |
|---|---|---|---|
| 2.12.0 | 2.3.0 | 2.12.0 | The clients require Engram memory management fields and methods. Older clients can still list and forget memories on the new host. |
| 2.4.0 | 2.3.0 | 2.3.0 | First release with the check. Since 2.3.0 the wire only gained `minApp` and lost the unused `protocol`; 2.3.0 is the oldest pairing verified to work, and a higher floor would lock out hosts that never auto-update. |

## Trying it on a simulator

Run a throwaway Debug host (it registers with the dev cloud) on its own data and port, never
on `~/.codync` or 19222:

1. Build a copy with a patched floor or version into a separate target, then restore the
   source: e.g. `MIN_APP = "9.0.0"` in `host/src/compat.rs` (app too old) or `"version": "2.2.0"`
   in `hello` in `host/src/api/host.rs` (host too old), built with
   `CARGO_TARGET_DIR=<scratch>/hosttarget cargo build`.
2. `CODYNC_HOME=<scratch>/home <scratch>/hosttarget/debug/codync-host serve --port 19333`.
3. Pair the simulator: `POST /api/pairing` with `<scratch>/home/token`, then launch with
   `SIMCTL_CHILD_CODYNC_PAIR_URL=<pairingUrl>`.
4. Swap hosts on the same `CODYNC_HOME` (patched ↔ normal) to see the notice appear in the
   list and in place of an open chat's composer, and go away after the reconnect.

The simulator has no App Store app, so *Update Codync* ends in a Safari error there; on a
device it opens the App Store.
