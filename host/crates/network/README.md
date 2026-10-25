# codync-network

**Agent Tailscale.** The agents on your computers find and message each other, peer to peer: the bots on your
desktop, your laptop and your Linux box form one private network, the way Tailscale joins the computers themselves.
It is how the bots in [Codync](https://github.com/leepokai/Codync) reach each other across computers, as a
standalone crate, and it reaches Claude Code and Codex sessions too.

- **A key is an address.** Each computer has one key, and its public half is how the others reach it, whatever
  network or IP it is on today. No ports to open, no VPN to install.
- **Direct first.** Computers connect straight to each other over QUIC ([iroh](https://iroh.computer)): on the same
  network, or through NATs by hole punching. Only when no direct path exists does traffic go through a relay, which
  sees nothing but ciphertext.
- **Your connector.** Pick how computers reach each other when they can't connect directly: iroh's public relays
  (`public`, for trying it out), your own `iroh-relay` (a URL, and nothing of anyone else's), or no relay at all
  (`private`: same network, Tailscale or a VPN).
- **Invites, not accounts.** A computer joins with a one-time invite, good for an hour. No server decides who is
  in: the roster is a log of signed changes that every member keeps a copy of, and only the owner (the computer that
  created the network) and the admins it names may admit or remove computers. The recovery code shown once at
  creation moves the owner to another computer if it is lost. A removed computer stays removed.
- **Names like ssh.** An agent is `agent@computer` (`reviewer@desk`); a bare `reviewer` finds the one computer that
  offers it, this computer first. A taken computer name gets a number (`laptop-2`).
- **Wakes the agent, or leaves a note.** A message reaches the agent wherever it runs: the app's own agents (Codync's
  bots) through the app, a Claude Code session through the inbox socket Claude Code gives every session (read
  between tool calls while it works, a new turn when it is idle), a Codex thread through `codex queue`. A notice
  wakes no one: it goes in with the next message that does.
- **Quiet by design.** A message costs the receiving agent a turn, so nothing is sent unless an agent sends one or
  someone asks who is around: no presence heartbeats, no status pushes. The receiving computer enforces budgets
  whatever the sender's agents were told: a chain of handoffs (A asks B, B's answer makes A ask C, …) stops after 8,
  each pair of agents gets a burst of 5 messages and then one every 2 minutes, and the same words twice within 10
  minutes are refused. A sender's retry of a letter already taken is acknowledged, not delivered again.
- **Letters wait.** A letter for a computer that is asleep waits in the sender's outbox for up to a day and goes
  out when that computer answers again.

## Layers

New ways of working together don't change the layers below them:

| Layer | What it is |
|---|---|
| Computers (`Node`) | keys, the signed roster and its roles, invites, the connector, the outbox |
| Letters (`Letter`, `Message`) | text from agent to agent, `wake` or notice; `reply_to` links letters, `meta` carries an app's own structure untouched |
| Reach (`Reach`) | how a letter gets into the agent: the app, Claude Code, Codex |
| Budgets (`guard`) | chain length, bursts, repeats |

Requests and replies, rooms or shared task boards are patterns of letters, built by the app above the network.

## Status

0.x, developed inside Codync, whose host uses it to carry bot requests between the user's computers, and mirrored
here read-only. The API changes with Codync. Report issues here; send pull requests to
[Codync's `dev` branch](https://github.com/leepokai/Codync) (`host/crates/network`). Messages into Claude Code
sessions and the daemon's control socket work on macOS and Linux; on Windows the crate builds, and those come later.

## Install

The daemon and its command (`codync-network`), with Rust 1.91 or later:

```sh
cargo install codync-network
```

As a library:

```sh
cargo add codync-network
```

It builds iroh on rustls with ring. An app that already uses rustls with aws-lc-rs takes the other one, so the two
aren't built in together (rustls then can't pick a provider by itself):

```toml
codync-network = { version = "0.1", default-features = false, features = ["tls-aws-lc-rs"] }
```

## Try it

Start the node on each computer and keep it running (a terminal, or a login service):

```sh
codync-network init desk                         # the first computer, its owner; prints the recovery code
codync-network invite                            # prints codync-network:… (one use, one hour)
codync-network join 'codync-network:…' laptop    # on the other computer
```

Then, from any terminal or agent on either computer:

```sh
codync-network peers                             # desk  owner  online  terminal
codync-network send terminal@desk hello          # delivered to terminal@desk
codync-network send terminal@desk --notice "tests are green"
codync-network promote laptop                    # the owner makes laptop an admin
codync-network remove laptop [--cascade]         # out for good (--cascade: and whom it admitted)
```

To put a Claude Code session or a Codex thread on the network, run this in it (or have the agent run it):

```sh
codync-network agent reviewer claude "$CLAUDE_CODE_SESSION_ID" --about "Reviews changes"
codync-network agent coder codex "$CODEX_THREAD_ID" --about "Writes the code"
```

A message to `reviewer@desk` then shows up in that session, with the command to reply. A Claude Code session that
skips permission prompts holds messages from outside it for your approval, unless its `crossSessionInbound` setting
is `accept`.

`--connector public|private|https://your-relay` picks the connector (default `public`), `--dir` the state folder
(default `~/.codync-network`: the key in `secret.key`, the roster in `network.json`, the agents in `agents.json`).
A Codync host keeps its own node in `~/.codync/network`; never point two nodes at one folder.

## Use

```rust
use codync_network::{Address, Agent, Connector, Letter, Node, Reach};

let (node, mut inbox) = Node::start(dir, &Connector::Public).await?;
node.join(invite, "laptop").await?;
let coder = Agent { id: "coder".into(), name: "Coder".into(), about: "Writes the code".into() };
node.set_agents([(coder, Reach::App)]);

let to: Address = "reviewer@desk".parse()?;
node.send(Letter::new("coder", to, "Review src/auth.rs")).await?;   // Delivered, or Queued while desk is away

while let Some(message) = inbox.recv().await {
    println!("{} → {}: {}", message.from, message.to, message.body);
}
```

## Build

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

The tests run two computers on one machine over direct connections, without a relay or internet.

## License

Apache-2.0
