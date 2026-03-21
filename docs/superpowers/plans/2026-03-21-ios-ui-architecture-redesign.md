# iOS UI Architecture Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the Codync iOS app into a tabbed architecture with dual Live Activity modes and Primary Session support.

**Architecture:** Incremental refactor — wrap existing views in TabView, extend LiveActivityManager with dual mode, add PrimarySessionManager as new service. All changes are additive; existing Individual mode logic preserved.

**Tech Stack:** Swift 6 (strict concurrency), SwiftUI, ActivityKit, CloudKit, CodyncShared package

**Spec:** `docs/superpowers/specs/2026-03-21-ios-ui-architecture-redesign.md`

---

## Task 1: Shared Data Models

**Files:**
- Create: `CodyncShared/Sources/Models/LiveActivityMode.swift`
- Create: `CodyncShared/Sources/Models/SessionSummary.swift`
- Create: `CodyncShared/Sources/Models/OverallAttributes.swift`

These models are used by both the iOS app and the Live Activity widget extension. They must be in `CodyncShared` so both targets can import them.

- [ ] **Step 1: Create LiveActivityMode enum**

Create `CodyncShared/Sources/Models/LiveActivityMode.swift`:

```swift
import Foundation

public enum LiveActivityMode: String, Codable, Sendable {
    case overall
    case individual
}
```

- [ ] **Step 2: Create SessionSummary model**

Create `CodyncShared/Sources/Models/SessionSummary.swift`:

```swift
import Foundation

public struct SessionSummary: Codable, Hashable, Sendable {
    public let sessionId: String
    public let projectName: String
    public let status: SessionStatus
    public let model: String
    public let currentTask: String?
    public let costUSD: Double

    public init(sessionId: String, projectName: String, status: SessionStatus,
                model: String, currentTask: String?, costUSD: Double) {
        self.sessionId = sessionId
        self.projectName = projectName
        self.status = status
        self.model = model
        self.currentTask = currentTask
        self.costUSD = costUSD
    }
}
```

- [ ] **Step 3: Create OverallAttributes**

Create `CodyncShared/Sources/Models/OverallAttributes.swift`:

```swift
import Foundation

#if os(iOS)
import ActivityKit

public struct OverallAttributes: ActivityAttributes, Codable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let sessions: [SessionSummary]
        public let primarySessionId: String?
        public let totalCost: Double

        public init(sessions: [SessionSummary], primarySessionId: String?, totalCost: Double) {
            self.sessions = sessions
            self.primarySessionId = primarySessionId
            self.totalCost = totalCost
        }
    }

    public init() {}
}

#else

public struct OverallAttributes: Codable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let sessions: [SessionSummary]
        public let primarySessionId: String?
        public let totalCost: Double

        public init(sessions: [SessionSummary], primarySessionId: String?, totalCost: Double) {
            self.sessions = sessions
            self.primarySessionId = primarySessionId
            self.totalCost = totalCost
        }
    }

    public init() {}
}

#endif
```

Note: Uses the same `#if os(iOS)` pattern as `CodyncAttributes.swift` — `ActivityAttributes` conformance on iOS only, plain `Codable` on macOS.

- [ ] **Step 4: Build to verify compilation**

Run: `xcodebuild -scheme CodyncShared -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 5: Commit**

```bash
git add CodyncShared/Sources/Models/LiveActivityMode.swift \
       CodyncShared/Sources/Models/SessionSummary.swift \
       CodyncShared/Sources/Models/OverallAttributes.swift
git commit -m "feat: add shared models for dual Live Activity mode"
```

---

## Task 2: CloudKit — Primary Session & Live Activity Preference

**Files:**
- Modify: `CodyncShared/Sources/CloudKit/CloudKitManager.swift`

Add methods to manage two new singleton CloudKit records: `PrimarySession` and `LiveActivityPreference`. Both use fixed `recordName` values for upsert semantics.

- [ ] **Step 1: Add PrimarySession record methods**

Add to `CloudKitManager.swift` after the existing `// MARK: - Pinned Sessions` section:

