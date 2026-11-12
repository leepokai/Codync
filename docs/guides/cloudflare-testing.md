# Testing the Cloudflare path

The remote channel, account approval and relay implementation exist. You can begin testing the **development** path after the prerequisites below. A successful iOS build/install is not an end-to-end relay result. See [environments](environments-and-deployment.md).

## Which cloud a change is tested on

`dev-api.codync.dev` is shared: every session's sandboxes started with `--cloud`, the owner's Codync Dev on the Mac
and phones, and the Dev cloud computers use it. So it runs only code merged into `dev`, and a branch never deploys
there.

| What changed | Tested against | How |
|---|---|---|
| `cloud/` on a branch (a worktree) | that worktree's own local Worker (`wrangler dev --env local`, a local D1, its own port); `npm run dev` in `apps/desktop` starts one for the sandbox by itself | `npm run e2e` (a real host, a fake agent, test tokens), [real phones against a local cloud](#real-phones-against-a-local-cloud), and for cloud computers `npm run e2e:cloud` or `npm run cloud:stack` ([cloud computer testing](cloud-computer-testing.md)) |
| only the apps or the host | the dev cloud as deployed | Codync Dev builds, or a worktree sandbox with `npm run dev -- --cloud` |
| `cloud/` merged into `dev` | `dev-api.codync.dev` | `npm run deploy:dev` from a clean tree at `origin/dev` right after the merge is pushed (migrations first), so `dev-api` always runs what `dev` has ([releasing cloud changes](environments-and-deployment.md#releasing-cloud-changes)) |
| `cloud/` released | `api.codync.dev` | `npm run deploy:main` right after the merge into `main` |
| `relay/` (push) or `discord/` | their tests (`npm test`); each has a single deployment everyone uses | deploy once the change has merged, then check the real path |

- **A worktree sandbox picks by itself**: `npm run dev` gives the sandbox its own local cloud when the branch changes
  `cloud/` against `origin/dev` (committed or not; not on Windows yet), and leaves the dev cloud off otherwise; `--local-cloud` and `--cloud`
  choose one outright ([worktree sandboxes](dev-app.md#worktree-sandboxes)). The local cloud keeps its D1 and device
  certificate key in the sandbox folder, so pairings and the sandbox's account outlast a restart; it has no cloud
  computers unless started by hand with a provider key (`npm run cloud:stack`, [cloud computer testing](cloud-computer-testing.md)).
- **Never from a branch**: `npm run deploy:dev`, `wrangler d1 migrations apply codync-dev --remote` or
  `wrangler secret put … --env dev`. A branch's Worker on dev breaks what everyone else is testing, and the next deploy
  from `dev` takes it away again without a word. Something that must be on dev before its change merges (a binding, a
  secret) goes there only when the owner says so, and the change records it.
- **A local Worker belongs to its worktree**: its D1 sits in a temporary folder or the worktree's `.wrangler/` and goes
  with it; its keys (device certificate key, Codync Cloud sealing keys) are throwaway values the runner makes. A
  worktree sandbox's own cloud is the exception: its D1 and device certificate key stay in the sandbox folder's
  `cloud/` across runs, until `npm run dev -- --prune` deletes the sandbox. Either way it takes test tokens or dev
  Clerk sessions (`cloud:stack` and the commands below read the dev issuer's public key).
- **Point what should use it at it** as in [real phones against a local cloud](#real-phones-against-a-local-cloud)
  (a host also needs `CODYNC_DEVICE_CERT_KEY`, the public half of the Worker's device certificate key, to accept its phones) or
  [real apps against a local cloud](cloud-computer-testing.md#real-apps-against-a-local-cloud). A machine elsewhere
  (a cloud computer) reaches it through a tunnel (`boat forward --reverse` or a Cloudflare quick tunnel).
- **What a local Worker can't show**: Cloudflare's edge (custom domains, rate limits across locations, cron on its real
  schedule), the production Clerk instance, APNs and FCM. Those are checked on dev after the merge, which is why
  releasing a cloud change still tests on dev first.

## Prerequisites

- Run app/host versions within each other's `minApp` / `minHost` floors ([compatibility](../reference/compatibility.md)) and replace old running processes after installation ([development](development.md)).
- Point both app and host at the intended cloud. `iOS Dev` and `dist:*:dev` builds use `https://dev-api.codync.dev`; check `codync-host cloud` on the computer. Enable/configure it through **Reach from anywhere** or the CLI when needed.
- Keep the computer awake, online, and its host running. Ensure the selected agent works locally first.
- Pair through a fresh QR, or sign in and approve device access by comparing the six-digit code on the computer. A Google login alone does not authorize access.

## Prove traffic uses the relay

The client tries direct connectivity before falling back to Cloudflare. Successful chat on the same Wi-Fi can therefore exercise only the direct path.

1. Start with one paired computer and confirm its bot list loads.
2. On the phone, disable Wi-Fi and use cellular. Disable a VPN/Tailscale route to this computer for this scenario. Leave the host online.
3. Check the computer's connection route in the app. The cloud icon means relay (accessibility label: “Through the encrypted relay”); the Wi-Fi icon means direct. Require **relay** rather than direct or loopback; correlate host logs if the UI cannot establish the route.
4. Send a harmless prompt, observe streamed state and the final reply, then close/reopen the app and confirm the transcript catches up without duplicate entries.
5. Return to Wi-Fi and repeat; record whether the route changes. A direct result is valid for the direct scenario only.

Record app/host versions, cloud environment, route, network, scenario and observed result. Do not publish local bearer tokens, pairing links, session JWTs or private keys in logs/screenshots.

## Scenario checklist

| Scenario | Expected observation |
| --- | --- |
| QR pairing | Fresh code authorizes this device; expiry/reuse is rejected |
| Account access | Computer appears for its account; access requires host approval and matching six-digit code |
| Relay chat | Confirmed relay route, streamed updates, final response and reconnect catch-up |
| Network transition | Wi-Fi/cellular change reconnects without duplicate messages |
| Brief connection loss | Header shows connecting smoothly; transient reconnect does not insert a status row above bots |
| Host offline | UI reports unreachable; an eligible queued send follows mailbox behavior when the host returns |
| Cancel queued send | Canceled mailbox message does not execute when host reconnects |
| Device revoke | Revoked device cannot establish authorized access; other authorized devices continue |
| Account switching | Pairings, caches, bot links and widgets remain within the selected account/computer |
| APNs | With notification permission, background completion/approval notification arrives and opens the correct chat |

Mailbox is for eligible offline `send` messages, not general offline RPC. Retry/deduplication uses `clientNonce`; do not infer mailbox success merely from a local pending bubble. APNs uses the separate `relay/` service, so relay chat passing does not prove push delivery.

## Repeatable local integration

```sh
cargo build --manifest-path host/Cargo.toml
cd cloud
npm ci
npm test
npm run typecheck
env -u CODYNC_CLOUD npm run e2e
```

The integration runner starts an isolated Wrangler service and real host with temporary state, generated test authentication and a fake ACP agent. `CODYNC_HOST_BIN` can select the host binary; `CODYNC_E2E_CLOUD_PORT` selects the local cloud port. It covers relay pairing/chat/reconnect, mailbox cancellation/deduplication, account claims/approval/revocation, direct channels and bearer access boundaries. Non-loopback rejection coverage can be skipped when no suitable interface exists; inspect the output.

This runner does not validate a deployed Worker, actual Google consent, APNs or iOS background behavior. Cloud CI runs unit/type checks; run this integration command explicitly for the complete local scenario.

### Real phones against a local cloud

To walk a cloud change on the iPhone and Android apps before it is deployed (for example the device certificates and short pairing codes of 2.36.0), run Wrangler so it trusts the dev Clerk instance's tokens, and point a host and the debug apps at it. Debug builds of both apps accept plain `http` to `127.0.0.1`/`localhost` as a cloud.

```sh
PORT=8787   # any port free on this computer: another worktree's Worker may already hold this one
cd cloud && npx wrangler d1 migrations apply codync-local --env local --local
curl -s https://sunny-mollusk-8651.clerk.accounts.dev/.well-known/jwks.json   # → CLERK_JWT_KEY as PEM
npx wrangler dev --env local --port $PORT --var CLERK_ISSUER:https://sunny-mollusk-8651.clerk.accounts.dev \
  --var "CLERK_JWT_KEY:$PEM" --var "DEVICE_CERT_KEY:$JWK"          # a throwaway Ed25519 JWK
CODYNC_HOME=$TMP CODYNC_VAULT=folder CODYNC_CLOUD_URL=http://127.0.0.1:$PORT CODYNC_DEVICE_CERT_KEY=$PUB codync-host serve --port 19391
```

- iPhone: launch with `SIMCTL_CHILD_CODYNC_CLOUD_URL=http://127.0.0.1:$PORT` (also while running `npm run phone-account`).
- Android: build with `-PcodyncCloudUrl=http://127.0.0.1:$PORT` plus the dev `codyncClerkPublishableKey` and `codyncPushRelayUrl` (the three go together), and `adb reverse tcp:$PORT tcp:$PORT`.

## Troubleshooting

- **Direct works, relay fails:** compare cloud URLs, cloud enablement, registration status and host logs.
- **Signed in, no access:** check account ownership and the host's pending approval; compare the displayed code before approving.
- **Old UI/protocol:** stop stale desktop app/host copies and relaunch the installed phone process.
- **Chat works, push fails:** inspect notification permission and APNs configuration separately.
- **Release fails while Debug works:** inspect the main environment's empty configuration before diagnosing networking.

Protocol details and security boundaries: [remote relay reference](../reference/remote-relay.md). Account/SSH behavior: [accounts and SSH](accounts-and-ssh.md).
