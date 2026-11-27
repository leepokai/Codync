# MCP access: outside apps driving threads

Any MCP client (a coding agent in a terminal, an editor, a hosted assistant) can start and follow Codync threads,
run routines and workflows through two servers with the same tools:

- **On the computer**: `codync-host mcp network` (stdio, registered as `codync` by `codync-host network setup`),
  calling the local API `agentCall` (loopback only). With a token (`--token` or `CODYNC_MCP_TOKEN`) it acts as that
  app; without one it acts as "a session on this computer".
- **Through the cloud**: the connector at `mcp.codync.dev` ([cloud assistants](cloud-assistants.md)). The cloud
  reads these calls in plain text: they are not end-to-end encrypted.

A thread started this way is a plain [thread](work.md#other-sources) of a bot, or of the box of threads
without a bot, marked with who started it. Code: `host/src/chat/team/control/`.

## Who is calling

Every call has a principal (`clients::Principal`):

| Principal | How | Default level | Reaches |
|---|---|---|---|
| An app the user added | its token `cdy_mcp_…` (shown once, kept only as its sha256) | the level it was given (new: Ask) | the bots and folders it was given, threads without a bot if allowed; scopes `threads`, `allThreads`, `routines`, `workflows` |
| A session on this computer | `codync-host mcp network` without a token | `mcpLocalAccess`, Ask | every bot; threads, routines, workflows |
| The cloud assistant | the connector, after the account check | `mcpCloudAccess`, Read only | only bots with Reachable by cloud assistants on |

Every principal is also held to the computer's maximum (`mcpMaxAccess`, Auto). A token lasts 30 days by default and
at most 90, and can be renewed or revoked. Revoked or expired tokens are refused with their own code. At most 50 apps.
Any program the user runs can act as a session on this computer, which is why its level is a setting of its own.

Managing apps (`mcpClientCreate`, `mcpClientUpdate`, `setMcpAccess`) is local only: the desktop app's Computers page
(**MCP access**) and the terminal UI (`MCP access` in the actions). Each app's row there also has a switch for its
`routines` and `workflows` scopes (`mcpClientUpdate` `runs`, keeping its other scopes; `s` in the terminal UI), so an app
at Full can be let run them. Phones may only call `mcpClients` and `mcpClientRevoke`: the iPhone and Android apps show
the list (each app's level and expiry) under **MCP access** in the computer's card in Computer management, with Revoke,
when the host's `hello` says `mcpAccess`.

## Levels

`readOnly < ask < auto < full`. A call acts at the lowest of the app's level, the computer's maximum, its own
principal default, and the call's `accessLimit` (which can narrow it but never widen it).

| Level | May | A thread it starts runs |
|---|---|---|
| Read only | list, read, wait | — |
| Ask | start threads, write to them, steer and stop them, answer choices | with every request an approval card (network ceiling Ask) |
| Auto | the same | in the agent's own auto mode; anything it asks for is a card (ceiling Auto) |
| Full | the same, plus pick the agent's mode and model options, run routines and workflows | with requests approved by themselves |

Levels govern the worker tools only. The older messaging tools (`list_bots`, `message_bot`, `read_replies`, the
mailbox) keep their own switches: Cloud assistants and each bot's Reachable by cloud assistants (`Class::Messages`).

A thread keeps the level it was started at (`Task::started_by`): `work::access_of` turns it into the thread's
per-turn ceiling, so a restart doesn't widen it. An app may act only on threads at or below its level. A thread the
user started that approves its own requests counts as Full. **Approval cards are never answered through MCP**:
`thread_pending` lists them so the app can tell the user, but only choice cards (`decision`) can be answered.

## Tools

`worker_capabilities`, `thread_list`, `thread_launch`, `thread_send`, `thread_wait`, `thread_read`, `thread_steer`,
`thread_interrupt`, `thread_pending`, `routine_list`, `routine_run`, `workflow_list` and `workflow_run`. Schemas are
in `control/schema.rs`. The cloud's copy is in `cloud/src/routes/assistant.ts`.

- **Access classes** (`access::class`, one table with a completeness test): reads, starts threads, writes threads,
  writes the owner's work (routines and workflows run with the bot's own setup, so they need Full).
- **Order of checks** (`control::call`): the principal is live → rate limit (60 calls a minute per principal, first
  so a flood costs nothing) → level ceiling → read-only refusal → class and scope → bot, folder and no-bot
  allowances → the tool.
- **Refusals** carry a code: `capability_denied`, `mode_escalation_denied`, `not_found` (also for a thread the
  caller can't see), `invalid_request`, `rate_limited`, `revoked`, `expired`.
- **Idempotency**: `thread_launch`, `thread_send`, `routine_run` and `workflow_run` take `clientRequestId`. The same
  id from the same principal returns the first result with `replayed: true` (kept 7 days).
- **Limits**: 8 live threads per app, 32,000 characters a message, `thread_wait` up to 50 s locally and 8 s over the
  cloud (its 10 s call timeout).
- **Folders**: a folder must be under one the app was given, or one of the reachable bots' projects and folders. A
  folder is resolved through symlinks, must exist, and can't be in or contain Codync's data folder.
- **Framing**: the thread's context says it was started by an outside app and that its messages aren't the user's
  own words. Each message it sends reaches the agent prefixed with the app's name and is kept out of memory as user
  input (`pasted`).

## What the user sees

- Threads say **Started by <app> (MCP)** under their title (desktop, iPhone, Android) and in the terminal UI's
  thread details. The wire field is `startedBy: {label, client, level}`, absent on the
  user's own threads.
- The MCP access section lists the computer's three levels and the apps, each with its level, days left, renew and
  revoke. A host before MCP access answers `unknown method`, and the section hides itself.
- An audit log of the last 500 calls (`mcp.audit`: client, tool, outcome, code, time; never arguments) comes with
  `mcpClients`. Analytics: `mcp_client_created`, `mcp_client_revoked`, `mcp_call`, `mcp_thread_launched`, and
  `work_added.mcp` ([events](../reference/analytics-events.md)).

## Verification

- `control/access.rs`, `control/clients/tests.rs`, `control/threads/tests.rs`: every tool has a class, levels
  narrow, tokens, expiry and revocation, folders, replays, rate limits, visibility, what each level may act on.
- `host/tests/mcp_access_e2e.rs`: through `agentCall` with a token on a real host and scripted agents, an Ask app
  starts, replays, waits for, reads and writes to a thread that runs in a mode that asks; a Full app waits on and
  stops one; a revoked token is refused.

## Not built yet

- An HTTP `/mcp` endpoint on the host for apps that can't run a stdio server.
- A pairing-code flow to hand an app its token without copying it.
- `thread_steer` and `thread_interrupt` check the thread's level before taking the box's lock. `thread_send` checks
  it under the lock.
