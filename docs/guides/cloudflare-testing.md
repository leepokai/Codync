# Testing the Cloudflare path

The remote channel, account approval and relay implementation exist. You can begin testing the **development** path after the prerequisites below. A successful iOS build/install is not an end-to-end relay result. Production configuration remains incomplete; see [environments](environments-and-deployment.md).

## Prerequisites

- Run matching app/host major versions and replace old running processes after installation ([development](development.md)).
- Point both app and host at the intended cloud. Debug app configuration uses `https://dev-api.codync.dev`; check `codync-host cloud` on the computer. Enable/configure it through **Reach from anywhere** or the CLI when needed.
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

## Troubleshooting

- **Direct works, relay fails:** compare cloud URLs, cloud enablement, registration status and host logs.
- **Signed in, no access:** check account ownership and the host's pending approval; compare the displayed code before approving.
- **Old UI/protocol:** stop stale Mac/host copies and relaunch the installed phone process.
- **Chat works, push fails:** inspect notification permission and APNs configuration separately.
- **Release fails while Debug works:** inspect the main environment's empty configuration before diagnosing networking.

Protocol details and security boundaries: [remote relay reference](../reference/remote-relay.md). Account/SSH behavior: [accounts and SSH](accounts-and-ssh.md).
