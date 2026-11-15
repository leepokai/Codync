# Codync cloud

Cloudflare Worker for accounts and the off-LAN relay (spec: [docs/reference/remote-relay.md](../docs/reference/remote-relay.md)).

- **`/v1` API** (`src/api.ts`, route groups in `src/routes/`): Clerk-authenticated account routes (devices, claims, computers, access
  requests, grants), `Codync-Sig`-authenticated host routes (`/v1/host/*`), the Clerk webhook, and a
  15-minute cron that expires requests and prunes nonces, a 5-minute cron that runs the release gate
  while a release waits behind GitHub's latest mark or the Homebrew tap trails it (`src/release-gate.ts`), and the public routine
  webhook route `POST /v1/hooks/:computerId/:routineId` (spec §7.8). D1 schema: `migrations/` (later files rebuild some tables).
- **Cloud assistant connector** (`src/routes/assistant.ts`, spec §7.9): the MCP server at `mcp.codync.dev/mcp`
  (dev `dev-mcp.codync.dev`, same Worker) that ChatGPT, Claude and other MCP clients sign in to with the Codync
  account (Clerk OAuth, tokens bound to the connector URL); its tool calls go to the account's computers through
  their DOs (`src/host-calls.ts`), and only a computer that turned cloud assistants on answers them.
  Setup and checks: [docs/features/cloud-assistants.md](../docs/features/cloud-assistants.md).
- **`ComputerRelay`** (`src/relay.ts`): one Durable Object per computer (SQLite storage, WebSocket
  Hibernation). It admits device sockets against the host-signed ACL, forwards channel frames it can't
  read, reports presence, holds the offline mailbox (`src/relay-mailbox.ts`), queues routine webhook deliveries
  (`src/relay-hooks.ts`, checked against the keys the host registers; `src/hooks.ts`), and wakes a sleeping cloud
  computer (`src/cloud-machines.ts`). Limits and wire types: `src/relay-wire.ts`. It is reachable only through
  requests the Worker builds itself (`X-Codync-Internal: 1`); `/internal/*` has no public route.
- **Auth** (`src/auth.ts`): Clerk session JWTs via `@clerk/backend` (`CLERK_SECRET_KEY` → JWKS fetched and
  cached per isolate; `CLERK_JWT_KEY` → networkless, dev/e2e only) and Ed25519 request signatures
  (WebCrypto).

The channel relay sees ciphertext and routing metadata. The account API also stores account/device metadata. It can shorten access (revoke, block) but never grant
it: new devices come from the host's own QR pairing or its approval, and the ACL is signed by the host.

## Test

```bash
npm ci
npm run typecheck
npm test          # vitest in workerd: vectors, Codync-Sig, API, DO relay/mailbox
```

`test/ref.ts` is a device/host reference implementation on `@noble/*`, checked against
`docs/reference/fixtures/remote-relay-vectors.json` and reused by the relay tests and the e2e harness.

The vitest pool's bundled workerd may trail wrangler's; `vitest.config.ts` pins the test compatibility
date to the newest one it supports. `wrangler.toml` keeps the deploy date.

### End to end (spec §12.1)

```bash
(cd ../host && cargo build)
npm run e2e       # local D1 + wrangler dev + real codync-host + test/e2e/phone.ts
```

Env: `CODYNC_HOST_BIN` (default `../host/target/debug/codync-host`), `CODYNC_E2E_CLOUD_PORT` (default: a free
port), `E2E_VERBOSE=1` to stream wrangler and host logs. Needs `python3` for the fake agent.

## Deploy