```swift
// MARK: - Primary Session

static let primarySessionRecordType = "PrimarySession"
private static let primarySessionRecordName = "primary-session"

public func fetchPrimarySession() async -> (sessionId: String?, locked: Bool) {
    let recordID = CKRecord.ID(recordName: Self.primarySessionRecordName, zoneID: Self.zoneID)
    do {
        let record = try await database.record(for: recordID)
        let sessionId = record["sessionId"] as? String
        let locked = (record["isManuallyLocked"] as? Int64 ?? 0) == 1
        return (sessionId, locked)
    } catch {
        return (nil, false)
    }
}

public func setPrimarySession(_ sessionId: String, locked: Bool) async {
    let recordID = CKRecord.ID(recordName: Self.primarySessionRecordName, zoneID: Self.zoneID)
    let record: CKRecord
    if let existing = try? await database.record(for: recordID) {
        record = existing
    } else {
        record = CKRecord(recordType: Self.primarySessionRecordType, recordID: recordID)
    }
    record["sessionId"] = sessionId as CKRecordValue
    record["isManuallyLocked"] = (locked ? 1 : 0) as CKRecordValue
    record["updatedAt"] = Date() as CKRecordValue
    _ = try? await database.save(record)
}

public func clearPrimarySession() async {
    let recordID = CKRecord.ID(recordName: Self.primarySessionRecordName, zoneID: Self.zoneID)
    try? await database.deleteRecord(withID: recordID)
}
```

- [ ] **Step 2: Add LiveActivityPreference record methods**

Add after the Primary Session section:

```swift
// MARK: - Live Activity Preference

static let liveActivityPrefRecordType = "LiveActivityPreference"
private static let liveActivityPrefRecordName = "live-activity-pref"

public func fetchLiveActivityPreference() async -> (mode: LiveActivityMode, maxSessions: Int) {
    let recordID = CKRecord.ID(recordName: Self.liveActivityPrefRecordName, zoneID: Self.zoneID)
    do {
        let record = try await database.record(for: recordID)
        let modeStr = record["mode"] as? String ?? "overall"
        let mode = LiveActivityMode(rawValue: modeStr) ?? .overall
        let maxSessions = record["maxSessions"] as? Int64 ?? 4
        return (mode, Int(maxSessions))
    } catch {
        return (.overall, 4)
    }
}

public func setLiveActivityPreference(mode: LiveActivityMode, maxSessions: Int) async {
    let recordID = CKRecord.ID(recordName: Self.liveActivityPrefRecordName, zoneID: Self.zoneID)
    let record: CKRecord
    if let existing = try? await database.record(for: recordID) {
        record = existing
    } else {
        record = CKRecord(recordType: Self.liveActivityPrefRecordType, recordID: recordID)
    }
    record["mode"] = mode.rawValue as CKRecordValue
    record["maxSessions"] = maxSessions as CKRecordValue
    _ = try? await database.save(record)
}
```

- [ ] **Step 3: Add import for LiveActivityMode**

The file already imports `Foundation` and `CloudKit`. `LiveActivityMode` is in the same `CodyncShared` module, so no extra import needed. Verify it compiles:

Run: `xcodebuild -scheme CodyncShared -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 4: Commit**

```bash
git add CodyncShared/Sources/CloudKit/CloudKitManager.swift
git commit -m "feat: add CloudKit methods for PrimarySession and LiveActivityPreference"
```

---

## Task 3: PrimarySessionManager

**Files:**
- Create: `Codync-iOS/Services/PrimarySessionManager.swift`

New `@MainActor` service that manages which session is "primary" for Dynamic Island focus. Supports auto-selection (most active session) and manual lock.

- [ ] **Step 1: Create PrimarySessionManager**

Create `Codync-iOS/Services/PrimarySessionManager.swift`:

```swift
import Foundation
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "PrimarySession")

@MainActor
final class PrimarySessionManager: ObservableObject {
    @Published var primarySessionId: String?
    @Published var isManuallyLocked: Bool = false

    func autoSelect(from sessions: [SessionState]) {
        guard !isManuallyLocked else {
            // If locked session no longer exists, unlock
            if let lockedId = primarySessionId,
               !sessions.contains(where: { $0.sessionId == lockedId }) {
                logger.info("Locked primary session \(lockedId) no longer exists, unlocking")
                isManuallyLocked = false
                // Fall through to auto-select
            } else {
                return
            }
        }
        let best = sessions
            .sorted { autoFillPriority($0) > autoFillPriority($1) }
            .first
        if primarySessionId != best?.sessionId {
            primarySessionId = best?.sessionId
            if let id = primarySessionId {
                logger.info("Auto-selected primary: \(id)")
            }
        }
    }

    func manualLock(_ sessionId: String) {
        primarySessionId = sessionId
        isManuallyLocked = true
        logger.info("Manually locked primary: \(sessionId)")
        Task { await save() }
    }

    func unlock() {
        isManuallyLocked = false
        logger.info("Unlocked primary session")
        Task { await save() }
    }

    func load() async {
        let result = await CloudKitManager.shared.fetchPrimarySession()
        primarySessionId = result.sessionId
        isManuallyLocked = result.locked
        if let id = result.sessionId {
            logger.info("Loaded primary: \(id), locked: \(result.locked)")
        }
    }

