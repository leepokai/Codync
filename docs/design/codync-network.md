# codync-network and cloud assistants: design

Why the user's computers, their bots and hosted assistants connect the way they do, the options weighed, and
what comes next. How it behaves today: [codync-network](../features/codync-network.md) and
[cloud assistants](../features/cloud-assistants.md); the relay protocol: [remote relay §7.9](../reference/remote-relay.md).

Status (2026-10-26): built on `feat/codync-network`. The cloud side (`mcp.codync.dev`, Clerk, the zone rule) is
deployed to dev and production; the host side ships in 2.28.0. The crate is on crates.io (0.1.0) and mirrored to
leepokai/codync-network.

## Goals

- A bot on one of the user's computers messages a bot on another the way bots on one computer already do.
- The user sets up nothing beyond one invite per computer: no extra app, account, port or server.
- Quiet: agents trade a few self-contained letters, never a chat that runs on by itself.
- Agents that aren't Codync bots (a Claude Code session, a Codex thread) can take part, and so can an assistant
  the user already uses on the web (ChatGPT, Claude).
- The cloud never decides who is in, as with devices: it may only shorten access.

## Why iroh

[iroh](https://iroh.computer) is a library that connects two programs by public key over QUIC.

| What it gives | Why it matters here |
|---|---|
| Dialing a key, not an IP | A computer is found after it changes networks; the key is also its identity in the roster, so authentication and encryption come with the connection, with no certificate authority or account |
| Hole punching, relay fallback, path switching | Direct when the network allows, through a relay otherwise, and a connection moves between the two without dropping |
| A library, not a VPN | Built into `codync-host`: no install, admin rights, sign-in or network extension, and only Codync's traffic uses it (the rest of the computer isn't exposed the way a whole-machine VPN exposes it) |
| No coordination server | Membership lives in the signed roster the computers keep; relays only pass traffic and keep nothing |
| QUIC, open source (MIT/Apache) | Many streams per connection, connection migration; self-hostable |

