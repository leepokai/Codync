# iOS UI Architecture Redesign — Design Spec

**Date:** 2026-03-21
**Status:** Approved
**Approach:** Incremental refactor (方案 A)

## Overview

Restructure the Codync iOS app from a flat `NavigationStack` into a tabbed architecture with dual Live Activity modes (Overall / Individual) and a Primary Session concept.

## Confirmed Requirements

- Bottom `TabView` with three tabs: Claude Code, Cowork (Coming Soon), Codex (Coming Soon)
- Live Activity dual mode: Overall (unified) + Individual (per-session), top Segmented Control to switch
- Overall mode shows max 4 sessions
- Primary Session: hybrid mode — auto-select most active, user can manually lock
- Primary shown on Dynamic Island and Apple Watch (future)
- AppDelegate consolidates all initialization (CloudKit fetch, pins, preferences, notification auth)
- Icons from lobehub/lobe-icons (MIT License), stored in Asset Catalog as SVG

## Section 1: Tab Architecture

### View Hierarchy

```
CodyncIOSApp
└── IOSRootView (retains onboarding gate)
    ├── IOSOnboardingView (first launch)
    └── TabRootView (post-onboarding)
        ├── Tab 1: Claude Code (image: "ClaudeIcon")
        │   └── NavigationStack
        │       ├── IOSSessionListView (reused)
        │       └── IOSSessionDetailView (reused)
        ├── Tab 2: Cowork (systemImage: "person.2.fill") — Coming Soon
        │   └── ComingSoonView
        └── Tab 3: Codex (image: "CodexIcon") — Coming Soon
            └── ComingSoonView
```

### New Files

- `Codync-iOS/Views/TabRootView.swift` — `TabView` container with `@AppStorage("selectedTab")` persistence
- `Codync-iOS/Views/ComingSoonView.swift` — Generic placeholder (icon + title + description)

### Key Decisions

- Each tab has its own `NavigationStack` — independent navigation stacks
- Tab selection persisted via `@AppStorage` — restores last tab on relaunch
- iOS 26 automatically applies liquid glass tab bar effect — no custom styling needed
- `IOSRootView` retains onboarding gate logic, delegates to `TabRootView` after completion

### Tab Enum

```swift
enum AppTab: String, CaseIterable {
    case claudeCode
    case cowork
    case codex
}
```

## Section 2: Live Activity Dual Mode

### Mode Switcher UI

Top of Sessions tab (`IOSSessionListView`):

```
┌──────────────────────────────────┐
│  [Overall]  [Individual]          │  ← Picker, SegmentedPickerStyle
├──────────────────────────────────┤
│  (Overall mode extra controls)    │
│  Primary: ByCrawl ★              │
│  Max sessions: [1][2][3][4]      │
├──────────────────────────────────┤
│  Session list...                 │
└──────────────────────────────────┘
```

- `Picker` with `.segmentedPickerStyle()` in a `Section` header or toolbar area
- Overall-only controls (primary selector, max session count) shown conditionally
- Mode persisted via CloudKit `LiveActivityPreference` record

### Data Model Changes (CodyncShared)

**New: `OverallAttributes`**

```swift
struct OverallAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        let sessions: [SessionSummary]
        let primarySessionId: String?
        let totalCost: Double
    }
}
```

**New: `SessionSummary`**

```swift
struct SessionSummary: Codable, Hashable, Sendable {
    let sessionId: String
    let projectName: String
    let status: SessionStatus
    let model: String
    let currentTask: String?
    let costUSD: Double
}
```

**New: `LiveActivityMode`**

```swift
enum LiveActivityMode: String, Codable, Sendable {
    case overall
    case individual
}
```

Existing `CodyncAttributes` remains unchanged — used for Individual mode.

### LiveActivityManager Extension

```swift
@MainActor
class LiveActivityManager: ObservableObject {
    @Published var mode: LiveActivityMode = .overall

    // Existing (Individual mode)
    var activities: [String: Activity<CodyncAttributes>]
    var trackedSessionIds: Set<String>
    var pinnedSessionIds: Set<String>

    // New (Overall mode)
    var overallActivity: Activity<OverallAttributes>?

    func updateSessions(_ sessions: [SessionState]) {
        switch mode {
        case .overall:    updateOverall(sessions)
        case .individual: updateIndividual(sessions)  // existing logic
        }
    }

    func switchMode(to newMode: LiveActivityMode) {
        // 1. End all current activities (individual or overall)
        // 2. Reset transient state: previousTasks, secondPreviousTasks, grace periods
        // 3. Switch mode
        // 4. Recreate activities in new mode from current session data
    }

    func loadPreference() async {
        // Fetch mode + maxSessions from CloudKit
    }
}
```

### Overall Mode Layout Adaptation (Live Activity Widget)

| Session Count | Layout | Details |
|---|---|---|
| 1 | Full detail | Tool cards + timer + stacking, similar to Individual |
| 2–3 | Medium rows | Status dot + project + model + currentTask |
| 4 | Compact rows | Status dot + project name only |

Dynamic Island always shows primary session only.

### PrimarySessionManager (New)

```swift
@MainActor
final class PrimarySessionManager: ObservableObject {
    @Published var primarySessionId: String?
    @Published var isManuallyLocked: Bool = false

    /// Priority order matches LiveActivityManager.autoFillPriority:
    /// working(5) > needsInput(4) > compacting(3) > idle/error(2) > completed(0)
    func autoSelect(from sessions: [SessionState]) {
        guard !isManuallyLocked else { return }
        primarySessionId = sessions
            .sorted { autoFillPriority($0) > autoFillPriority($1) }
            .first?.sessionId
    }

    private func autoFillPriority(_ s: SessionState) -> Int {
        switch s.status {
        case .working:    5
        case .needsInput: 4
        case .compacting: 3
        case .idle, .error: 2
        case .completed:  0
        }
    }

    func manualLock(_ sessionId: String) {
        primarySessionId = sessionId
        isManuallyLocked = true
    }

    func unlock() {
        isManuallyLocked = false
        // autoSelect will run on next updateSessions
    }

    func load() async {
        // Fetch from CloudKit PrimarySession record
    }

    func save() async {
        // Persist to CloudKit PrimarySession record
    }
}
```