    func save() async {
        if let id = primarySessionId {
            await CloudKitManager.shared.setPrimarySession(id, locked: isManuallyLocked)
        } else {
            await CloudKitManager.shared.clearPrimarySession()
        }
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
}
```

- [ ] **Step 2: Build to verify**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 3: Commit**

```bash
git add Codync-iOS/Services/PrimarySessionManager.swift
git commit -m "feat: add PrimarySessionManager with auto-select and manual lock"
```

---

## Task 4: Tab Architecture — TabRootView & ComingSoonView

**Files:**
- Create: `Codync-iOS/Views/TabRootView.swift`
- Create: `Codync-iOS/Views/ComingSoonView.swift`
- Modify: `Codync-iOS/Views/IOSRootView.swift`

- [ ] **Step 1: Create ComingSoonView**

Create `Codync-iOS/Views/ComingSoonView.swift`:

```swift
import SwiftUI

struct ComingSoonView: View {
    let icon: String
    let isSystemImage: Bool
    let title: String
    let description: String
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 16) {
            if isSystemImage {
                Image(systemName: icon)
                    .font(.system(size: 48))
                    .foregroundStyle(theme.tertiaryText)
            } else {
                Image(icon)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 48, height: 48)
                    .foregroundStyle(theme.tertiaryText)
            }
            Text(title)
                .font(.title2.bold())
                .foregroundStyle(theme.primaryText)
            Text(description)
                .font(.subheadline)
                .foregroundStyle(theme.secondaryText)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
            Text("Coming Soon")
                .font(.caption.bold())
                .foregroundStyle(theme.tertiaryText)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(theme.tertiaryText.opacity(0.1), in: Capsule())
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.background)
    }
}
```

- [ ] **Step 2: Create TabRootView**

Create `Codync-iOS/Views/TabRootView.swift`:

```swift
import SwiftUI
import CodyncShared

enum AppTab: String, CaseIterable {
    case claudeCode
    case cowork
    case codex
}

struct TabRootView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @AppStorage("codync_selectedTab") private var selectedTab: String = AppTab.claudeCode.rawValue

    var body: some View {
        TabView(selection: $selectedTab) {
            Tab("Claude Code", image: "ClaudeIcon", value: AppTab.claudeCode.rawValue) {
                NavigationStack {
                    IOSSessionListView(
                        sessions: sessions,
                        liveActivityManager: liveActivityManager
                    )
                }
            }

            Tab("Cowork", systemImage: "person.2.fill", value: AppTab.cowork.rawValue) {
                ComingSoonView(
                    icon: "person.2.fill",
                    isSystemImage: true,
                    title: "Cowork",
                    description: "Monitor Claude Cowork sessions in real time"
                )
            }

            Tab("Codex", image: "CodexIcon", value: AppTab.codex.rawValue) {
                ComingSoonView(
                    icon: "CodexColorIcon",
                    isSystemImage: false,
                    title: "Codex",
                    description: "Track OpenAI Codex jobs and costs"
                )
            }
        }
    }
}
```

- [ ] **Step 3: Modify IOSRootView to use TabRootView**

In `Codync-iOS/Views/IOSRootView.swift`, the view currently wraps everything in a `NavigationStack` and shows `IOSSessionListView` directly. Replace it so the `NavigationStack` is inside each tab, and use `TabRootView` post-onboarding.

Replace the entire content of `IOSRootView.swift` with:

```swift
import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @AppStorage("codync_darkMode") private var isDarkMode = true
    @State private var displayedSessions: [SessionState] = []
    @State private var reorderTimer: Timer?

    var body: some View {
        Group {
            if !onboardingComplete {
                NavigationStack {
                    IOSOnboardingView()
                }
            } else {
                TabRootView(
                    sessions: displayedSessions,
                    liveActivityManager: liveActivityManager,
                    primarySessionManager: primarySessionManager
                )
            }
        }
        .environment(\.theme, CodyncTheme(isDark: isDarkMode))
        .preferredColorScheme(isDarkMode ? .dark : .light)
        .onChange(of: receiver.sessions) { _, sessions in
            if !sessions.isEmpty {
                onboardingComplete = true
            }
            liveActivityManager.updateSessions(sessions)
            primarySessionManager.autoSelect(from: sessions)
        }
        .task {
            displayedSessions = sortSessions(receiver.sessions)
            reorderTimer?.invalidate()
            reorderTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { _ in
                Task { @MainActor in
                    let sorted = sortSessions(receiver.sessions)
                    withAnimation(.spring(duration: 2.0, bounce: 0.1)) {
                        displayedSessions = sorted
                    }
                }
            }
        }
        .onDisappear {
            reorderTimer?.invalidate()
            reorderTimer = nil
        }
    }

    private func sortSessions(_ sessions: [SessionState]) -> [SessionState] {
        sessions.sorted { a, b in
            let aWeight = a.status == .working ? 0 : a.status == .needsInput ? 1 : 2
            let bWeight = b.status == .working ? 0 : b.status == .needsInput ? 1 : 2
            if aWeight != bWeight { return aWeight < bWeight }
            return a.updatedAt > b.updatedAt
        }
    }
}
```

Key changes:
- Accepts `primarySessionManager` parameter
- Removed outer `NavigationStack` (each tab has its own)
- Onboarding view gets its own `NavigationStack`
- Added `primarySessionManager.autoSelect(from:)` in `onChange`
- Passes `primarySessionManager` to `TabRootView`

- [ ] **Step 4: Build to verify**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -5`

