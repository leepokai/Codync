# Codync relay

Cloudflare Worker that holds the APNs key and forwards pushes for codync-host.
The phone exchanges its APNs token for an AES-GCM **ticket** (`POST /register`);
hosts only ever see tickets (`POST /push`). A ticket is useless without the
worker's `TICKET_KEY`, and it only reaches the one device it was issued for.

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

It deploys as `codync-relay`. The iOS app points at `SharedStore.relayURL` in `CodyncKit`.
Rotating `TICKET_KEY` invalidates every ticket; phones re-register on launch.

## Test

```bash
npm test        # ticket seal/open round trip, `mutableContent` → `mutable-content: 1`
npm run typecheck
```
