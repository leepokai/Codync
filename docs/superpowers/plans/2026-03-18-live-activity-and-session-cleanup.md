# Live Activity Fix + CloudKit Session Cleanup

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Live Activity / Dynamic Island not showing on iOS, and ensure dead sessions are deleted from CloudKit so iOS doesn't display ghost sessions.

**Architecture:** Two independent fixes: (1) Add missing `NSSupportsLiveActivities` build setting to iOS target, (2) Change CloudKitSync to delete CloudKit records when sessions complete (PID death), plus a startup orphan cleanup.

**Tech Stack:** Swift 6, CloudKit, ActivityKit, WidgetKit

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `CodePulse.xcodeproj/project.pbxproj` | Modify | Add `NSSupportsLiveActivities = YES` to iOS target build settings (Debug + Release) |
| `CodePulse-macOS/Services/CloudKitSync.swift` | Modify | Sync completed sessions as deletes, not saves |
| `CodePulseShared/Sources/CloudKit/CloudKitManager.swift` | Modify | Add `deleteByIds()` and `deleteOrphans()` methods |
| `CodePulse-macOS/App/CodePulseApp.swift` | Modify | Call orphan cleanup on launch |

---

### Task 1: Enable Live Activity in iOS build settings

**Files:**
- Modify: `CodePulse.xcodeproj/project.pbxproj:601-606` (iOS Release build settings)
- Modify: `CodePulse.xcodeproj/project.pbxproj:627-632` (iOS Debug build settings)

The iOS target is missing `INFOPLIST_KEY_NSSupportsLiveActivities = YES`. Without this, `Activity.request()` fails and no Live Activity or Dynamic Island appears.

- [ ] **Step 1: Add NSSupportsLiveActivities to iOS Debug build settings**

In `project.pbxproj`, inside the iOS Debug `XCBuildConfiguration` (section starting at line 620, `9A811AEDB402F850CF77C133`), add after the `GENERATE_INFOPLIST_FILE = YES;` line:

```
INFOPLIST_KEY_NSSupportsLiveActivities = YES;
```

- [ ] **Step 2: Add NSSupportsLiveActivities to iOS Release build settings**

In `project.pbxproj`, inside the iOS Release `XCBuildConfiguration` (section starting at line 594, `74301E9EAFCD478E76A7FDEF`), add after the `GENERATE_INFOPLIST_FILE = YES;` line:

```
INFOPLIST_KEY_NSSupportsLiveActivities = YES;
```

- [ ] **Step 3: Build iOS target to verify**

Run: `xcodebuild -project CodePulse.xcodeproj -scheme CodePulse-iOS -destination 'generic/platform=iOS' -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add CodePulse.xcodeproj/project.pbxproj
git commit -m "fix: add NSSupportsLiveActivities to iOS target for Dynamic Island"
```

---

### Task 2: Add CloudKit delete methods

**Files:**
- Modify: `CodePulseShared/Sources/CloudKit/CloudKitManager.swift`

Add two methods: `deleteByIds` for targeted deletion when sessions complete, and `deleteOrphans` for startup cleanup of stale records.

- [ ] **Step 1: Add deleteByIds method**

Add to `CloudKitManager` after `deleteCompleted`:

```swift
/// Delete specific session records by ID (called when sessions complete on macOS)
public func deleteByIds(_ sessionIds: [String]) async throws {
    guard !sessionIds.isEmpty else { return }
    let recordIDs = sessionIds.map { CKRecord.ID(recordName: $0) }
    for id in recordIDs {
        try? await database.deleteRecord(withID: id)
    }
    logger.info("Deleted \(sessionIds.count) session records from CloudKit")
}
```

- [ ] **Step 2: Add deleteOrphans method**

Add to `CloudKitManager` after `deleteByIds`:

```swift
/// Delete all CloudKit records whose sessionId is NOT in the active set.
/// Called once on macOS app launch to clean up stale records from prior crashes.
public func deleteOrphans(activeSessionIds: Set<String>) async throws {
    let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
    let (results, _) = try await database.records(matching: query, resultsLimit: 50)
    var deletedCount = 0
    for (id, result) in results {
        guard case .success = result else { continue }
        if !activeSessionIds.contains(id.recordName) {
            try? await database.deleteRecord(withID: id)
            deletedCount += 1
        }
    }
    if deletedCount > 0 {
        logger.info("Cleaned up \(deletedCount) orphan records from CloudKit")
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add CodePulseShared/Sources/CloudKit/CloudKitManager.swift
git commit -m "feat: add deleteByIds and deleteOrphans to CloudKitManager"
```

---

### Task 3: Sync session deletion to CloudKit

**Files:**
- Modify: `CodePulse-macOS/Services/CloudKitSync.swift`