This will fail because `CodyncIOSApp` and `AppDelegate` don't pass `primarySessionManager` yet. That's expected — Task 5 fixes this.

- [ ] **Step 5: Commit**

```bash
git add Codync-iOS/Views/TabRootView.swift \
       Codync-iOS/Views/ComingSoonView.swift \
       Codync-iOS/Views/IOSRootView.swift
git commit -m "feat: add TabRootView with Claude Code, Cowork, Codex tabs"
```

---

## Task 5: AppDelegate Consolidation

**Files:**
- Modify: `Codync-iOS/App/AppDelegate.swift`
- Modify: `Codync-iOS/App/CodyncIOSApp.swift`

Move all initialization from SwiftUI `.task` blocks into AppDelegate, add `primarySessionManager`.

- [ ] **Step 1: Update AppDelegate**

Replace `Codync-iOS/App/AppDelegate.swift` with:

```swift
import UIKit
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "AppDelegate")

final class AppDelegate: NSObject, UIApplicationDelegate {
    let receiver = CloudKitReceiver()
    let liveActivityManager = LiveActivityManager()
    let primarySessionManager = PrimarySessionManager()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        application.registerForRemoteNotifications()
        Task {
            async let startReceiver: () = receiver.start()
            async let loadPins: () = liveActivityManager.loadPinnedSessions()
            async let loadPrimary: () = primarySessionManager.load()
            _ = await (startReceiver, loadPins, loadPrimary)

            liveActivityManager.updateSessions(receiver.sessions)
            primarySessionManager.autoSelect(from: receiver.sessions)

            let center = UNUserNotificationCenter.current()
            try? await center.requestAuthorization(options: [.alert, .sound, .badge])
        }
        return true
    }

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        let hex = deviceToken.prefix(4).map { String(format: "%02x", $0) }.joined()
        logger.info("APNs device token registered: \(hex)...")
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        logger.error("APNs registration failed: \(error.localizedDescription)")
    }

    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        let notification = CKNotification(fromRemoteNotificationDictionary: userInfo)
        let subID = notification?.subscriptionID
        guard subID == "session-zone-changes" || subID == "session-changes" else {
            return .noData
        }
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        let ts = fmt.string(from: Date())
        logger.info("[\(ts)] CloudKit push received")
        await receiver.fetch(source: "silent-push")
        let statuses = receiver.sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
        logger.info("[\(ts)] → \(statuses)")
        liveActivityManager.updateSessions(receiver.sessions)
        primarySessionManager.autoSelect(from: receiver.sessions)
        return .newData
    }
}
```

Changes from current:
- Added `primarySessionManager` property
- `didFinishLaunchingWithOptions` now runs parallel init: `receiver.start()`, `loadPinnedSessions()`, `primarySessionManager.load()`
- After init, calls `updateSessions` and `autoSelect`
- Requests notification auth
- `didReceiveRemoteNotification` also calls `primarySessionManager.autoSelect`
- Added `import UserNotifications` (implicit via UIKit, but explicit is cleaner — actually UIKit already provides it)

- [ ] **Step 2: Update CodyncIOSApp**

Replace `Codync-iOS/App/CodyncIOSApp.swift` with:

```swift
import SwiftUI
import CodyncShared

@main
struct CodyncIOSApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            IOSRootView(
                receiver: appDelegate.receiver,
                liveActivityManager: appDelegate.liveActivityManager,
                primarySessionManager: appDelegate.primarySessionManager
            )
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                Task {
                    await appDelegate.receiver.fetch(source: "foreground-return")
                    appDelegate.liveActivityManager.updateSessions(appDelegate.receiver.sessions)
                    appDelegate.primarySessionManager.autoSelect(from: appDelegate.receiver.sessions)
                }
            }
        }
    }
}
```

All three `.task` blocks removed — init is now in AppDelegate. The `scenePhase` foreground-return handler is preserved and extended with `primarySessionManager.autoSelect`.