The core is Rust, native to the host. Official bindings exist for Python, Swift, Kotlin, JavaScript (Node.js) and
C, Go is community-maintained ([languages](https://docs.iroh.computer/languages)); the Kotlin build doesn't cover
Android yet. Agents in other languages don't need them: they use the `codync-network` CLI or daemon.

Costs: the public relays are rate-limited and not meant for production; bindings trail the Rust releases; 1.0 is
young; a network that blocks UDP keeps a computer on the relay.

**Alternatives considered.** Tailscale needs its own app or daemon, an account and a coordination server, and joins
the whole machine to the network; it stays useful as a direct path (`private` connector). Codync's Cloudflare
relay can't punch holes, and computer-to-computer traffic would keep Durable Objects awake, which is what drives
the bill (the 2026-10-17 incident).

## How two computers reach each other

1. **Directly**: same network, Tailscale, or a hole punched through the router.
2. **Through an iroh relay** when no direct path works. Traffic stays end-to-end encrypted between the two
   computers; iroh can start on the relay and move to a direct path once one opens.
3. **Later**: a letter for a computer that's off waits in the sender's outbox for up to 24 hours, retried every
   30 seconds; a computer that comes back says hello and gets it. Rosters are compared every 6 hours.

The connector picks the relay (`codync-host network create|join --connector …`):

| Connector | Paths | Relay |
|---|---|---|
| `public` (default) | direct, then relay | iroh's public relays, run by n0 (free, shared, rate-limited) |
| `https://…` | direct, then relay | an `iroh-relay` the user runs |
| `private` | direct only | none: same network, Tailscale or a VPN |

Codync's Cloudflare relay isn't on this path: it carries the phone ↔ computer channel and the cloud assistant.

Each computer signs where it is into the roster (`Locate`) when the host starts and again whenever its addresses
change (iroh's address watcher, 2 s after a burst settles), then says hello to everyone, so a laptop that moved to
another network is found without a restart, even with the `private` connector. The roster keeps only each
computer's newest location, so moving doesn't grow the log. With a relay, a computer that moved is also found
through its relay and iroh's address lookup.

The host builds iroh on aws-lc-rs, the TLS library reqwest and tokio-tungstenite use: with ring built in as well,
rustls has no default provider and the relay connection panicked (found in end-to-end testing, 2026-10-26). `main`
also installs the aws-lc-rs provider, so a `--workspace` build (where the crate's default `tls-ring` comes back)
works too. The crate keeps ring as its own default (`tls-ring`, `tls-aws-lc-rs` features).

## Relays for production (planned)

Users never run a relay. Codync runs `iroh-relay` in at least two regions (North America and Europe, small VPSs with
ACME TLS) and makes them the default connector; `public` stays for development, and the URL and `private`
connectors stay for users who want their own. It mirrors the phone path: Codync's relay by default, the user's own
optional.

| Option | For | Against |
|---|---|---|
| Codync-run relays (chosen) | cheap, no rate limits, under Codync's control | two servers to run and update with iroh |
| n0's dedicated relays (Iroh Services) | no operations | monthly cost, another dependency |
| Letters over Codync's Cloudflare relay when iroh fails | reuses the relay | networks aren't tied to accounts, so it needs its own authorization; the most work |
| iroh's relay protocol on Workers | one platform | must track iroh's protocol; relay traffic keeps Durable Objects awake |

## Membership and trust

- One-time invites (1 hour). The roster is a log of signed changes every member keeps; copies merge the same way
  in any order. Owner (the creator), admins, members; the recovery code is the root key, printed once and kept on
  no computer, and it can move the owner. Removal is permanent; `--cascade` also removes what a stolen computer
  admitted. Details: [codync-network](../features/codync-network.md#who-may-do-what).
- Sign-in stays optional. Networks today need no account (anonymous); account networks, where the account vouches
  for its computers, are planned beside them. Either way the computers, not the cloud, admit members.

## Addresses

- `agent@computer`, like an email or SSH address. A bare name finds the nearest bot (this computer first); a taken
  computer name gets a number (`desk-2`); a bot's address name is never reused for another bot.
- A computer can be in several networks; the `.network` suffix names one (`reviewer@desk.home`), the way a DNS
  search domain resolves short names: a short address is taken where only one network has it.

## Several networks: one node each

A computer in several networks runs one node per network, each with its own key, roster, connector and rules
(chosen over one node holding several rosters, which the plan first described):

| | One node per network (chosen) | One node, several rosters |
|---|---|---|
| Privacy | a different key in each: one network's members can't tell the computer is also in another | one key everywhere links them |
| Isolation | a stolen key or bad roster stays in its network | one endpoint and key shared by all |
| Connectors | per network (private at home, relays for work) | one per computer |
| Wire | unchanged: old peers keep working | every request names its network |
| Cost | one endpoint (a socket, a relay connection) per network | one endpoint |
- `list_bots` takes `query`, `computer` (`this` for this one), `online`, `kind` (`codync`, `claude-code`,
  `codex`, `terminal`, `other`), `limit` and `cursor`, and an unnarrowed
  long list says how many bots each computer has, so a computer with many bots doesn't flood the agent. On one
  computer without a network it lists that computer's bots.

## Letters, quiet by design

- Letters, not calls: the sender goes on at once and the reply arrives as a new turn. Nothing waits on another
  agent, so an idle or busy agent costs nothing.
- The receiving computer enforces the budget: 8 handoffs per chain, a burst of 5 letters per pair of agents then
  one every 2 minutes, the same words twice within 10 minutes refused. Notices don't wake anyone. Letter ids make
  retries arrive once.
- `reply_to` and `meta` carry collaboration modes (hand-off, review, broadcast) without changing the protocol.
- Who can take a letter (`Reach`): a Codync bot (the host starts its turn), a Claude Code session (its inbox
  socket), a Codex thread (`codex queue`).

## A server (VPS)

A VPS can be a member (the host or the standalone daemon, with its own bots or agents), the place Codync's
`iroh-relay` runs, or both. The cloud assistant reaches it like any other computer in the account.

## Cloud assistants

- **In the existing cloud Worker** at `mcp.codync.dev`, with **Clerk as the authorization server**: assistants sign in
  with the Codync account (Google, Apple). Clients register with Client ID Metadata Documents (dynamic client
  registration is deprecated in MCP and stays off), and access tokens are JWTs bound to the connector's URL.
- **Through the computer's relay socket**, like public routine webhooks; the computer is the authority: off until
  turned on there, only for its own account, never on a private relay.
- **Not end-to-end encrypted**: a hosted assistant can't hold a device key and the cloud has to speak MCP, so the
  cloud reads these calls. That is why the computer has to opt in, and the manual says so.
- **`read_replies` instead of a callback**: MCP can't wake a hosted assistant; replies wait on the host (in memory).
- **The Worker defines the tool list** so adding the connector works while every computer is off; the host does the
  work behind each tool.

## Who sees what

| Path | Readable by |
|---|---|
| Phone ↔ computer | the two ends only |
| Computer ↔ computer (network) | the two computers only; a relay sees ciphertext |
| Cloud assistant | Codync's cloud and the assistant's provider |
| Public routine webhook | Codync's cloud |

## Next, in order

The build plan, with what each step touches and when it's done: [what's left](../plans/codync-network-next.md).

Done: the network screens and the cloud assistants switch in every client (2.29.0): the desktop app and the
terminal UI manage the network, phones show it, and a `network` event keeps them current. Per-bot offering, the
cloud assistant scope and inbound rules, checked by the receiving computer. A computer that moves re-signs where it
is. Cloud assistants and agents that aren't Codync bots (`codync-host mcp network`) reach the bots across the
network. A computer can be in several networks.

1. Codync-run iroh relays as the default connector.
2. Hellos that carry only what changed, lifting the roster's per-computer cap.
3. Account networks; per-network lists of offered bots, for networks shared with other people.
4. A control channel for the standalone daemon on Windows (named pipe).
