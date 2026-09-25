# Codync cloud

Cloudflare Worker for accounts and the off-LAN relay (spec: [docs/remote-relay-spec.md](../docs/remote-relay-spec.md)).

- **`/v1` API** (`src/api.ts`): Clerk-authenticated account routes (devices, claims, computers, access
  requests, grants), `Codync-Sig`-authenticated host routes (`/v1/host/*`), the Clerk webhook, and a
  15-minute cron that expires requests and prunes nonces. D1 schema: `migrations/0001_init.sql`.
- **`ComputerRelay`** (`src/relay.ts`): one Durable Object per computer (SQLite storage, WebSocket
  Hibernation). It admits device sockets against the host-signed ACL, forwards channel frames it can't
  read, reports presence, and holds the offline mailbox. It is reachable only through requests the Worker
  builds itself (`X-Codync-Internal: 1`); `/internal/*` has no public route.
- **Auth** (`src/auth.ts`): Clerk session JWTs via `@clerk/backend` (`CLERK_SECRET_KEY` → JWKS fetched and
  cached per isolate; `CLERK_JWT_KEY` → networkless, dev/e2e only) and Ed25519 request signatures
  (WebCrypto).

The cloud sees ciphertext and routing metadata only. It can shorten access (revoke, block) but never grant
it: new devices come from the host's own QR pairing or its approval, and the ACL is signed by the host.

## Test

```bash
npm ci
npm run typecheck
npm test          # vitest in workerd: vectors, Codync-Sig, API, DO relay/mailbox
```

`test/ref.ts` is a device/host reference implementation on `@noble/*`, checked against
`docs/remote-relay-vectors.json` and reused by the relay tests and the e2e harness.

The vitest pool's bundled workerd may trail wrangler's; `vitest.config.ts` pins the test compatibility
date to the newest one it supports. `wrangler.toml` keeps the deploy date.

### End to end (spec §12.1)

```bash
(cd ../host && cargo build)
npm run e2e       # local D1 + wrangler dev + real codync-host + test/e2e/phone.ts
```

Env: `CODYNC_HOST_BIN` (default `../host/target/debug/codync-host`), `CODYNC_E2E_CLOUD_PORT` (default
8787), `E2E_VERBOSE=1` to stream wrangler and host logs. Needs `python3` for the fake agent.

## Deploy

Staging (the integration stage runs this; production needs the owner):

```bash
npx wrangler d1 create codync-staging          # put database_id into [env.staging]
npx wrangler d1 migrations apply codync-staging --env staging --remote
npx wrangler secret put CLERK_SECRET_KEY --env staging
npx wrangler secret put CLERK_WEBHOOK_SECRET --env staging
npx wrangler deploy --env staging               # → https://codync-cloud-staging.<subdomain>.workers.dev
```

| Name | Kind | Purpose |
|---|---|---|
| `CLERK_SECRET_KEY` | secret | JWKS for session tokens |
| `CLERK_WEBHOOK_SECRET` | secret | Svix signature on `/v1/webhooks/clerk` (`user.deleted`) |
| `CLERK_ISSUER` | var | Clerk Frontend API URL; tokens must carry this `iss` |
| `CLERK_AUTHORIZED_PARTIES` | var, optional | comma-separated; a token with `azp` must match |
| `CLERK_JWT_KEY` | var, dev/e2e only | public key for networkless verification |

New computers are limited to 10 per IP per minute with the Workers Rate Limiting binding
`REGISTER_LIMITER` (`[[ratelimits]]`; each environment needs its own `namespace_id`).