- [ ] **Step 3: Build to verify**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 4: Commit**

```bash
git add Codync-iOS/App/AppDelegate.swift \
       Codync-iOS/App/CodyncIOSApp.swift
git commit -m "refactor: consolidate init into AppDelegate, add PrimarySessionManager"
```

---

## Task 6: LiveActivityManager — Dual Mode Support

**Files:**
- Modify: `Codync-iOS/Services/LiveActivityManager.swift`

Extend the existing manager to support Overall mode alongside the existing Individual mode.

- [ ] **Step 1: Add mode property and Overall activity tracking**

At the top of `LiveActivityManager` class, after existing property declarations, add:

```swift
@Published var mode: LiveActivityMode = .overall
@Published var maxOverallSessions: Int = 4
private var overallActivity: Activity<OverallAttributes>?
private var lastOverallState: OverallAttributes.ContentState?
private var overallTrackedIds: Set<String> = []  // tracks sessions seen in Overall mode for completion notifications
```

- [ ] **Step 2: Add loadPreference method**

Add after `loadPinnedSessions()`:

```swift
func loadPreference() async {
    let pref = await CloudKitManager.shared.fetchLiveActivityPreference()
    mode = pref.mode
    maxOverallSessions = pref.maxSessions
}

func savePreference() async {
    await CloudKitManager.shared.setLiveActivityPreference(mode: mode, maxSessions: maxOverallSessions)
}
```

- [ ] **Step 3: Modify updateSessions to route by mode**

Rename the existing `updateSessions` body into `updateIndividual`, then create a new `updateSessions` that routes:

```swift
func updateSessions(_ sessions: [SessionState]) {
    let statuses = sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
    logger.debug("updateSessions (\(self.mode.rawValue)): \(statuses)")
    sessionsByID = Dictionary(sessions.map { ($0.sessionId, $0) }, uniquingKeysWith: { _, last in last })

    switch mode {
    case .individual: updateIndividual(sessions)
    case .overall:    updateOverall(sessions)
    }
}
```

Move the existing `updateSessions` logic (cleanup, grace timers, desired set, start/stop tracking) into a new `private func updateIndividual(_ sessions: [SessionState])`.

- [ ] **Step 4: Implement updateOverall**

Add the Overall mode logic:

```swift
private func updateOverall(_ sessions: [SessionState]) {
    let active = sessions
        .filter { $0.status != .completed }
        .sorted { autoFillPriority($0) < autoFillPriority($1) }
        .prefix(maxOverallSessions)

    let summaries = active.map { session in
        SessionSummary(
            sessionId: session.sessionId,
            projectName: session.projectName,
            status: session.status,
            model: session.model,
            currentTask: session.currentTask,
            costUSD: session.costUSD
        )
    }

    let totalCost = sessions.reduce(0) { $0 + $1.costUSD }

    // Find primary (from PrimarySessionManager, passed via caller)
    // For now, use first working session as primary
    let primaryId = active.first { $0.status == .working }?.sessionId
        ?? active.first?.sessionId

    let state = OverallAttributes.ContentState(
        sessions: summaries,
        primarySessionId: primaryId,
        totalCost: totalCost
    )

    guard state != lastOverallState else { return }
    lastOverallState = state

    if overallActivity == nil {
        guard UIApplication.shared.applicationState == .active else { return }
        do {
            overallActivity = try Activity.request(
                attributes: OverallAttributes(),
                content: .init(state: state, staleDate: nil)
            )
            logger.info("Started Overall Live Activity with \(summaries.count) sessions")
        } catch {
            logger.error("Failed to start Overall Live Activity: \(error)")
        }
    } else {
        Task {
            await overallActivity?.update(.init(state: state, staleDate: nil, relevanceScore: 100))
        }
    }

    // Track sessions seen in Overall mode
    for session in active {
        overallTrackedIds.insert(session.sessionId)
    }

    // Send completion notifications for newly completed sessions
    for session in sessions where session.status == .completed {
        if overallTrackedIds.contains(session.sessionId) {
            sendCompletionNotification(session)
            overallTrackedIds.remove(session.sessionId)
        }
    }
}
```

- [ ] **Step 5: Add switchMode method**

