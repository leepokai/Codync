# APNs Worker Push — Removed (2026-03-20)

## What It Was

A secondary push path for updating iOS Live Activities via a Cloudflare Worker relay:

```
macOS → POST to Cloudflare Worker → Worker signs APNs JWT → Apple APNs → iOS Live Activity widget
```

### Implementation

- **Worker**: `worker/src/index.ts` — Cloudflare Worker at `https://codync-push.kevin2005ha.workers.dev`
  - Receives `{pushToken, event, contentState}` from macOS
  - Signs with APNs credentials (Team ID, Key ID, .p8 signing key stored in Worker secrets)
  - Sends to `api.sandbox.push.apple.com` with push type `liveactivity`
  - Bundle ID: `com.pokai.Codync.ios.push-type.liveactivity`

- **macOS side** (`APNsPushService.swift`):
  - Fetches push tokens from CloudKit every 30 seconds (`PushToken` record type)
  - After each CloudKit save, calls `sendUpdate(session:)` for changed sessions
  - On session disappear, calls `sendEnd(sessionId:)` before deleting CloudKit record
  - Bearer token auth to Worker

- **iOS side** (`LiveActivityManager.swift`):
  - On `Activity.request(pushType: .token)`, observes `activity.pushTokenUpdates`
  - Saves hex-encoded push token to CloudKit as `PushToken` record (keyed by `pushtoken-{sessionId}`)
  - macOS reads these tokens to address APNs pushes

### Push Token Sync Chain

```
iOS starts Live Activity → gets push token from ActivityKit
  → saves to CloudKit (PushToken record)
  → macOS fetches tokens every 30s
  → macOS POSTs to Worker with token + contentState
  → Worker → APNs → iOS Live Activity widget updates on Lock Screen
```

## Why It Was Removed

### CloudKit Silent Push Is Sufficient

Testing on 2026-03-20 confirmed that CloudKit zone subscription silent push (`CKRecordZoneSubscription` with `shouldSendContentAvailable: true`) is:
- **Immediate**: every CloudKit record save triggers a silent push within ~1 second
- **Reliable**: consistently delivered while the iOS app is in background (not force-quit)
- **Sufficient**: iOS `didReceiveRemoteNotification` wakes the app, fetches fresh data, and calls `activity.update()` locally

### Worker Push Was Redundant

With silent push working, the Worker relay added complexity without benefit:
- Extra network hop (macOS → Worker → APNs) vs. native (macOS → CloudKit → Apple silent push → iOS)
- Push token sync overhead (CloudKit PushToken records, 30s fetch interval)
- Worker maintenance (secrets, deployment, sandbox vs production APNs endpoint)
- Both paths ultimately update the same Live Activity

### Known Limitation

**When the iOS app is force-quit by the user** (swipe up to kill), silent push is not delivered by iOS. In this scenario, the Worker APNs push would have been the only way to update the Live Activity on the Lock Screen. Without it, the Live Activity will show stale data until the user reopens the app.

This trade-off was accepted because:
1. Force-quit is an explicit user action — they chose to stop the app
2. The Live Activity will eventually be dismissed by iOS (8-hour expiry)
3. The complexity cost of maintaining the Worker path outweighed this edge case

## How to Restore

If needed in the future, the full implementation is in git history:
- Commit before removal: `c88c368` (feat: APNs push diagnostic logging)
- Key files: `APNsPushService.swift`, `CloudKitSync.swift` (the APNs call sites), `worker/src/index.ts`
- CloudKit record type: `PushToken` (still exists in schema, iOS still saves tokens)

To restore: re-add `APNsPushService.shared.fetchPushTokens()` and `sendUpdate(session:)` calls in `CloudKitSync.syncToCloud()` after the `saveBatch` succeeds.

## Current Architecture

```
macOS hooks → SessionStateManager → CloudKitSync → CloudKit
                                                      ↓
                                          Apple silent push (automatic)
                                                      ↓
                                    iOS didReceiveRemoteNotification
                                                      ↓
                                    CloudKitReceiver.fetch() → LiveActivityManager.updateSessions()
                                                      ↓
                                              activity.update() (local)
```

Single path. No Worker, no polling, no APNs relay.