Development deployment (see [environment readiness](../docs/guides/environments-and-deployment.md); these commands change remote resources). Run them only from `dev`, never from a branch: a branch tests on its own local Worker ([which cloud](../docs/guides/cloudflare-testing.md#which-cloud-a-change-is-tested-on)).

```bash
# Only if the database does not exist: npx wrangler d1 create codync-dev
# Confirm its database_id in [env.dev] before applying migrations.
npx wrangler d1 migrations apply codync-dev --env dev --remote   # includes 0005 (short pairing codes), 0006 (signed-out devices)
npx wrangler secret put CLERK_SECRET_KEY --env dev
npx wrangler secret put CLERK_WEBHOOK_SECRET --env dev
# The device certificate key: without it phones can't pair (503 notConfigured).
grep '^CODYNC_DEVICE_CERT_KEY=' ../.env.development | cut -d= -f2- | npx wrangler secret put DEVICE_CERT_KEY --env dev
npx wrangler deploy --env dev               # → https://dev-api.codync.dev
```

Every secret across the Workers, GitHub and local files, and what to do when one leaks: [secrets](../docs/reference/secrets.md).

| Name | Kind | Purpose |
|---|---|---|
| `CLERK_SECRET_KEY` | secret | JWKS for session tokens |
| `CLERK_WEBHOOK_SECRET` | secret | Svix signature on `/v1/webhooks/clerk` (`user.deleted`) |
| `DEVICE_CERT_KEY` | secret | Ed25519 private key (JWK JSON) that signs device certificates (`POST /v1/devices/certificate`); hosts compile in its public half (`host/src/environment.rs`), `CODYNC_DEVICE_CERT_KEY` on a host overrides it for a self-hosted cloud. Kept in the git-ignored `.env.development` / `.env.production` as `CODYNC_DEVICE_CERT_KEY` |
| `RELEASE_GATE_TOKEN` | secret, main only | fine-grained GitHub token, *Actions: write* on `leepokai/Codync`: dispatches the release gate |
| `CLERK_ISSUER` | var | Clerk Frontend API URL; tokens must carry this `iss` |
| `CLERK_AUTHORIZED_PARTIES` | var, optional | comma-separated; a token with `azp` must match |
| `CLERK_JWT_KEY` | var, dev/e2e only | public key for networkless verification |
| `ASSISTANT_LIMITER` | rate limit binding | cloud assistant (MCP connector) requests per account, 120 per minute |
| `PAIRING_CODE_LIMITER` | rate limit binding | short pairing codes typed per account and per IP, 10 per minute, a miss counting twice |
| `CLOUD_SECRET_KEYS` | secret, optional | Codync Cloud: `id:base64` AES-256 keys (space-separated, the first seals) that seal the provider keys users give it and each cloud computer's keeper vault key (`src/secrets.ts`; [bring your own cloud](../docs/plans/codync-cloud.md#bring-your-own-cloud)); unset = no cloud computers. Make one with `echo "k1:$(openssl rand -base64 32)"`; never reuse one across environments |
| `CLOUD_KEY_LIMITER` | rate limit binding | provider keys saved per account (each save calls the provider), 5 per minute |
| `BOAT_API_URL`, `INSTABOX_API_URL`, `CLOUD_AUTO_STOP_SECONDS` | var, dev/e2e only | Boat and InstaBox base URLs; provider auto-stop (Boat's trial needs ≤ 7200; InstaBox ignores it and pauses a machine idle for 30 minutes) |
| `POSTHOG_HOST`, `ANALYTICS_ENV` | var | PostHog's ingestion host for the Worker's own events (`src/analytics.ts`: a Codync Cloud computer's wakes, only while its host shares analytics); unset = nothing sent. `ANALYTICS_ENV=dev` (dev only) marks them `env: dev` |
| `HOST_RELEASES_URL` | var, dev only | the release channel cloud computers install and update from; unset: GitHub's releases ([dev release channel](../docs/guides/development.md#dev-release-channel)) |
| `CLOUD_HOST_ENV`, `CLOUD_HOST_BINARY_URL`, `CLOUD_MACHINE_PREFIX` | var, dev/e2e only | a cloud computer's host service environment (`CODYNC_…=value`, e.g. its cloud URL, analytics off); a host build to boot instead of the release; the machine name prefix ([cloud computer testing](../docs/guides/cloud-computer-testing.md)) |
| `HOST_FLOOR`, `HOST_FLOOR_REASON` | var, emergencies only | cloud hosts below it update at once; self-hosted ones below it get `426 hostUpdateRequired` at registration |
| `APP_FLOOR_IOS`, `APP_FLOOR_ANDROID`, `APP_FLOOR_DESKTOP`, `APP_FLOOR_REASON` | var, emergencies only | apps below their platform's floor get `426 appUpdateRequired` on every request |

Cloud computers (`src/routes/cloud-computers.ts`, provider calls in the sans-I/O `machines/` package) are
tested end to end on a real Boat sandbox with `npm run e2e:cloud`, and with the real apps against a local cloud with
`npm run cloud:stack` ([cloud computer testing](../docs/guides/cloud-computer-testing.md);
[trial log](../docs/plans/codync-cloud-boat-trial.md)).

`RELEASE_GATE_TOKEN` is a fine-grained token without expiry. If it's revoked, set a new one with
`npx wrangler secret put RELEASE_GATE_TOKEN`: without a working token the release gate runs only after
each release, and only the Worker's cron logs show it (`GitHub graphql: 401`, `release gate dispatch: 403`).

New computers are limited to 10 per IP per minute with the Workers Rate Limiting binding
`REGISTER_LIMITER` (`[[ratelimits]]`; each environment needs its own `namespace_id`).
Public routine webhook deliveries are limited to 60 per sender IP and hook per minute with
`HOOK_LIMITER` (same binding kind, its own `namespace_id` per environment).

## Remote screen relay

`POST /v1/host/screen-ice` issues one-hour Cloudflare Realtime TURN credentials to a
signed host belonging to an active account. The host first checks the calling
device's screen permission. The Worker requires `TURN_KEY_ID`, `TURN_KEY_API_TOKEN`
(server-only secrets) and `TURN_LIMITER` (12 issuances per account per minute).
The credentials are never cached in HTTP caches or stored in D1. See
[Remote screen setup and verification](../docs/features/remote-screen.md).