When `SessionStateManager` marks a session as `.completed` (PID died), CloudKitSync should delete it from CloudKit instead of ignoring it.

- [ ] **Step 1: Track previous session IDs for disappearance detection**

Add a property to `CloudKitSync`:

```swift
private var previousSessionIds: Set<String> = []
```

- [ ] **Step 2: Rewrite syncToCloud to handle deletions**

Replace the `syncToCloud` method with logic that:
1. Finds sessions that disappeared or became `.completed` since last sync → delete from CloudKit
2. Syncs changed active (non-completed) sessions → save to CloudKit

```swift
private func syncToCloud(_ sessions: [SessionState]) {
    guard !isSyncing else { return }

    // Respect quota backoff
    if let backoff = quotaBackoffUntil, Date() < backoff { return }
    quotaBackoffUntil = nil

    let currentIds = Set(sessions.map(\.sessionId))

    // Sessions that disappeared or completed since last sync → delete from CloudKit
    let completedIds = sessions.filter { $0.status == .completed }.map(\.sessionId)
    let disappearedIds = previousSessionIds.subtracting(currentIds)
    let toDelete = Array(Set(completedIds).union(disappearedIds))

    // Active sessions with changed content → save to CloudKit
    let toSave = sessions.filter { session in
        session.status != .completed
        && previousStates[session.sessionId]?.updatedAt != session.updatedAt
    }

    guard !toDelete.isEmpty || !toSave.isEmpty else {
        previousSessionIds = currentIds
        return
    }

    isSyncing = true

    Task {
        defer {
            isSyncing = false
            previousSessionIds = currentIds
        }

        do {
            if !toDelete.isEmpty {
                Self.log("deleteByIds \(toDelete.count) sessions")
                try await CloudKitManager.shared.deleteByIds(toDelete)
                Self.log("SUCCESS: deleted \(toDelete.count)")
                for id in toDelete {
                    previousStates.removeValue(forKey: id)
                }
            }

            if !toSave.isEmpty {
                Self.log("saveBatch \(toSave.count) sessions")
                try await CloudKitManager.shared.saveBatch(toSave)
                Self.log("SUCCESS: saved \(toSave.count)")
                for session in toSave {
                    previousStates[session.sessionId] = session
                }
            }
        } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
            let retryAfter = error.retryAfterSeconds ?? 600
            Self.log("QUOTA: backoff \(Int(retryAfter * 2))s")
            quotaBackoffUntil = Date().addingTimeInterval(retryAfter * 2)
        } catch {
            Self.log("ERROR: \(error)")
            quotaBackoffUntil = Date().addingTimeInterval(300)
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Services/CloudKitSync.swift
git commit -m "fix: sync session deletions to CloudKit when PID dies"
```

---

### Task 4: Orphan cleanup on macOS app launch

**Files:**
- Modify: `CodePulse-macOS/App/CodePulseApp.swift:25-50` (applicationDidFinishLaunching)

On launch, after scanner starts, compare active sessions with CloudKit records and delete orphans. This cleans up any legacy ghost sessions.

- [ ] **Step 1: Add orphan cleanup call after scanner.start()**

Add after `scanner.start()` (line 49) in `applicationDidFinishLaunching`:

```swift
Task {
    do {
        let activeIds = Set(scanner.activeSessions.keys)
        try await CloudKitManager.shared.deleteOrphans(activeSessionIds: activeIds)
    } catch {
        logger.warning("Orphan cleanup failed: \(error.localizedDescription)")
    }
}
```

- [ ] **Step 2: Add CloudKitManager import**

Add at the top of `CodePulseApp.swift`:

```swift
import CloudKit
```

(CloudKitManager is in CodePulseShared which is already imported, but verify `CloudKitManager` is accessible.)

- [ ] **Step 3: Build macOS target to verify**

Run: `xcodebuild -project CodePulse.xcodeproj -scheme CodePulse-macOS -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add CodePulse-macOS/App/CodePulseApp.swift
git commit -m "feat: clean up orphan CloudKit records on macOS app launch"
```

---

### Task 5: Remove dead code

**Files:**
- Modify: `CodePulseShared/Sources/CloudKit/CloudKitManager.swift`

`deleteCompleted(olderThan:)` is now unnecessary — session records are deleted immediately on completion, and orphans are cleaned on launch. Remove it to avoid confusion.

- [ ] **Step 1: Remove deleteCompleted method**

Delete the `deleteCompleted(olderThan:)` method (lines 78-92).

- [ ] **Step 2: Commit**

```bash
git add CodePulseShared/Sources/CloudKit/CloudKitManager.swift
git commit -m "chore: remove unused deleteCompleted method"
```
