# Heartbeat + Worker APNs Fallback (2026-03-21)

## Problem

CloudKit silent push is the primary sync path for iOS Live Activity updates, but iOS throttles it when the app is in background for a while or gets killed. This causes Live Activity to freeze with stale data.

The Worker APNs Live Activity push was previously removed because it was always-on (redundant when silent push works) and there was no way to know if the iOS app was still running.

## Proposed Solution: Heartbeat Detection + Smart Fallback

### Heartbeat Mechanism

1. iOS app writes a heartbeat record to CloudKit every 30 seconds while running
2. macOS reads the heartbeat timestamp before deciding which push path to use
3. If heartbeat is fresh (< 60s): iOS app is alive → silent push is sufficient → skip Worker
4. If heartbeat is stale (> 60s): iOS app is suspended/killed → send Worker APNs push

### Architecture

```
iOS app running:
  - Every 30s → write heartbeat to CloudKit
  - Silent push arrives → fetch → update Live Activity locally

iOS app suspended/killed:
  - Heartbeat stops → timestamp becomes stale
  - macOS detects stale heartbeat → sends Worker APNs push
  - APNs push updates Live Activity directly (no app needed)
```

### CloudKit Record

```
Record Type: "Heartbeat"
Record Name: "heartbeat-{deviceId}"
Fields:
  - deviceId: String
  - timestamp: Date
  - appState: String ("active" | "background" | "suspended")
```

## Paid Feature Tier (Future)

| | Free | Pro |
|--|------|-----|
| Foreground sync | CloudKit silent push | Same |
| Background sync | Silent push (iOS may throttle) | Worker APNs (direct Live Activity update) |
| App killed | Live Activity freezes | Worker keeps updating |
| Heartbeat detection | — | Smart fallback switching |

### Pro Selling Point

"Always-on Live Activity" — Dynamic Island and Lock Screen stay updated even when the app is closed.

### Cost Considerations

- Cloudflare Worker: free tier covers ~100k requests/day
- APNs: free (Apple doesn't charge)
- CloudKit heartbeat: minimal (1 write/30s per device)
- Main cost is APNs signing key management and Worker maintenance

## Implementation Order

1. Add heartbeat write to iOS app (CloudKit record every 30s)
2. Add heartbeat read to macOS CloudKitSync (check before save)
3. Re-enable Worker APNs push in CloudKitSync (only when heartbeat stale)
4. Add paid gate (UserDefaults flag or StoreKit check)

## References

- Worker code: `worker/src/index.ts` (still deployed)
- Previous APNs implementation: git commit `c88c368`
- Removal decision: `docs/architecture-decisions/2026-03-20-apns-worker-push-removed.md`
