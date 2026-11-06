# Cloud assistants (the Codync MCP connector)

A hosted assistant the user already talks to (ChatGPT, Claude on the web or phone, any MCP client) adds Codync as
a connector at `https://mcp.codync.dev/mcp`, signs in with the Codync account, and can then list the user's bots,
give one a task and read its reply. The bots run on the user's computers as always; the assistant reaches them
through the Codync cloud over the computer's relay socket, like public routine webhooks
([relay reference §7.9](../reference/remote-relay.md#79-cloud-assistantmcp-connector)). The reasons behind each
choice: [design](../design/codync-network.md#cloud-assistants).

## How it works

```
ChatGPT / Claude ──MCP over HTTPS──▶ mcp.codync.dev (cloud Worker)
                                     │  OAuth: Clerk issues a token for this URL, bound to the Codync account
                                     ▼
                         the account's computer's relay DO ──assistant.call──▶ codync-host
                                                          ◀─assistant.result──  (answers only if turned on)
```

- **Sign-in**: Clerk is the authorization server; the Worker is only the protected resource. The connector's
  401 names its metadata (`/.well-known/oauth-protected-resource/mcp`), which points at Clerk. Clients register
  with a Client ID Metadata Document (CIMD: ChatGPT and Claude do); dynamic registration (DCR) stays off, the
  MCP spec deprecates it. Tokens are JWTs with `aud` = the connector URL, and the Worker accepts no other
  audience, so a token Clerk issued for something else never works here.
- **Protocol**: Streamable HTTP with JSON answers and no sessions. Stateless clients (2026-07-28: version in each
  request's `_meta`, `server/discover`) and `initialize` clients (2025-03-26 to 2025-11-25) both work.
- **Tools**: `list_computers` (from the account, answered by the Worker), and `list_bots`, `message_bot`,
  `read_replies`, answered by the host (`list_bots` narrows by `query` and `kind`: `codync`, `claude-code`, `codex`,
  `terminal`, `other`). With several computers online, a listing asks each; a message needs
  `computer` then. `message_bot` returns at once; the bot's reply waits on the host for `read_replies`, since
  nothing can wake a hosted assistant.
- **Host** (`host/src/chat/team/assistant.rs`): the assistant messages bots like a bot does
  (`Back::Assistant`): the bot's chat shows "Message from Cloud assistant" and the reply is kept in memory (the
  newest 50 requests, at most 16 running) until read. A bot with **Reachable by cloud assistants** off
  (`reachableByAssistants`) is left out of `list_bots` and refused by `message_bot`; agents on this computer using
  `codync-host mcp network` aren't held to it. `list_bots` also lists the bots the user's other
  computers in this one's network offer (`reviewer@laptop`), and `message_bot` sends those a letter from
  `cloud-assistant@<this computer>` (`Node::set_senders`: it may send but isn't offered, so nothing elsewhere can
  ask it). That computer decides whether it takes it with its own rules, its Cloud assistants switch and the bot's
  `reachableByAssistants` (`remote::take`); the outcome comes back into the same replies, and a refusal ends the
  request as failed with that computer's reason. The receiving bot's chat shows "Message from Cloud assistant on
  desk". The relay session answers each call in a task of its own, so a listing that waits for the other
  computers doesn't hold up the phones on the same connection.

## Who can get in

The cloud sees these calls in plain text, and a bot can run anything on its computer, so the computer decides:

- **Off by default.** The **Cloud assistants** switch on the computer turns it on for that computer: the desktop
  app (Settings → Computers, beside the account rows, shown while the computer is in the account) and the terminal
  UI's network sheet (`N`, then `a`), or `codync-host cloud --assistants on`. All go through the local API
  `setAssistants`, loopback only. The setting is the `assistants` field of `cloudStatus` and of `networkView`, which
  phones read (they show it in the network sheet and can't change it); the `network` event carries a change. The
  view's `assistantsAvailable` says whether the computer is in a Codync account: the terminal UI and the phones show
  the switch only then, as the desktop app does.
- **Only the computer's own account.** The Worker routes a call only to computers the token's account owns, and
  the host checks the call's account against its owner again.
- **Not on a private relay**: it has no accounts, so the host ignores `assistant.call` there.
- Revoking: turn it off on the computer, or remove the connector's access in the Codync account (Clerk).

## Configuration

| Where | What |
|---|---|
| `cloud/wrangler.toml` | custom domains `mcp.codync.dev` (main) and `dev-mcp.codync.dev` (dev) on the same Worker; `ASSISTANT_LIMITER` 120 requests per account per minute |
| Clerk (each instance) | Agents → MCP server: the connector URL (`https://mcp.codync.dev/mcp`, dev `https://dev-mcp.codync.dev/mcp`) with public clients allowed. OAuth applications → Settings: CIMD on (any compatible client), DCR off, PKCE required, JWT access tokens with audience |
| Cloudflare zone | the configuration rule "API hosts: no Browser Integrity Check" lists both MCP hosts: assistants' servers call without a browser user agent |

Clerk's MCP server page re-runs its public checks (401 challenge with scopes, resource metadata, discovery); all
pass on both instances. A local check of a deployment:
`curl -i https://mcp.codync.dev/mcp` → `401` with `WWW-Authenticate: Bearer resource_metadata=…, scope=…`.

## Verification

- `cloud/test/assistant.test.ts`: discovery, token checks (missing, app session, other audience), both protocol
  eras, routing to one or several computers, refusals, a host going away; `HostCalls` timing out.
- `host/src/chat/team/assistant/tests.rs`: the switch, the account check, a real bot turn and `read_replies`.
- End to end (dev, 2026-10-26): a scratch host claimed by the Clerk test user `cloud-e2e+clerk_test@example.com`
  (code `424242`), assistants on; an OAuth client run with the dev test client "Codync MCP e2e" (public, redirect
  `http://127.0.0.1:8976/callback`) signed in on Clerk's page, got an `at+jwt` for `https://dev-mcp.codync.dev/mcp`,
  and listed the computer and its bots, messaged a Claude bot and read its reply ("pong").

## Current limits

What lifts them, and in what order: [what's left](../plans/codync-network-next.md).

| Limit | Why, and what lifts it |
|---|---|
| Hosts before 2.28.0 don't answer (`assistant.call` is ignored, the call times out after 10 s) | the host side ships in 2.28.0 |
| Only computers in the account that use the Codync relay; a private relay has no accounts | by design: the account is how the cloud knows whose computer it is |
| No hidden bots or groups | the same visibility rule as `list_bots` for bots |
| No callback when a reply is ready | MCP can't wake a hosted assistant; it calls `read_replies` |
| Replies in host memory: newest 50, at most 16 running, gone on restart | kept small and unencrypted data off disk; persist if users need it |
| 120 requests per account per minute (`ASSISTANT_LIMITER`) | Worker rate limit |
| Revoking: off on the computer is immediate; removing the connector or its grant in Clerk leaves an issued JWT valid until it expires (Clerk's `expires_in`, a day) | JWT access tokens are checked locally; opaque tokens (Clerk setting) would revoke at once at the cost of a Clerk call per request |
| No self-hosted connector: it lives in Codync's cloud Worker | the only self-hosted path is running the whole `cloud/` Worker with your own Clerk and pointing the host at it with `CODYNC_CLOUD_URL` (no relay token); a lighter self-hosted connector needs its own OAuth server |
| Not end-to-end encrypted | a hosted assistant can't hold a device key; documented in Privacy |
