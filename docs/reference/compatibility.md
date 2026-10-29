# Client and host compatibility

Clients (iPhone, desktop app on macOS, Linux and Windows, terminal client) and the host update on
different schedules: the iPhone app waits for App Review, the desktop app and standalone hosts
update when their owner (or the automatic updater, on by default for hosts that update themselves) installs a release. Any client can
meet an older or a newer host.

## Rule

Each side names the **oldest version of the other side** it still works with. Versions are
the ordinary release versions (`MARKETING_VERSION`, `host/Cargo.toml`, `apps/desktop/package.json`,
always equal). There is no separate protocol number.

| Floor | Lives in | Meaning |
|---|---|---|
| `minApp` | `host/src/compat.rs` `MIN_APP`, sent in `hello` | The oldest client this host serves correctly. |
| `minHost` | `apps/ios/Kit/.../Models/Compatibility.swift` `VersionMismatch.minHost`, `apps/desktop/src/shared/compat.ts` `MIN_HOST`, `apps/android/core/.../Compatibility.kt` `VersionMismatch.MIN_HOST`, `host/src/compat.rs` `MIN_HOST` (terminal client) | The oldest host this client works with. |

On every (re)connect the client reads `hello` and decides, in this order:

1. its own version is below `minApp` → **Update this app**;
2. the host's `version` is below its `minHost` → **Update Codync on <computer>**;
3. otherwise it syncs normally.

Versions compare as `major.minor.patch` (a leading `v` and any `-pre`/`+build` suffix are
ignored). An unreadable version, or a `hello` without one, never blocks: an unknown is not a
reason to lock someone out.

## What each client does

| | iPhone and desktop (`BotStore.mismatch`); Android (`AppState.mismatch`, `AppStoreConnection.kt`) | Terminal client |
|---|---|---|
| Notice | `UpdateNeededCard` above the computer's bots and in place of the composer; status reads *Needs update* | *needs update* in the status line, with the reason |
| Update this app | iPhone: App Store page. Desktop: electron-updater check (`apps/desktop/src/main/updates.ts`). Android: text only | Text only |
| Update the host | Desktop on macOS and Windows, its own computer: the bundled host is reinstalled (**Update now**). Any computer whose `hello` has `canUpdate` (the Linux desktop's own too): **Update now** calls `installHostUpdate` ([updates](../guides/updates.md#standalone-hosts-on-linux-and-macos)), and the notice reads *Updating Codync on …* until the host comes back newer, or for 10 minutes; then it offers the update again with why. Otherwise: instructions for that computer. The phones also offer it, and any newer release (`hello.latestVersion`), on the Codync row of the computer's card in Computer management; the desktop on SSH computers' cards in Settings > Computers | `^k` → Check for updates |
| Sync while mismatched | Stopped; `hello` is asked again every 30 s (5 s while an update asked from here runs) and on every reconnect | Continues; sending is blocked |

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
  change it, set `MIN_APP` to this release (the phone apps ship with it: `Submit iOS` and
  `Submit Android` ship any release whose `MIN_APP` is above their store's version, even
  without app changes), add a row to the floor history, and re-record
  the wire shape snapshot. That release isn't offered (downloads, updaters, Homebrew) until
  the App Store has the iPhone app (next section), so nobody is asked for an update that
  isn't there yet.
- **Raise `minHost`** when the client's core flows need something older hosts lack. Hosts
  update without review, so this only asks the person to update the computer. An optional
  feature that needs a newer host hides its control instead.
- Floors never exceed the release that ships them (unit tests in both crates check this).
- The encrypted channel's handshake version (`v` in its `hello`, rejected with
  `unsupportedVersion`, close code 4400) is separate: the host must keep accepting every `v`
  a supported client sends.

### Holding releases for the App Store

A release that raises `minApp` would lock out paired iPhones until App Review passes, and a
computer set up from it would lock out the iPhone it pairs with. So what anyone can get
follows the App Store:

- **Release gate** (`tools/release-gate.py`, run by `.github/workflows/release-gate.yml` after
  each release, and every 5 minutes while a newer release waits or the Homebrew tap trails the
  latest one, dispatched by the cloud Worker's cron: `cloud/src/release-gate.ts`, token
  `RELEASE_GATE_TOKEN`): releases are published without GitHub's *latest* mark. The gate marks
  the newest fully uploaded release whose `MIN_APP` the App Store's iPhone version meets
  (`itunes.apple.com/lookup?id=6760984418`, with a query string of its own each time: Apple's CDN
  keeps the bare URL for up to a day after a release) and points the Homebrew tap (cask and formula) at
  it. The website's download links (`releases/latest/download/…`), `packaging/install.sh`,
  electron-updater and `codync-host update` all read the latest release, so none of them sees
  a release that waits. Every release after a raise waits with it, desktop-only ones too, until
  App Review passes; within 5 minutes the gate then offers the newest. The new iPhone app asks for
  its own host at once, so the wait has to be short. The gate itself reads the App Store from
  GitHub's runners (Apple answers Workers with 403). The cron replaced the workflow's hourly GitHub
  schedule, which GitHub ran late or not at all: on 2026-10-20 iPhone 2.20.1 reached the App Store
  at 09:49 UTC while the next scheduled run came at 11:43, so for two hours every phone that
  updated said to update the computer to a host no updater could see. If the Worker can't dispatch
  (its cron logs show `GitHub graphql: 401` or `release gate dispatch: 403`), releases wait for the
  next release's own run or a run by hand. The mark only moves forward on its own (a lookup can
  briefly answer with an older version); to move it back, run the *Release gate* workflow by hand
  with `allow_back` on. `DRY_RUN=1 python3 tools/release-gate.py` prints the decision;
  `python3 tools/release-gate-test.py` checks it. Before the gate, 2.18.0 (the first real raise)
  went straight to the website: in its first day 33 new computers were paired by the App Store's
  iPhone 2.17.0 and received no message from it.
- **Updaters** only compare versions with the latest release; they don't ask the App Store
  themselves (they did from 2.5.0 to 2.19.2, before the gate). The hidden `codync-host compat`
  command stays: the desktop app runs it to read `environment` before installing the host, and
  those older hosts run it on a downloaded update to read its `minApp`.

On the iPhone, the *Update Codync* button of an *Update this app* notice is greyed out with
"still in App Store review" while the App Store has an older version; an unanswered lookup
keeps it enabled (the App Store page shows the truth). Its lookup skips the CDN the same way.

### Reminders

While everything still works, a client behind a newer release gets a reminder, dismissed per
version. A host behind a client gets none: computers update themselves, and the phone usually
trails them (App Store review), so nudging a computer from it would only be noise.

| Client | This app is behind |
|---|---|
| iPhone | Card at the top of the bot list (App Store version from the lookup), *Update* opens the App Store |
| Desktop | *Update to <version>*, then *Restart to update*, on the update card, in the menu bar's Settings → Updates and on the Settings Updates page |
| Terminal client | — (it is the host binary) |

### Wire shape snapshot

`wire_shape_matches_the_snapshot` (`host/tests/e2e/wire_shape.rs`) runs a full turn against a real host
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
than the code (`backends`, `screen`, `urls`, `usage`, including asynchronous usage events), fields the sample turn doesn't produce
(replies, group chats, routines and other method responses), and what the host
accepts in requests. Changes there still follow the rule by review.

## Floor history

Update this table in the release that changes a floor, with the reason.

| Release | Host `minApp` | Client `minHost` | Why |
|---|---|---|---|
| 2.26.0 | 2.21.0 | 2.26.0 (iPhone, Android, desktop) | What a call's voice hears comes from the host as `call` events (replies, approvals, failures, progress); the apps no longer derive it from the chat, so on an older host a call would never hear the bot. |
| 2.25.0 | 2.21.0 | 2.25.0 (iPhone, Android, desktop) | Voice calls run on the host: the apps start a call, run the voice's tools and end it through `startCall`, `callTool`, `callLines` and `endCall`, which older hosts lack (a Linux desktop or an SSH computer can run one). `logCall` stays for older apps. |
| 2.21.0 | 2.21.0 | 2.20.1 | Memory is files the bot keeps, and the apps only read it: the edit methods (`saveMemory`, `pinMemory`, `archiveMemory`, `forgetMemory`, `clearMemory`, `undoDream`, `answerMemory`, `undoMemory`, `saveMemoryFile`, `deleteMemoryFile`, `restoreMemoryFile`) are gone, so older apps' memory buttons would fail. `memory` keeps its shape, so new apps still read an older host's memory. |
| 2.20.1 | 2.20.1 | 2.20.1 | Reply threads gave way to quote replies: the `thread` method, `send.threadId`, `markRead.threadId`, entry `threadId`, `data.thread` and bot `workingThread` are gone, and old thread replies sit in the main chat. Older apps would open empty threads. The clients no longer leave thread replies out of the chat or read threads, so an older host would show them inline, routine results twice and thread unread counts that never clear. |
| 2.18.0 | 2.18.0 | 2.12.0 | Memory moved from Engram into the host: the memory methods take and return the new two layers, and the Engram ones (`memoryDetail`, `reviewMemory`, `exportMemory`, `importMemory`) are gone. Clients still talk to older hosts; only their memory screen shows an error there. |
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

A Debug host is a development build, so its `hello.canUpdate` is false and the notice gives
instructions. To see **Update now** too, also patch `installable()` in `host/src/update/mod.rs`
to return `true` and `request_install` to return `Ok(())` (or an `Err` with a message, for the
failure note); restarting the scratch host unpatched then plays the update finishing.

The simulator has no App Store app, so *Update Codync* ends in a Safari error there; on a
device it opens the App Store.
