# codync-network and cloud assistants: what's left

Status (2026-10-26): planned, nothing below is built. 2.28.0 shipped the network (`host/crates/network`, crate
`codync-network` 0.1.1 on crates.io) and the cloud assistant connector (`mcp.codync.dev`). Read first:
[design](../design/codync-network.md) (why, and the decisions already made), [feature](../features/codync-network.md)
(how it behaves today), [cloud assistants](../features/cloud-assistants.md), [relay §7.9](../reference/remote-relay.md).

Today every network and assistant setting is a command on the computer (`codync-host network …`,
`codync-host cloud --assistants on|off`). The apps only show the result: an exchange with a bot on another
computer reads "Messaged Reviewer on laptop". And there are no access rules: any agent on any member computer can
message any visible bot on any other.

The work is split into packages, in the order to build them. Each one ships on its own.

## Rules for whoever builds this

- **Host owns the logic** ([overview](../architecture/overview.md#host-owns-the-logic)). Rules, checks and state live
  in the host or the crate, and clients only show them and send intents. A client that needs data gets a new host
  method.
- **Every client in the same change** (CLAUDE.md, cross-platform UI): iPhone (`apps/ios/`), Android (`apps/android/`),
  desktop (`apps/desktop/`, one codebase for macOS, Linux and Windows) and the terminal UI (`host/src/tui/`). Name
  every client a package leaves out, and why, in the PR.
- **Membership is decided at a computer.** The `network*` host methods and `setAssistants` are loopback-only
  (`host/src/api/devices.rs`, `LOCAL_ONLY`), the same rule as pairing a device. The cloud never admits. Keep that:
  only the desktop app and the terminal (both on the computer) change membership. Phones get read-only views.
- **Compatibility**: new fields and methods are additive. An app that calls a method an older host lacks gets
  "unknown method", so it hides that section and doesn't show an error. No `minApp` change unless something is
  renamed or removed ([compatibility](../reference/compatibility.md)).
- **The crate**: a change to `host/crates/network` raises its 0.x version (minor for API additions) once per release.
  `network-crate.yml` checks that, then mirrors the crate to `leepokai/codync-network` and publishes it. Merge
  `dev → main` with a merge commit, never a squash: the mirror only fast-forwards
  ([crate releases](../guides/development.md#crate-releases)).
- **Release**: a minor bump of the app version (feature). Write the iPhone and Android WhatsNew sections when a phone
  app changes.
- **Docs** in the same change: the feature doc, the design doc's "Next" list, the user manual (`teamwork.mdx`
  "Bots on your other computers", `cloud-assistants.mdx`), and privacy if what the cloud or relay sees changes.
- **Process**: one worktree per package (`feat/network-ui`, …), the `after-change` workflow, minimal e2e on the Mac
  (the desktop app through `npm run dev`, the iPhone simulator, the Android emulator, the terminal in tmux). Two hosts
  on one Mac work for the network ([the earlier e2e](../features/codync-network.md#code-and-verification)).
- Never name another product a feature copies, in code, docs or commits.

## 1. Network screens in the apps

**Goal**: everything `codync-host network` does, done from the desktop app and the terminal; phones see the
network without changing it.

### Host

- `networkStatus` and `networkPeers` already return what the screens need: this computer's name and role, the
  connector, and the other computers with role, online, and the bots they offer (`host/src/api/network.rs`).
  Check each field against the UI below and add what's missing. Keep the JSON additive.
- **Live updates**: emit a `network` event (`hub.events`, like `cloud`) when membership, roles, names or presence
  change, so clients refresh without polling. The node learns of changes on hellos and merges
  (`host/crates/network/src/node/`). Add a change hook to `Node` (a `watch` channel, or a callback set by the host).
  That's a crate API addition.
- **Read-only for phones**: a new method `networkView` (status plus peers, no invite and no recovery). Leave it off
  `LOCAL_ONLY` so paired devices can call it over the encrypted channel. Don't open the existing methods: `networkInvite`
  must stay local.
- The recovery code exists only in `networkCreate`'s answer. The UI must show it once, make the user confirm they
  saved it, and never fetch it again: the host doesn't keep it.

### Desktop (`apps/desktop/src/renderer/views/settings/ComputersView.tsx`, store in `store/`)

A "Network" section on the computer's settings page, styled like the cloud and approval rows already there:

- **Not in a network**: two actions, Create (a computer name defaulting to the host name, plus the connector:
  public, private or a relay URL) and Join (paste an invite, name).
- **Create** shows the recovery code once in a dialog: copy button, a "I saved it" confirmation, and the warning
  that it moves ownership if this computer is lost.
- **Invite**: a one-time code with a QR and its one-hour expiry, copyable.
- **Computers**: one row per computer with name, role badge, online dot and how many bots it offers. Actions by
  role: the owner can promote (admin) and remove; an admin can remove members. Remove confirms in a dialog, with the
  cascade option explained ("also removes the computers it invited; for a stolen computer").
- **Recover**: enter a recovery code to become the owner.
- Calls go through `window.codync`, using the host's loopback API like `setApproval` does
  (`store/cloud.ts`, `ComputersView.tsx`). There are no static inline styles ([desktop styles](../guides/desktop-styles.md)).

### Terminal (`host/src/tui/`, overlays in `view/`, keys in `app/overlay_keys.rs`)

The same as the desktop, as an overlay reached from the computer view, keyboard-driven, with confirmations as in
`view/computer_access.rs`. The terminal runs on the computer, so it gets the full set.

### iPhone and Android (read-only)

- In each computer's settings (iPhone `apps/ios/Views/SettingsView.swift` and the computer rows; Android
  `SettingsView.kt`): a "Network" row. It opens a list of the computers in the network, with name, role, online
  and offered bots, plus a note that changes are made on a computer. It's hidden when the host is too old
  (`networkView` unknown) or the computer isn't in a network.
- iOS menus and sheets stay native (`.codyncSheet`, `DropdownMenu`). Android uses the `design` module only.

### The bot list (decide with the user before building)

Recommended default: **don't** mix other computers' bots into the bot list. A phone chats with a computer's own
bots, and a remote bot is reached by asking a local one. Show them in the network screen under their computer
instead. If the user wants them in the list, that is a separate change.

### Cloud assistants switch

Next to "Reach from anywhere" in the desktop and terminal: "Cloud assistants" (`setAssistants`). It carries the
same warning as the manual: the relay can read these calls. Phones show it read-only (it's in `cloudStatus` as
`assistants`; phones need it through `networkView`, or a field next to it). The manual's Cloud assistants page then
describes the switch, not only the command.

### Done when

The desktop app and the terminal can create, invite, join, list, promote, remove and recover on two real hosts.
Phones list the computers. The live event updates every client. Analytics: `setting_changed` for the assistants
switch is already there, and `network_joined` exists; add events for the screens the way
`docs/features/analytics.md` describes.

## 2. Access rules (ACL)

**Goal**: choose which bots each computer offers, and which agents may message them, before a network is ever
shared with other people.

### Model (recommended)

Enforced by the **receiving** computer, like the message budgets: no rule depends on the sender's goodwill, and
nothing new crosses the wire beyond a refusal reason.

1. **Offered to the network**, per bot: a switch, on by default so nothing changes for current users. A bot that's
   off isn't published to the network (`network::publish` filters it), so other computers don't list it, and
   letters to it are refused like letters to an unknown agent. Store it on the bot (`BotConfig`, additive field),
   editable wherever bot settings are: it narrows or widens exposure within the user's own network.
2. **Inbound rules**, per computer: an ordered list of `{from_computer, from_agent, to_bot, allow|deny}` with `*`
   wildcards. The first match wins, and with no rules everything is allowed (today's behavior). Put the check in the
   crate (`Node::set_rules`, checked in `node/inbound.rs` `take()` before handing over), so the standalone daemon
   gets it too (a `rules.json` beside `agents.json`, plus CLI commands). The host stores its rules in kv and pushes
   them to the node. Refuse with a clear reason, for example "laptop doesn't take messages from server's agents".
   The sender's `message_bot` returns it as the tool error. Replies (`reply_to` an outstanding request) always pass,
   or a rule added mid-request would strand it.
3. **Cloud assistant scope**: per bot, "Reachable by cloud assistants", on by default, checked in
   `chat/team/assistant.rs` for `list_bots` and `message_bot`.

The master switches stay loopback-only: being in a network and assistants on or off. The per-bot switches and the
rules are bot or computer settings. Editing rules is local-only (desktop and terminal), like membership. Per-bot
switches may be changed from any paired device.

### Clients

The per-bot switches go in bot settings on every client (iPhone `BotDetailsView`/`BotEditorView`, Android
`BotDetailsView.kt`/`BotSettingsForm.kt`, desktop bot details, terminal bot settings). The rules editor goes in the
desktop and terminal network screen; phones show the rules read-only.

### Tests

Crate: rules match, first match wins, wildcards, replies pass, the daemon's rules file. Host: an unoffered bot isn't
published and its letters are refused, and the assistant scope applies. e2e: two hosts, a deny rule, and a refused
`message_bot` that shows the reason in the sender's chat.

## 3. Codync's own relays as the default connector

Users never run a relay. Codync runs `iroh-relay` on two small VPSs (North America and Europe, ACME TLS), and the
default connector points at them instead of n0's public relays, which are rate-limited and not for production. Keep
`public` (n0) for development, and keep the URL and `private` connectors. Operations: hosting, DNS
(`relay-na.codync.dev`, …), monitoring, and keeping `iroh-relay` in step with the crate's iroh version. Code: the
default connector, the privacy policy's third-party list, and the design doc's relay table. Ask the user before
renting servers.

## 4. A computer's address after it moves

A computer signs where it is into the roster only when the host starts (`refresh_my_addr`). With the `private`
connector and no Tailscale, a laptop that changes networks can't be found until its host restarts. Watch the
endpoint's addresses (iroh exposes changes), re-sign `Locate` when they change, and hello everyone. The roster keeps
only each computer's newest location, so this doesn't grow the log. Test: change addresses in a two-node test.

## 5. Cloud assistants beyond the connected computer

The connector reaches only the bots of the computer it talks to. To reach that computer's network: publish the
assistant as an agent of that computer (or let the host send letters for it under a reserved name), route
`message_bot` to `name@computer` through `chat::team::remote`, and keep replies in the assistant's reply store. Mind
the ACL from package 2: the assistant is a sender like any other.

## 6. A local MCP server for agents that aren't Codync bots

Claude Code or Codex sessions on the same computer use the standalone daemon's CLI today. Give them an MCP server:
`codync-host mcp network` over stdio, or the daemon's own. It offers `list_bots`, `message_bot` and `read_replies`,
backed by the host, with the session registered as an agent. It's a local alternative to the cloud connector and
needs no cloud.

## 7. Lifting the roster's per-computer cap

Each computer may have at most 256 roster entries, the oldest kept. An owner or admin that has made 256 admissions,
promotions and removals can make no more (documented in the feature doc's "Not yet"). Fix: hellos that carry the
changes since the last exchange (a version vector or a hash per signer) instead of the whole log. Then compaction
can drop entries that no longer count (an admission whose computer was removed, for example) without the 1 MiB
frame as the reason. This is a wire change: keep old peers working by falling back to the whole log.

## 8. Account networks and several networks per computer

- **Account networks**: the account vouches for its computers, so a computer signed into the account is admitted
  without an invite. It's still signed by a member computer, never by the cloud.
- **Several networks per computer**: addresses get a network suffix (`reviewer@desk.home`), bare names resolve
  within the default network, and names that collide across networks stay apart. The crate takes several rosters,
  and the host stores one per network.

## 9. Windows

The standalone daemon's control socket and the Claude Code inbox are Unix sockets today. On Windows, add a named
pipe for the control door, and Claude Code's named pipe with its token (`reach.rs`, `bin/codync-network/control.rs`).
Then run the crate's tests on Windows in its mirror CI (`host/crates/network/.github/workflows/ci.yml` skips them).

## 10. A connector users host themselves (decide first)

Today the only self-hosted path is running the whole `cloud/` Worker with one's own Clerk and pointing the host at
it with `CODYNC_CLOUD_URL`. A lighter path, the host serving MCP itself behind the user's own HTTPS, needs its own
OAuth server, because ChatGPT and Claude require OAuth for remote connectors. The user is still deciding whether to
offer this. Until then, document the full-cloud path in `cloud/README.md` if anyone asks.