```swift
func switchMode(to newMode: LiveActivityMode) {
    guard newMode != mode else { return }
    logger.info("Switching Live Activity mode: \(self.mode.rawValue) → \(newMode.rawValue)")

    // End all current activities
    if mode == .individual {
        for sessionId in Array(trackedSessionIds) {
            stopTracking(sessionId: sessionId)
        }
    } else {
        if let activity = overallActivity {
            Task { await activity.end(nil, dismissalPolicy: .immediate) }
            overallActivity = nil
            lastOverallState = nil
        }
    }

    // Reset transient state
    previousTasks.removeAll()
    secondPreviousTasks.removeAll()
    graceTimers.removeAll()
    lastPushedState.removeAll()
    overallTrackedIds.removeAll()

    mode = newMode
    Task { await savePreference() }

    // Recreate with current data
    updateSessions(Array(sessionsByID.values))
}
```

- [ ] **Step 6: Update AppDelegate to load preference**

In `Codync-iOS/App/AppDelegate.swift`, add `loadPreference` to the parallel init:

```swift
async let startReceiver: () = receiver.start()
async let loadPins: () = liveActivityManager.loadPinnedSessions()
async let loadPrimary: () = primarySessionManager.load()
async let loadPref: () = liveActivityManager.loadPreference()
_ = await (startReceiver, loadPins, loadPrimary, loadPref)
```

- [ ] **Step 7: Build to verify**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 8: Commit**

```bash
git add Codync-iOS/Services/LiveActivityManager.swift \
       Codync-iOS/App/AppDelegate.swift
git commit -m "feat: add dual mode support to LiveActivityManager (Overall + Individual)"
```

---

## Task 7: Mode Switcher UI in SessionListView

**Files:**
- Modify: `Codync-iOS/Views/IOSSessionListView.swift`

Add a Segmented Control at the top of the session list for switching between Overall and Individual modes, plus primary session selection controls in Overall mode.

- [ ] **Step 1: Add mode switcher and primary controls**

Modify `IOSSessionListView` to accept additional dependencies and add the mode/primary UI:

```swift
struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 1) {
                // Mode switcher
                modeSection

                // Primary session controls (Overall mode only)
                if liveActivityManager.mode == .overall {
                    primarySection
                }

                // Session list
                ForEach(sessions) { session in
                    NavigationLink(destination: IOSSessionDetailView(session: session)) {
                        SessionRowContent(
                            session: session,
                            isTracking: liveActivityManager.isTracking(sessionId: session.sessionId),
                            isPinned: liveActivityManager.isPinned(sessionId: session.sessionId),
                            isPrimary: primarySessionManager.primarySessionId == session.sessionId,
                            showPrimary: liveActivityManager.mode == .overall,
                            onTogglePin: { liveActivityManager.togglePin(session.sessionId) },
                            onSetPrimary: { primarySessionManager.manualLock(session.sessionId) }
                        )
                    }
                    .buttonStyle(.plain)
                    .tint(theme.primaryText)
                }
            }
        }
        .background(theme.background)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Image("CodyncIcon")
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 22, height: 22)
                    .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
            }
        }
        .toolbarColorScheme(.dark, for: .navigationBar)
    }

    @ViewBuilder
    private var modeSection: some View {
        Picker("Live Activity Mode", selection: Binding(
            get: { liveActivityManager.mode },
            set: { liveActivityManager.switchMode(to: $0) }
        )) {
            Text("Overall").tag(LiveActivityMode.overall)
            Text("Individual").tag(LiveActivityMode.individual)
        }
        .pickerStyle(.segmented)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var primarySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Max sessions picker
            HStack {
                Text("Max Sessions")
                    .font(.caption.bold())
                    .foregroundStyle(theme.secondaryText)
                Spacer()
                Picker("Max", selection: Binding(
                    get: { liveActivityManager.maxOverallSessions },
                    set: { newVal in
                        liveActivityManager.maxOverallSessions = newVal
                        Task { await liveActivityManager.savePreference() }
                    }
                )) {
                    ForEach(1...4, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 160)
            }

            // Primary session
            HStack {
                Text("Primary Session")
                    .font(.caption.bold())
                    .foregroundStyle(theme.secondaryText)
                Spacer()
                if primarySessionManager.isManuallyLocked {
                    Button("Unlock") {
                        primarySessionManager.unlock()
                        primarySessionManager.autoSelect(from: sessions)
                    }
                    .font(.caption)
                }
            }
            if let primaryId = primarySessionManager.primarySessionId,
               let session = sessions.first(where: { $0.sessionId == primaryId }) {
                HStack(spacing: 6) {
                    Image(systemName: "star.fill")
                        .font(.system(size: 10))
                        .foregroundStyle(.yellow)
                    Text(session.projectName)
                        .font(.subheadline.bold())
                        .foregroundStyle(theme.primaryText)
                    if primarySessionManager.isManuallyLocked {
                        Image(systemName: "lock.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(theme.tertiaryText)
                    }
                }
            } else {
                Text("No active session")
                    .font(.subheadline)
                    .foregroundStyle(theme.tertiaryText)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}
```

