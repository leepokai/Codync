# codync-network

The user's computers join private networks, and a bot on one messages a bot on another the way bots on one
computer already do ([bot collaboration](bot-collaboration.md)). A computer can be in several networks (`home`,
`work`), each kept apart ([several networks](#several-networks)). The apps mark it **Beta**. The network is the `codync-network` crate
(`host/crates/network`, its own [README](../../host/crates/network/README.md)): peer to peer over QUIC
([iroh](https://iroh.computer)), with no Codync server involved. It is off until the user creates or joins a network.
Why it is built this way, the options weighed and what comes next: [design](../design/codync-network.md).

## Set it up

On each computer, in the desktop app (Settings → Network, a section per computer it manages, this one first,
`NetworkView.tsx` / `NetworkSection.tsx`), the terminal UI (`N`, `host/src/tui/manage/network.rs`) or the CLI. These are the only
clients that change membership: the `network*` API methods are loopback-only, like pairing a device.

```sh
codync-host network create [name] [--network home] [--connector public|private|https://relay]  # this computer owns it
codync-host network invite [--network home]                                   # prints a one-time invite (1 h)
codync-host network join '<invite>' [name] [--network …] [--connector …]      # on the other computer
codync-host network                                                           # every network and who is around
codync-host network peers | promote <computer> | remove <computer> [--cascade] | recover <code> | leave
```

`--network` names the network (by name or id) and may be left out while the computer is in only one; for `create`
it is the new network's name (default `home`), for `join` it replaces the name the invite carries.

The apps show the recovery code once, in a sheet that only "I saved it" closes; the host keeps it nowhere and the
apps never fetch it again. An invite shows when it stops working (`networkInvite` → `expiresAt`). Each computer row
offers only what this computer may do to it: the host says so per row (`canRemove`, `canPromote`, from
`Role::removes` and `Role::promotes` in the crate) and for this computer (`canInvite` from `Role::invites`,
`canRecover`), so the apps never work out the role rules themselves.

The desktop app adds the computers it manages over SSH without a pasted invite: under each network where this
computer has `canInvite`, it lists every SSH computer whose own view has no network with the same `key`, and
**Add to the network** makes an invite here (`networkInvite`) and joins with it there (`networkJoin` through the
tunnel, the network's connector, the default name). Only the computer the app runs on adds; an SSH computer's
section never offers to add this one (the master → slave direction of Computers). A network both are in shows
once, in this computer's card, with one rules list per computer in it ("Who may message bots on …", each saved
on its own host); an SSH computer's own section lists only the networks this one isn't in, without Create or Join.
Once added, the bots on both computers may message each other (no rules = everyone in the network): adding a
computer is the permission.

The iPhone and Android apps read the network from the event stream's hello and through `networkView` (allowed for
paired devices over the encrypted channel): the computers with their roles, presence and offered bots, and whether
cloud assistants are on (`assistants`) and can reach the computer at all (`assistantsAvailable`: it's in a Codync
account). Never an invite or the recovery code. The **Network** row on the computer's card in Manage computers (and its long-press menu) opens it; it's
hidden when the host is older (no `network` in the hello) or the computer is neither in a network nor has cloud
assistants on that can reach it.

**Live**: the node ticks a change hook (`Node::changes`, a `watch` channel) when members, roles, names, presence or
offered agents change; the host sends a `network` event (`{"type":"network","network": <networkView>}`) to every
client then and when cloud assistants are switched, and the stream's hello carries the view, so one sent before a
client subscribed isn't lost. Every client reads `networkView` when its network screen opens; that asks the other
computers in the background when what the host heard is over a minute old, so presence comes up to date through the
event. The first ask after the host starts is left to bot listings (`list_bots`), which wait up to 5 s for it.

`create` prints the **recovery code** once: the network's root key, kept on no computer. The connector picks the
fallback when two computers can't connect directly: iroh's public relays (`public`, the default), the user's own
`iroh-relay` (a URL), or none (`private`: same network, Tailscale or a VPN). A computer that moves to another network signs
where it is again and tells the others, so it stays reachable without a restart. A computer's name defaults to its host
name; a taken name gets a number (`laptop-2`), also when two admins admit the same name before their copies meet
(the later admission gets the number, on every copy). No computer's name changes when another is removed; one
admitted after a removal can take the freed name.

## Several networks

Each network this computer is in has its **own node**: its own key, roster, connector and rules, in its own folder
of the data directory (`network` for the first, `network-<id>` after), listed in kv `networks`
(`host/src/network/saved.rs`). One node per network, rather than one node with several rosters, keeps networks
apart: members of one can't tell the computer is in another (different keys), a stolen key or a bad roster in one
reaches no other, connectors differ per network (a private home network, a work one on relays), and nothing on the
wire changed. The cost is one endpoint per network.

- The view (`networkView`, the `network` event, the hello) lists `networks`, the first being the default, each with
  its `id` (local to this computer), `key` (the network's own id, its root key: the same on every computer in it),
  `name`, this computer's name and role there, `canInvite`/`canRecover`, `connector`, `computers` and
  `rules`. Methods take `network` (`networkInvite`, `networkRemove`, `setNetworkRules`, …); `networkLeave` stops
  this computer's node for one and deletes its folder (the others list the computer until they remove it).
- An invite carries the network's name (`Node::set_network_name`, read with `codync_network::read_invite`), so a
  computer that joins calls it the same; a name already used here gets a number (`home-2`). Joining a network this
  computer is already in is refused.
- **Addresses** name the network when they need to: `reviewer@desk.home`. While the computer is in several,
  `list_bots` ids carry the network (`computer` reads `desk.home`, and the `computer` filter `desk` finds it); a
  bare name or `name@computer` is taken where exactly one network has it, and two candidates are refused with both
  full addresses. With one network, ids stay `reviewer@desk`.
- This computer's bots are offered in every network under the same address name; the per-bot offered switch applies
  to all of them. Each network's letters come in through its own node and their answers go back through it.
- A computer that was in one network before keeps it, under the name `home` (its folder and its old kv connector
  and rules are taken over once).

## Who may do what

The roster is a log of signed changes every member keeps a copy of; copies merge, and every computer works out the
same members whatever order the entries arrived in.

| Role | Can |
|---|---|
| Root key (recovery code) | name the owner (move it to another computer) |
| Owner (the computer that created the network) | invite, remove anyone, promote admins |
| Admin | invite, remove plain members |
| Member | message, list, block on itself |

A removed computer stays removed; it rejoins only with a new key. `--cascade` also voids every computer it
admitted (a stolen one). Every computer compares rosters with the others when it starts and every 6 hours, so one
that was off learns of a removal.

- **Recovery wins.** Only the root removes the current owner, and an owner that a later owner (or the root)
  removed with `--cascade` removes and promotes no one, whatever time it signs with: a stolen owner can't undo its
  replacement with backdated entries.
- **The log stays small.** Every copy keeps only entries that can count: none by a computer the log never let in,
  no admissions, promotions or removals by plain members, each computer's newest location only (none for a removed
  one), and at most 256 entries per computer, each within bounds (a name a computer could have, at most 64
  addresses, 8 KiB in all). So the whole roster always fits in the one frame (1 MiB) a hello carries it in.

## Bots across computers

- Every visible bot is offered under its **address name** (`Code Reviewer` → `code-reviewer`, a second
  `reviewer` → `reviewer-2`); from another computer it is `code-reviewer@desk`. The host keeps every name it ever
  gave out (kv `network_handles`), so a name never moves to another bot: a letter on its way to a renamed or
  deleted bot can't reach the wrong one. What the network offers is published after each bot is created, changed
  or deleted.
- `list_bots` lists this computer's bots, then the other computers' (ids `name@computer`, `status` online/away),
  filtered by `query`, `computer`, `online` and `kind`, and paged by `limit` (default 20, at most 50) and `cursor`.
  Every row says its `kind`: `codync` (a Codync bot), `claude-code`, `codex` (sessions a standalone daemon offers),
  `terminal`, or `other` (a computer from before kinds). Each computer states the kind in what it offers
  (`codync_network::AgentKind` on the wire `Agent`); for the sessions the network knocks on, the crate takes it from
  their `Reach`, and the network view's `agents` carry it too. A listing
  past the limit adds `summary` (bots per computer). What other computers offer is cached for a minute and refreshed
  in the background; the first listing waits up to 5 s for it.
- `message_bot` takes a bot id, an address name, or `name@computer`. A request to another computer is a letter; the
  recipient computer turns it into a bot request in that bot's chat (its notice says "Message from Lead on laptop"),
  and its outcome goes back as a letter that names the request (`reply_to`, `meta.codync` = `completed`, `failed` or
  `cancelled`). The sender's notice ends and the sender gets the usual reply turn; a cancelled one wakes no one.
  A request for a bot that went away after its computer took the letter comes back `failed`. An answer may come
  from a bot the computer no longer offers (renamed or deleted while it worked).
- Exchange notices carry `botMessage.peerName` ("Reviewer on desk") for the bot on the other computer, whose id
  isn't in this computer's bot list; every client shows that name in the row, the pair sheet and its author labels.
- Other computers' bots stay out of the apps' bot lists: a phone chats with a computer's own bots, and a bot
  elsewhere is reached by asking a local one. The network screens list them under their computer.
- A letter to a computer that is away waits in the sender's outbox for up to a day, and so does one for a computer
  that has started but not offered its bots yet (it answers *later*; offering them calls everyone back). A reply
  reaches its request even when the asking bot was renamed or hidden meanwhile. The chain's hop limit counts
  across computers (replies carry it back, one more handoff each), and the receiving computer enforces its own budgets:
  8 handoffs per chain, a burst of 5 messages per pair of agents then one every 2 minutes, the same words twice in
  10 minutes refused. A letter's id is at most 64 bytes, its addresses 256 (40 characters of any script), its body 64 KiB; what another computer
  offers is kept to its first 200 agents, names cut to 80 bytes and descriptions to 1 KiB. A letter that isn't a
  request (a notice, or a reply nobody waits for after a restart) shows as a "Note from …" notice in the bot's chat.
- Agents on this computer that aren't Codync bots (a Claude Code or Codex session) get `list_bots`, `message_bot`
  and `read_replies` from `codync-host mcp network --name "Claude Code"` (stdio MCP, local API `agentCall`, loopback
  only): the cloud assistant's tools (`chat/team/assistant.rs`), under the agent's own name ("Message from Claude Code
  (this computer)"), its own replies, no cloud switch and no per-bot cloud scope, and letters elsewhere sent as
  `agent-<name>@<computer>`. Add it with `claude mcp add codync -- codync-host mcp network --name "Claude Code"`.
- Agents that aren't Codync bots use the same letters: the standalone `codync-network` daemon offers Claude Code
  sessions (their inbox socket) and Codex threads (`codex queue`), see the crate README. It keeps its own key in
  `~/.codync-network` (the host's are `~/.codync/network` and `network-…`, one per network), so beside a host it is a computer of its own.

## Who reaches this computer's bots

Every check is the **receiving** computer's, like the message budgets: no rule depends on the sender's goodwill,
and nothing new crosses the wire beyond a refusal's reason.

- **Offered to the network**, per bot (`offeredToNetwork` on the bot, on by default): a bot that's off isn't
  published (`network::publish` filters it), so other computers don't list it, and a letter to it is refused like
  one to an unknown agent. It still messages bots elsewhere and hears back (the node sends as it without offering
  it, `Node::set_senders`). It's a bot setting, so any paired device may change it.
- **Reachable by cloud assistants**, per bot (`reachableByAssistants`, on by default): `list_bots` from the cloud
  assistant leaves it out and `message_bot` refuses it (`chat/team/assistant.rs`). A letter from the cloud
  assistant through another computer (`cloud-assistant@desk`) meets the same checks here, and this computer's own
  Cloud assistants switch: off, it refuses them all (`remote::take`).
- **Rules**, per network: an ordered list of `{fromComputer, fromAgent, toAgent, access}` (`allow`/`deny`, `*`
  for any). The first rule that matches decides; with none, everyone may write (`codync_network::rules`). The crate
  checks them in `node/inbound.rs` before it takes a letter, so the standalone daemon has them too (`rules.json`
  beside `agents.json`, `codync-network rules allow|deny <agent@computer> <agent> [--first]`, `rules remove <n>`).
  A refused letter's sender gets the reason as its tool error, for example "desk refused it: reviewer doesn't take
  messages from lead@laptop", and its exchange notice ends failed. An answer to one of this computer's own
  requests passes (the crate keeps each request's id, the computer it went to and the agent that sent it), so a
  rule added while a request runs doesn't strand it: only from that computer, only to that agent, and only until
  the answer that says how it ended (`meta`). The host keeps each network's rules with it in kv `networks`
  (`network/saved.rs`), applies them when its node starts, and changes them with `setNetworkRules {network, rules}`
  (loopback only, like membership); `networkView` shows them, so phones list them read-only.
- The master switches stay at the computer: being in a network, and cloud assistants on or off.

## Not yet

Planned in order, with what each step touches: [what's left](../plans/codync-network-next.md).

- Shared networks (other people's computers): rules are per network, but which bots are offered is one switch per
  bot for all networks; a per-network list of offered bots comes with networks shared with other people.
- Bots can't reach Claude Code or Codex sessions on another computer through the host (the standalone daemon can);
  those sessions reach the bots with `codync-host mcp network`, and the replies wait for `read_replies`.
- Each computer's share of the roster is capped (256 entries) and keeps its oldest: an owner or admin that has
  made 256 admissions, promotions and removals can't make more. Hellos sending changes since the last one, instead
  of the whole roster, would lift the cap.

## Code and verification

- `host/crates/network`: the crate (`node/`, `roster.rs`, `rules.rs`, `guard.rs`, `reach.rs`, `wire.rs`); `cargo test -p codync-network`
  runs computers on one machine over direct connections, without a relay or internet.
- `host/src/network/`: the host's nodes (one per network, `mod.rs`), the saved networks (`saved.rs`), published bots
  and address names (`handles.rs`), the `network*` API methods, `networkView` and the `network` event (`api.rs`).
- Clients: desktop `apps/desktop/src/renderer/views/settings/NetworkSection.tsx` (+ `NetworkCard.tsx`,
  `NetworkSheets.tsx`, `RuleSheet.tsx`, `store/network.ts`) and the bot editor's `views/bots/NetworkSwitches.tsx`;
  terminal `host/src/tui/manage/network.rs` (+ `network_setup.rs`, `network_rules.rs`) and `view/network.rs`
  (+ `view/network_rules.rs`); iPhone `apps/ios/Kit/Sources/CodyncUI/NetworkSheet.swift`; Android
  `apps/android/app/.../NetworkSheet.kt`.
- `host/src/chat/team/remote.rs`, `directory.rs`: requests and replies across computers, `list_bots`;
  `assistant.rs`: the cloud assistant and agents that aren't bots (`codync-host mcp network`, `agentCall`).
- `host/src/network_cli.rs`: `codync-host network`.
- `host/tests/network_e2e.rs`: two real hosts in one network, a bot messaging a bot on the other computer through
  approval and back, a request its recipient's user stops and one that fails, and the view with its per-row
  permissions and the `network` event reaching both computers' apps on a promotion.
