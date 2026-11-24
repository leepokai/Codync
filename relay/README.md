# Codync relay

Cloudflare Worker that holds the APNs key and forwards pushes for codync-host.
The phone exchanges its APNs token for an AES-GCM **ticket** (`POST /register`);
hosts only ever see tickets (`POST /push`). A ticket is a bearer capability: anyone holding it can ask this Worker to push
to its device. Only the Worker can decrypt the raw APNs token. Do not log tickets.

## Deploy

```bash
npm install
npx wrangler secret put APNS_TEAM_ID       # Apple Developer team ID
npx wrangler secret put APNS_KEY_ID        # an APNs Auth Key (Keys → Apple Push Notifications service),
                                           # not an App Store Connect API key — both are AuthKey_*.p8
npx wrangler secret put APNS_SIGNING_KEY   # contents of the .p8 file
openssl rand -base64 32 | npx wrangler secret put TICKET_KEY
npm run deploy
```

It deploys as `codync-relay`, served at `https://push.codync.dev` (a custom domain: `*.workers.dev` is blocked in
mainland China) and still at its `workers.dev` address for older phones. The iPhone and Android apps point at
`SharedStore.relayURL` in `CodyncKit`; the host sends pushes registered for the `workers.dev` address to the custom domain.
Rotating `TICKET_KEY` invalidates every ticket; phones re-register on launch.

## Test

```bash
npm test        # ticket seal/open round trip, `mutableContent` → `mutable-content: 1`
npm run typecheck
```

## Notifications and Live Activities

See the [notification design](../docs/design/push-and-live-activity.md) for event
copy, encrypted content, ActivityKit payloads, configuration and device acceptance.
The Worker handles alerts and updates/ends for activities started on the phone.
Dynamic Island shares the ActivityKit update stream. This Worker does not start
activities remotely or refresh Home Screen widgets.