- [ ] **Step 2: Update SessionRowContent for primary indicator**

Add `isPrimary`, `showPrimary`, and `onSetPrimary` parameters to `SessionRowContent`:

```swift
private struct SessionRowContent: View {
    let session: SessionState
    let isTracking: Bool
    let isPinned: Bool
    let isPrimary: Bool
    let showPrimary: Bool
    let onTogglePin: () -> Void
    let onSetPrimary: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 10) {
            SessionStatusView(
                status: session.status,
                completedTasks: session.completedTaskCount,
                totalTasks: session.totalTaskCount,
                waitingReason: session.waitingReason
            )

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    if showPrimary && isPrimary {
                        Image(systemName: "star.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(.yellow)
                    }
                    Text(session.projectName)
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                        .lineLimit(1)
                    if !session.model.isEmpty {
                        SessionTagView(tag: session.model)
                    }
                    Spacer(minLength: 4)
                }
                if session.statusDescription != nil || !session.tasks.isEmpty {
                    HStack(spacing: 4) {
                        if let desc = session.statusDescription {
                            Text(desc)
                                .font(.system(size: 13))
                                .foregroundStyle(subtitleColor)
                                .lineLimit(1)
                        }
                        Spacer()
                        Text(relativeTime(session.updatedAt))
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(theme.tertiaryText)
                    }
                }
                if !session.tasks.isEmpty {
                    MiniProgressBar(tasks: session.tasks)
                }
            }

            if showPrimary {
                Button(action: onSetPrimary) {
                    Image(systemName: isPrimary ? "star.fill" : "star")
                        .font(.system(size: 13))
                        .foregroundStyle(isPrimary ? .yellow : theme.tertiaryText)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            } else {
                Button(action: onTogglePin) {
                    Image(systemName: isPinned ? "pin.fill" : "pin")
                        .font(.system(size: 13))
                        .foregroundStyle(isPinned ? theme.primaryText : theme.tertiaryText)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    // ... subtitleColor and relativeTime unchanged
```

- [ ] **Step 3: Update TabRootView to pass primarySessionManager**

In `TabRootView.swift`, update the Claude Code tab to pass `primarySessionManager`:

```swift
Tab("Claude Code", image: "ClaudeIcon", value: AppTab.claudeCode.rawValue) {
    NavigationStack {
        IOSSessionListView(
            sessions: sessions,
            liveActivityManager: liveActivityManager,
            primarySessionManager: primarySessionManager
        )
    }
}
```

- [ ] **Step 4: Build to verify**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 5: Commit**

```bash
git add Codync-iOS/Views/IOSSessionListView.swift \
       Codync-iOS/Views/TabRootView.swift
git commit -m "feat: add mode switcher and primary session UI to session list"
```

---

## Task 8: Overall Live Activity Widget Layout

**Files:**
- Modify: `CodyncLiveActivity/CodyncLiveActivityWidget.swift`

Add a new widget configuration for `OverallAttributes` that adapts its layout based on session count.

- [ ] **Step 1: Create OverallLiveActivityWidget**

Add a new widget struct at the bottom of `CodyncLiveActivityWidget.swift` (before the closing of the file):