- If manually locked session disappears from CloudKit → auto-unlock and select next
- CloudKit persistence allows macOS to read the primary session choice

## Section 3: CloudKit Changes

### New Record Types

**PrimarySession** (singleton, fixed `recordName: "primary-session"`):

| Field | Type | Description |
|---|---|---|
| sessionId | String | Currently selected primary session |
| isManuallyLocked | Bool | Whether user manually locked this choice |
| updatedAt | Date | Last modification time |

**LiveActivityPreference** (singleton, fixed `recordName: "live-activity-pref"`):

| Field | Type | Description |
|---|---|---|
| mode | String | "overall" or "individual" |
| maxSessions | Int | 1–4, used in Overall mode |

### CloudKitManager Additions

```swift
extension CloudKitManager {
    func fetchPrimarySession() async -> (sessionId: String?, locked: Bool)
    func setPrimarySession(_ sessionId: String, locked: Bool) async
    func clearPrimarySession() async

    func fetchLiveActivityPreference() async -> (mode: LiveActivityMode, maxSessions: Int)
    func setLiveActivityPreference(mode: LiveActivityMode, maxSessions: Int) async
}
```

Both records use fixed `recordName` values to ensure only one instance exists (upsert semantics).

## Section 4: AppDelegate Consolidation

### Before (scattered initialization)

```swift
// AppDelegate: only APNs registration
// CodyncIOSApp .task {}: receiver.start(), loadPinnedSessions(), notification auth
```

### After (unified initialization)

```swift
final class AppDelegate: NSObject, UIApplicationDelegate {
    let receiver = CloudKitReceiver()
    let liveActivityManager = LiveActivityManager()
    let primarySessionManager = PrimarySessionManager()

    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions ...) -> Bool {
        application.registerForRemoteNotifications()
        Task {
            async let startReceiver: () = receiver.start()
            async let loadPins: () = liveActivityManager.loadPinnedSessions()
            async let loadPrimary: () = primarySessionManager.load()
            async let loadPref: () = liveActivityManager.loadPreference()
            _ = await (startReceiver, loadPins, loadPrimary, loadPref)

            liveActivityManager.updateSessions(receiver.sessions)
            primarySessionManager.autoSelect(from: receiver.sessions)

            let center = UNUserNotificationCenter.current()
            try? await center.requestAuthorization(options: [.alert, .sound, .badge])
        }
        return true
    }

    func application(_ application: UIApplication,
                     didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        // Existing push handling + primarySessionManager.autoSelect(...)
    }
}
```

`CodyncIOSApp` simplified — no `.task` blocks, just passes dependencies to `IOSRootView`.

## File Change Summary

### New Files (6)

| File | Description |
|---|---|
| `Codync-iOS/Views/TabRootView.swift` | TabView container |
| `Codync-iOS/Views/ComingSoonView.swift` | Coming Soon placeholder |
| `Codync-iOS/Services/PrimarySessionManager.swift` | Primary session logic |
| `CodyncShared/Sources/Models/OverallAttributes.swift` | Overall Activity data model |
| `CodyncShared/Sources/Models/SessionSummary.swift` | Session summary model |
| `CodyncShared/Sources/Models/LiveActivityMode.swift` | Mode enum |

### Modified Files (7)

| File | Changes |
|---|---|
| `Codync-iOS/App/AppDelegate.swift` | Consolidate init, add primarySessionManager |
| `Codync-iOS/App/CodyncIOSApp.swift` | Remove .task blocks, pass new dependencies |
| `Codync-iOS/Views/IOSRootView.swift` | Show TabRootView post-onboarding |
| `Codync-iOS/Views/IOSSessionListView.swift` | Mode switcher + primary selection UI |
| `Codync-iOS/Services/LiveActivityManager.swift` | Dual mode support |
| `CodyncShared/Sources/CloudKit/CloudKitManager.swift` | Primary + preference methods |
| `CodyncLiveActivity/CodyncLiveActivityWidget.swift` | Overall mode layout |

### Already Added (Assets)

| Asset | File | Rendering |
|---|---|---|
| `ClaudeIcon` | claude.svg | template (follows tint) |
| `ClaudeColorIcon` | claude-color.svg | original (#D97757) |
| `CodexIcon` | codex.svg | template (follows tint) |
| `CodexColorIcon` | codex-color.svg | original (gradient) |

### Unchanged Files

- `IOSSessionDetailView.swift`
- `IOSStatusIndicator.swift`
- `IOSTheme.swift`
- `IOSSessionTagView.swift`
- `IOSOnboardingView.swift`
- `CKRecordMapper.swift`
- `SessionState.swift`, `SessionStatus.swift`, `WaitingReason.swift`, `TaskItem.swift`
- `CodyncAttributes.swift` (retained for Individual mode)

## Implementation Priority

1. Tab architecture (TabRootView + ComingSoonView + IOSRootView modification)
2. AppDelegate consolidation (unified init, remove .task blocks)
3. Data models (OverallAttributes, SessionSummary, LiveActivityMode)
4. PrimarySessionManager + CloudKit records
5. LiveActivityManager dual mode
6. Mode switcher UI in IOSSessionListView
7. Overall Live Activity widget layout
8. Primary selection UI (manual lock/unlock)
