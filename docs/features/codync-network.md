# codync-network

The user's computers join one private network, and a bot on one messages a bot on another the way bots on one
computer already do ([bot collaboration](bot-collaboration.md)). The network is the `codync-network` crate
(`host/crates/network`, its own [README](../../host/crates/network/README.md)): peer to peer over QUIC
([iroh](https://iroh.computer)), with no Codync server involved. It is off until the user creates or joins a network.
Why it is built this way, the options weighed and what comes next: [design](../design/codync-network.md).

## Set it up

From a terminal on each computer (the API methods are loopback-only: phones can't change who is in):

```sh
codync-host network create [name] [--connector public|private|https://relay]   # the first computer: the owner
codync-host network invite                                                     # prints a one-time invite (1 h)
codync-host network join '<invite>' [name] [--connector …]                     # on the other computer
codync-host network                                                            # who is around (status)
codync-host network peers | promote <computer> | remove <computer> [--cascade] | recover <code>
```

`create` prints the **recovery code** once: the network's root key, kept on no computer. The connector picks the
fallback when two computers can't connect directly: iroh's public relays (`public`, the default), the user's own
`iroh-relay` (a URL), or none (`private`: same network, Tailscale or a VPN). A computer's name defaults to its host
name; a taken name gets a number (`laptop-2`), also when two admins admit the same name before their copies meet
(the later admission gets the number, on every copy). No computer's name changes when another is removed; one
admitted after a removal can take the freed name.

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
  no admissions, promotions or removals by plain members, each computer's newest location only, and at most 256
  entries per computer. So the whole roster always fits in the one frame (1 MiB) a hello carries it in.

## Bots across computers

- Every visible bot is offered under its **address name** (`Code Reviewer` → `code-reviewer`, a second
  `reviewer` → `reviewer-2`); from another computer it is `code-reviewer@desk`. The host keeps every name it ever
  gave out (kv `network_handles`), so a name never moves to another bot: a letter on its way to a renamed or
  deleted bot can't reach the wrong one. What the network offers is published after each bot is created, changed
  or deleted.
- `list_bots` lists this computer's bots, then the other computers' (ids `name@computer`, `status` online/away),
  filtered by `query`, `computer`, `online`, and paged by `limit` (default 20, at most 50) and `cursor`. A listing
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
- A letter to a computer that is away waits in the sender's outbox for up to a day, and so does one for a computer
  that has started but not offered its bots yet (it answers *later*; offering them calls everyone back). A reply
  reaches its request even when the asking bot was renamed or hidden meanwhile. The chain's hop limit counts
  across computers (replies carry it back, one more handoff each), and the receiving computer enforces its own budgets:
  8 handoffs per chain, a burst of 5 messages per pair of agents then one every 2 minutes, the same words twice in
  10 minutes refused. A letter that isn't a request (a notice, or a reply nobody waits for after a restart) shows as
  a "Note from …" notice in the bot's chat.
- Agents that aren't Codync bots use the same letters: the standalone `codync-network` daemon offers Claude Code
  sessions (their inbox socket) and Codex threads (`codex queue`), see the crate README. It keeps its own key in
  `~/.codync-network` (the host's is `~/.codync/network`), so beside a host it is a computer of its own.

## Not yet

Planned in order, with what each step touches: [what's left](../plans/codync-network-next.md).

- The apps have no network screen: setup is the CLI above.
- One network per computer; the `.network` address suffix comes with several.
- Shared networks (other people's computers) need the per-network list of offered bots and inbound rules first.
- Bots can't reach Claude Code or Codex sessions on another computer through the host (the standalone daemon can).
- The cloud assistant connector ([cloud assistants](cloud-assistants.md)) reaches only the bots of the computer it
  talks to, not that computer's network.
- Each computer's share of the roster is capped (256 entries) and keeps its oldest: an owner or admin that has
  made 256 admissions, promotions and removals can't make more. Hellos sending changes since the last one, instead
  of the whole roster, would lift the cap.

## Code and verification

- `host/crates/network`: the crate (`node/`, `roster.rs`, `guard.rs`, `reach.rs`, `wire.rs`); `cargo test -p codync-network`
  runs computers on one machine over direct connections, without a relay or internet.
- `host/src/network/`: the host's node, published bots and address names, the `network*` API methods.
- `host/src/chat/team/remote.rs`, `directory.rs`: requests and replies across computers, `list_bots`.
- `host/src/network_cli.rs`: `codync-host network`.
- `host/tests/network_e2e.rs`: two real hosts in one network, a bot messaging a bot on the other computer through
  approval and back, and a request its recipient's user stops and one that fails.