```swift
// MARK: - Overall Live Activity Widget

struct OverallLiveActivityWidget: Widget {
    let kind: String = "CodyncOverallLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: OverallAttributes.self) { context in
            overallLockScreen(context: context)
        } dynamicIsland: { context in
            // Dynamic Island shows primary session only
            let primary = context.state.sessions.first { $0.sessionId == context.state.primarySessionId }
                ?? context.state.sessions.first
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    if let p = primary, p.status == .working {
                        Circle().fill(.white).frame(width: 8, height: 8)
                    } else {
                        Circle().fill(.white.opacity(0.5)).frame(width: 8, height: 8)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(primary?.projectName ?? "Codync")
                            .font(.headline)
                        Text(primary?.currentTask ?? statusLabel(primary?.status.rawValue ?? "idle"))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(String(format: "$%.2f", context.state.totalCost))
                        .font(.caption2.bold())
                        .foregroundStyle(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(spacing: 4) {
                        ForEach(context.state.sessions, id: \.sessionId) { s in
                            HStack(spacing: 3) {
                                Circle()
                                    .fill(s.sessionId == context.state.primarySessionId ? .white : .white.opacity(0.5))
                                    .frame(width: 5, height: 5)
                                Text(s.projectName)
                                    .font(.caption2)
                                    .lineLimit(1)
                            }
                        }
                    }
                }
            } compactLeading: {
                Circle().fill(.white).frame(width: 6, height: 6)
            } compactTrailing: {
                Text("\(context.state.sessions.count)")
                    .font(.caption2.bold())
            } minimal: {
                Circle().fill(.white).frame(width: 6, height: 6)
            }
        }
    }

    @ViewBuilder
    private func overallLockScreen(context: ActivityViewContext<OverallAttributes>) -> some View {
        let sessions = context.state.sessions
        let primaryId = context.state.primarySessionId

        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(widgetFg.opacity(0.6))
                        .frame(width: 21, height: 21)
                        .background(widgetFg.opacity(0.08), in: Circle())
                    Text("Codync")
                        .font(.callout.bold())
                        .opacity(0.72)
                }
                Spacer()
                Text(String(format: "$%.2f", context.state.totalCost))
                    .font(.subheadline)
                    .monospacedDigit()
                    .opacity(0.5)
            }
            .foregroundStyle(widgetFg)
            .padding(.horizontal, 6)
            .frame(height: 28)

            // Session rows — layout adapts by count
            VStack(spacing: 2) {
                ForEach(sessions, id: \.sessionId) { session in
                    let isPrimary = session.sessionId == primaryId
                    if sessions.count <= 3 {
                        mediumRow(session: session, isPrimary: isPrimary)
                    } else {
                        compactRow(session: session, isPrimary: isPrimary)
                    }
                }
            }
            .frame(maxHeight: .infinity)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(0.75))
        .activityBackgroundTint(.clear)
    }

    @ViewBuilder
    private func mediumRow(session: SessionSummary, isPrimary: Bool) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(isPrimary ? widgetFg : widgetFg.opacity(0.4))
                .frame(width: 6, height: 6)
            Text(session.projectName)
                .font(.subheadline.bold())
                .foregroundStyle(widgetFg)
                .lineLimit(1)
            Text(session.model)
                .font(.caption2)
                .foregroundStyle(widgetFg.opacity(0.4))
            Spacer()
            if let task = session.currentTask, !task.isEmpty {
                Text(task)
                    .font(.caption2)
                    .foregroundStyle(widgetFg.opacity(0.6))
                    .lineLimit(1)
                    .frame(maxWidth: 120, alignment: .trailing)
            } else {
                Text(statusLabel(session.status.rawValue))
                    .font(.caption2)
                    .foregroundStyle(widgetFg.opacity(0.4))
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private func compactRow(session: SessionSummary, isPrimary: Bool) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(isPrimary ? widgetFg : widgetFg.opacity(0.4))
                .frame(width: 5, height: 5)
            Text(session.projectName)
                .font(.caption.bold())
                .foregroundStyle(widgetFg)
                .lineLimit(1)
            Spacer()
            Text(statusLabel(session.status.rawValue))
                .font(.caption2)
                .foregroundStyle(widgetFg.opacity(0.4))
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 4)
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "working": "Working"
        case "idle": "Idle"
        case "needsInput": "Needs Input"
        case "compacting": "Compacting"
        case "error": "Error"
        case "completed": "Complete"
        default: "Working"
        }
    }
}
```

- [ ] **Step 2: Register the new widget in the widget bundle**

Edit `CodyncLiveActivity/CodyncLiveActivityBundle.swift` — add `OverallLiveActivityWidget()` to the existing bundle:

```swift
import WidgetKit
import SwiftUI

@main
struct CodyncLiveActivityBundle: WidgetBundle {
    var body: some Widget {
        CodyncLiveActivityWidget()
        OverallLiveActivityWidget()
    }
}
```

- [ ] **Step 3: Build to verify**

Run: `xcodebuild -scheme CodyncLiveActivity -destination 'generic/platform=iOS' build 2>&1 | tail -5`

Expected: `BUILD SUCCEEDED`

- [ ] **Step 4: Commit**

```bash
git add CodyncLiveActivity/
git commit -m "feat: add Overall mode Live Activity widget layout"
```

---

## Task 9: Integration Verification

- [ ] **Step 1: Full project build**

Run: `xcodebuild -scheme Codync-iOS -destination 'generic/platform=iOS' build 2>&1 | tail -10`

Expected: `BUILD SUCCEEDED` with no warnings related to our changes.

- [ ] **Step 2: Verify all targets compile**

```bash
xcodebuild -scheme CodyncShared -destination 'generic/platform=iOS' build 2>&1 | tail -3
xcodebuild -scheme Codync-macOS -destination 'generic/platform=macOS' build 2>&1 | tail -3
```

Expected: Both `BUILD SUCCEEDED`. The macOS target should compile with the `#else` stubs for `OverallAttributes`.

- [ ] **Step 3: Final commit if any fixups needed**

```bash
git add -A
git commit -m "fix: resolve any compilation issues from integration"
```

Only create this commit if there were actual fixes needed.
