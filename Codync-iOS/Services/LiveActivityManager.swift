import ActivityKit
import CloudKit
import Foundation
import SwiftUI
import UIKit
import UserNotifications
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "LiveActivity")


@MainActor
final class LiveActivityManager: ObservableObject {
    @Published private(set) var trackedSessionIds: Set<String> = []
    @Published private(set) var pinnedSessionIds: Set<String> = []
    private var graceTimers: [String: Date] = [:]
    private var activities: [String: Activity<CodyncAttributes>] = [:]
    private var sessionsByID: [String: SessionState] = [:]
    private var previousStatuses: [String: SessionStatus] = [:]  // sessionId → last known status
    private var lastPushedState: [String: CodyncAttributes.ContentState] = [:]
    private var previousTasks: [String: String] = [:]        // sessionId → last completed tool
    private var secondPreviousTasks: [String: String] = [:]  // sessionId → second-last tool
    private var tickTimer: Timer?
    @Published var mode: LiveActivityMode = .overall
    @Published var maxOverallSessions: Int = 4
    var userPrimarySessionId: String?
    private var overallActivity: Activity<OverallAttributes>?
    private var lastOverallState: OverallAttributes.ContentState?
    private var overallTrackedIds: Set<String> = []
    private var pushTokenTasks: [String: Task<Void, Never>] = [:]
    @AppStorage("codync_darkMode") private var isDarkMode = true

    private static let maxActivities = 4

    init() {
        startTicking()
    }

    /// Tick every second to keep durationSec fresh — drives sparkle animation in Dynamic Island.
    /// Timer callback is @Sendable, so we hop back to @MainActor via Task.
    private func startTicking() {
        tickTimer?.invalidate()
        let timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.tickActivities()
            }
        }
        // Fire in .common mode so ticking survives scroll tracking
        RunLoop.main.add(timer, forMode: .common)
        tickTimer = timer
    }

    /// Re-establish the tick timer after returning from background — iOS can silently
    /// invalidate timers during long suspensions.
    func ensureTimerRunning() {
        if tickTimer == nil || tickTimer?.isValid == false {
            logger.info("Tick timer was dead — restarting")
            startTicking()
        }
    }

    /// Clear cached state so the next updateSessions() pushes unconditionally.
    func invalidateCache() {
        lastPushedState.removeAll()
        lastOverallState = nil
    }

    private func tickActivities() {
        // Individual mode: tick each tracked activity
        if !activities.isEmpty {
            for sessionId in activities.keys {
                guard let session = sessionsByID[sessionId] else { continue }
                pushUpdateIfChanged(session)
            }
        }
        // Overall mode: detect iOS-ended activity so next updateSessions() recreates it
        if let activity = overallActivity,
           activity.activityState == .ended || activity.activityState == .dismissed {
            logger.info("Tick detected Overall Activity ended by iOS — clearing")
            overallActivity = nil
            lastOverallState = nil
        }
    }

    func isTracking(sessionId: String) -> Bool {
        trackedSessionIds.contains(sessionId)
    }

    func isPinned(sessionId: String) -> Bool {
        pinnedSessionIds.contains(sessionId)
    }

    // MARK: - Pin / Unpin

    func togglePin(_ sessionId: String) {
        if pinnedSessionIds.contains(sessionId) {
            pinnedSessionIds.remove(sessionId)
            Task { try? await CloudKitManager.shared.unpinSession(sessionId) }
        } else {
            guard pinnedSessionIds.count < Self.maxActivities else { return }
            pinnedSessionIds.insert(sessionId)
            Task { try? await CloudKitManager.shared.pinSession(sessionId) }
        }
        // Re-evaluate tracking with current sessions
        updateSessions(Array(sessionsByID.values))
    }

    func loadPinnedSessions() async {
        do {
            pinnedSessionIds = try await CloudKitManager.shared.fetchPinnedSessionIds()
        } catch {
            // First run or no pins yet — not an error
        }
    }

    func loadPreference() async {
        let pref = await CloudKitManager.shared.fetchLiveActivityPreference()
        mode = pref.mode
        maxOverallSessions = pref.maxSessions
    }

    func savePreference() async {
        await CloudKitManager.shared.setLiveActivityPreference(mode: mode, maxSessions: maxOverallSessions)
    }

    // MARK: - Recovery

    /// Recover Activity references that iOS still manages but we lost
    /// (e.g., after app termination + background relaunch).
    private func recoverOrphanedActivities() {
        for activity in Activity<CodyncAttributes>.activities {
            let sid = activity.attributes.sessionId
            guard activity.activityState == .active || activity.activityState == .stale else {
                continue
            }
            if activities[sid] == nil {
                activities[sid] = activity
                trackedSessionIds.insert(sid)
                logger.info("Recovered orphaned Live Activity for \(sid)")
            }
        }
    }

    /// Prune activities that iOS has ended/dismissed but we still hold references to.
    private func pruneEndedActivities() {
        for (sid, activity) in activities {
            if activity.activityState == .ended || activity.activityState == .dismissed {
                activities.removeValue(forKey: sid)
                trackedSessionIds.remove(sid)
                lastPushedState.removeValue(forKey: sid)
                logger.info("Pruned ended Live Activity for \(sid)")
            }
        }
    }

    // MARK: - Unified Update

    func updateSessions(_ sessions: [SessionState]) {
        logger.debug("updateSessions (\(self.mode.rawValue)) primary=\(self.userPrimarySessionId ?? "none") sessions=\(sessions.count)")
        sessionsByID = Dictionary(sessions.map { ($0.sessionId, $0) }, uniquingKeysWith: { _, last in last })

        // Notify when primary session transitions from working → idle/completed
        if let primaryId = userPrimarySessionId {
            let session = sessionsByID[primaryId]
            let prevStatus = previousStatuses[primaryId]
            #if DEBUG
            logger.info("ALERT CHECK: primary=\(primaryId.suffix(8)) status=\(session?.status.rawValue ?? "nil") prev=\(prevStatus?.rawValue ?? "nil")")
            #endif
            if let session,
               (session.status == .idle || session.status == .needsInput || session.status == .completed),
               prevStatus == .working {
                #if DEBUG
                logger.info("ALERT SENDING local notification for \(session.projectName)")
                #endif
                sendCompletionNotification(session)
            }
        }

        // Track statuses for next comparison
        for session in sessions {
            previousStatuses[session.sessionId] = session.status
        }

        switch mode {
        case .individual: updateIndividual(sessions)
        case .overall:    updateOverall(sessions)
        }
    }

    private func updateIndividual(_ sessions: [SessionState]) {
        // Recover any activities iOS still has running but we lost track of
        recoverOrphanedActivities()
        // Remove stale references to activities iOS has already ended
        pruneEndedActivities()

        // 1. Clean up completed/dead sessions
        let completedIds = trackedSessionIds.filter { id in
            guard let session = sessionsByID[id] else { return true }
            return session.status == .completed
        }
        for sessionId in completedIds {
            stopTracking(sessionId: sessionId)
            pinnedSessionIds.remove(sessionId)
            graceTimers.removeValue(forKey: sessionId)
        }

        // Also clean pinned sessions that no longer exist
        let deadPinIds = pinnedSessionIds.filter { sessionsByID[$0] == nil }
        for sessionId in deadPinIds {
            pinnedSessionIds.remove(sessionId)
        }

        // 2. Update grace timers for currently tracked sessions
        for sessionId in trackedSessionIds {
            guard let session = sessionsByID[sessionId] else { continue }
            if session.status == .idle {
                // Start grace timer if not already running
                if graceTimers[sessionId] == nil {
                    graceTimers[sessionId] = Date()
                }
            } else {
                // Session is active again — remove grace timer
                graceTimers.removeValue(forKey: sessionId)
            }
        }

        // 3. Build desired set: auto-fill by priority up to maxOverallSessions
        var desired: [String] = []

        let maxSlots = maxOverallSessions
        let candidates = sortedSessions(sessions.filter { $0.status != .completed })

        for session in candidates {
            guard desired.count < maxSlots else { break }
            // Skip idle sessions past grace
            if session.status == .idle && !isInGrace(session.sessionId) {
                continue
            }
            desired.append(session.sessionId)
        }

        let desiredSet = Set(desired)

        // 4. Stop tracking sessions not in desired set
        let toRemove = trackedSessionIds.filter { !desiredSet.contains($0) }
        for sessionId in toRemove {
            stopTracking(sessionId: sessionId)
            graceTimers.removeValue(forKey: sessionId)
        }

        // 5. Start/update tracking for desired sessions
        for sessionId in desired {
            guard let session = sessionsByID[sessionId] else { continue }
            if !isTracking(sessionId: sessionId) {
                startTracking(session)
            } else {
                pushUpdateIfChanged(session)
            }
        }
    }

    private func updateOverall(_ sessions: [SessionState]) {
        // Recover orphaned Overall activity that iOS still manages but we lost reference to
        // (e.g., after app termination + background relaunch)
        if overallActivity == nil {
            for activity in Activity<OverallAttributes>.activities
                where activity.activityState == .active || activity.activityState == .stale {
                overallActivity = activity
                logger.info("Recovered orphaned Overall Live Activity")
                break
            }
        }

        // End any extra Overall activities beyond the one we're tracking
        for activity in Activity<OverallAttributes>.activities
            where activity.id != overallActivity?.id
                && (activity.activityState == .active || activity.activityState == .stale) {
            Task { await activity.end(nil, dismissalPolicy: .immediate) }
            logger.info("Ended duplicate Overall Live Activity")
        }

        // Detect if iOS ended the activity (stale timeout, user dismissed, etc.)
        if let activity = overallActivity,
           activity.activityState == .ended || activity.activityState == .dismissed {
            logger.info("Overall Live Activity was ended by iOS — will recreate")
            overallActivity = nil
            lastOverallState = nil
        }

        // Pro: ensure we're observing push token for existing activity
        if let activity = overallActivity, PremiumManager.shared.isPro {
            if activity.pushToken == nil && pushTokenTasks["__overall__"] == nil {
                // Activity exists but wasn't created with pushType: .token — recreate
                logger.info("Pro active but Overall Activity has no push token — recreating")
                Task { await activity.end(nil, dismissalPolicy: .immediate) }
                overallActivity = nil
                lastOverallState = nil
            } else if pushTokenTasks["__overall__"] == nil {
                // Activity has token but we're not observing — start observing + save existing token
                logger.info("Resuming push token observation for existing Overall Activity")
                observeOverallPushToken(activity: activity)
                if let token = activity.pushToken {
                    let hex = token.map { String(format: "%02x", $0) }.joined()
                    logger.info("Existing Overall push token: \(hex.prefix(8))...")
                    Task { await savePushToken(sessionId: "__overall__", tokenHex: hex) }
                }
            }
        }

        let active = sortedSessions(sessions.filter { $0.status != .completed })
            .prefix(maxOverallSessions)

        let summaries = active.map { session in
            let liveDuration = max(session.durationSec, Int(Date().timeIntervalSince(session.startedAt)))
            return SessionSummary(
                sessionId: session.sessionId,
                projectName: session.projectName,
                status: session.status,
                model: session.model,
                currentTask: session.currentTask,
                costUSD: session.costUSD,
                durationSec: liveDuration,
                completedCount: session.completedTaskCount,
                totalCount: session.totalTaskCount
            )
        }

        let totalCost = sessions.reduce(0) { $0 + $1.costUSD }
        // User-selected primary takes precedence, then auto-pick first working
        let resolvedPrimaryId = userPrimarySessionId.flatMap { id in
            active.contains(where: { $0.sessionId == id }) ? id : nil
        } ?? active.first(where: { $0.status == .working })?.sessionId
            ?? active.first?.sessionId

        let state = OverallAttributes.ContentState(
            sessions: summaries,
            primarySessionId: resolvedPrimaryId,
            totalCost: totalCost,
            isDark: isDarkMode
        )

        logger.debug("Overall state: primary=\(resolvedPrimaryId ?? "nil"), sessions=\(summaries.count)")
        guard state != lastOverallState else { return }
        lastOverallState = state

        if overallActivity == nil {
            guard UIApplication.shared.applicationState == .active else { return }
            let isPro = PremiumManager.shared.isPro
            do {
                let activity = try Activity.request(
                    attributes: OverallAttributes(),
                    content: .init(state: state, staleDate: nil),
                    pushType: isPro ? .token : nil
                )
                overallActivity = activity
                if isPro {
                    observeOverallPushToken(activity: activity)
                }
                logger.info("Started Overall Live Activity with \(summaries.count) sessions (push: \(isPro))")
            } catch {
                logger.error("Failed to start Overall Live Activity: \(error)")
            }
        } else if let activity = overallActivity {
            let content = ActivityContent(state: state, staleDate: nil, relevanceScore: 100)
            Task {
                await activity.update(content)
            }
        }

        // Track sessions seen in Overall mode
        for session in active {
            overallTrackedIds.insert(session.sessionId)
        }

        // Clean up completed sessions from overall tracking
        for session in sessions where session.status == .completed {
            overallTrackedIds.remove(session.sessionId)
        }
    }

    func switchMode(to newMode: LiveActivityMode) async {
        guard newMode != mode else { return }
        logger.info("Switching Live Activity mode: \(self.mode.rawValue) → \(newMode.rawValue)")

        // 1. End current mode's activities with await
        if mode == .individual {
            let snapshot = Array(activities.values)
            activities.removeAll()
            trackedSessionIds.removeAll()
            lastPushedState.removeAll()
            for activity in snapshot {
                await Task { await activity.end(nil, dismissalPolicy: .immediate) }.value
            }
        } else if let activity = overallActivity {
            overallActivity = nil
            lastOverallState = nil
            await Task { await activity.end(nil, dismissalPolicy: .immediate) }.value
        }

        // 2. Scan and end ALL orphaned iOS-managed activities (crash recovery)
        for activity in Activity<CodyncAttributes>.activities
            where activity.activityState == .active || activity.activityState == .stale {
            await Task { await activity.end(nil, dismissalPolicy: .immediate) }.value
        }
        for activity in Activity<OverallAttributes>.activities
            where activity.activityState == .active || activity.activityState == .stale {
            await Task { await activity.end(nil, dismissalPolicy: .immediate) }.value
        }

        // 3. Safety clear all internal state
        activities.removeAll()
        trackedSessionIds.removeAll()
        previousStatuses.removeAll()
        previousTasks.removeAll()
        secondPreviousTasks.removeAll()
        graceTimers.removeAll()
        lastPushedState.removeAll()
        pushTokenTasks.values.forEach { $0.cancel() }
        pushTokenTasks.removeAll()
        overallActivity = nil
        lastOverallState = nil
        overallTrackedIds.removeAll()

        // 4-5. Set new mode and persist
        mode = newMode
        await savePreference()

        // 6. Recreate with current data
        updateSessions(Array(sessionsByID.values))
    }

    // MARK: - Grace Period Helpers

    private func isInGrace(_ sessionId: String) -> Bool {
        guard let start = graceTimers[sessionId] else { return false }
        return Date().timeIntervalSince(start) < 60
    }

    /// Unified session sort: primary first, then by status priority.
    func sortedSessions(_ sessions: [SessionState]) -> [SessionState] {
        let pid = userPrimarySessionId
        return sessions.sorted { a, b in
            if a.sessionId == pid { return true }
            if b.sessionId == pid { return false }
            return autoFillPriority(a) < autoFillPriority(b)
        }
    }

    private func autoFillPriority(_ session: SessionState) -> Int {
        switch session.status {
        case .working: return 0
        case .needsInput: return 1
        case .compacting: return 2
        case .idle: return isInGrace(session.sessionId) ? 3 : 4
        case .error: return 4
        case .completed: return 5
        }
    }

    // MARK: - Core

    private func startTracking(_ session: SessionState) {
        guard !isTracking(sessionId: session.sessionId) else { return }
        guard trackedSessionIds.count < Self.maxActivities else { return }

        // Can only START Live Activities when app is in foreground
        guard UIApplication.shared.applicationState == .active else {
            logger.debug("Skipping Live Activity start — app in background")
            return
        }

        let attributes = CodyncAttributes(
            sessionId: session.sessionId,
            projectName: session.projectName,
            summary: session.summary
        )
        let state = contentState(from: session)
        let isPro = PremiumManager.shared.isPro
        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: isPro ? .token : nil
            )
            activities[session.sessionId] = activity
            lastPushedState[session.sessionId] = state
            trackedSessionIds.insert(session.sessionId)
            if isPro {
                observePushToken(activity: activity, sessionId: session.sessionId)
            }
            logger.info("Started Live Activity for \(session.sessionId) (push: \(isPro))")
        } catch {
            logger.error("Failed to start Live Activity: \(error)")
        }
    }

    // MARK: - Push Token Sync (Pro only)

    private func observePushToken(activity: Activity<CodyncAttributes>, sessionId: String) {
        pushTokenTasks[sessionId]?.cancel()
        pushTokenTasks[sessionId] = Task {
            for await token in activity.pushTokenUpdates {
                let hex = token.map { String(format: "%02x", $0) }.joined()
                logger.info("Push token for \(sessionId): \(hex.prefix(8))...")
                await savePushToken(sessionId: sessionId, tokenHex: hex)
            }
        }
    }

    private func observeOverallPushToken(activity: Activity<OverallAttributes>) {
        pushTokenTasks["__overall__"]?.cancel()
        pushTokenTasks["__overall__"] = Task {
            for await token in activity.pushTokenUpdates {
                let hex = token.map { String(format: "%02x", $0) }.joined()
                logger.info("Overall push token: \(hex.prefix(8))...")
                await savePushToken(sessionId: "__overall__", tokenHex: hex)
            }
        }
    }

    private func savePushToken(sessionId: String, tokenHex: String) async {
        let recordID = CKRecord.ID(
            recordName: "pushtoken-\(sessionId)",
            zoneID: CloudKitManager.zoneID
        )
        do {
            // Fetch existing record or create new one (upsert)
            let record: CKRecord
            do {
                record = try await CloudKitManager.shared.database.record(for: recordID)
            } catch {
                record = CKRecord(recordType: "PushToken", recordID: recordID)
            }
            record["sessionId"] = sessionId as CKRecordValue
            record["token"] = tokenHex as CKRecordValue
            record["updatedAt"] = Date() as CKRecordValue
            _ = try await CloudKitManager.shared.database.save(record)
            logger.info("Saved push token to CloudKit for \(sessionId)")
        } catch {
            logger.error("Failed to save push token: \(error.localizedDescription)")
        }
    }

    /// Push an activity update only if non-time fields changed.
    /// durationSec changes every second — comparing it would cause constant updates.
    private func pushUpdateIfChanged(_ session: SessionState) {
        guard let activity = activities[session.sessionId] else { return }
        let state = contentState(from: session)

        let prev = lastPushedState[session.sessionId]
        let fieldChange = prev == nil
            || prev?.status != state.status
            || prev?.model != state.model
            || prev?.completedCount != state.completedCount
            || prev?.totalCount != state.totalCount
            || prev?.currentTask != state.currentTask
            || prev?.previousTask != state.previousTask
            || prev?.contextPct != state.contextPct
            || prev?.costUSD != state.costUSD
        // Update every 2s for sparkle animation, even without field changes
        let animationTick = prev != nil && (state.durationSec / 2 != prev!.durationSec / 2)

        guard fieldChange || animationTick else { return }

        if let prev, prev.status != state.status {
            logger.info("[\(session.projectName)] status changed: \(prev.status) → \(state.status)")
        }
        lastPushedState[session.sessionId] = state
        // Pinned sessions get highest relevance → iOS shows them in Dynamic Island
        let score: Double = pinnedSessionIds.contains(session.sessionId) ? 100 : 25
        Task {
            await activity.update(.init(state: state, staleDate: nil, relevanceScore: score))
        }
    }

    private func stopTracking(sessionId: String) {
        guard let activity = activities.removeValue(forKey: sessionId) else { return }
        trackedSessionIds.remove(sessionId)
        lastPushedState.removeValue(forKey: sessionId)
        pushTokenTasks.removeValue(forKey: sessionId)?.cancel()
        Task {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
    }

    private func contentState(from session: SessionState) -> CodyncAttributes.ContentState {
        // Track tool changes: when currentTask changes, old one becomes previousTask
        let currentTool = session.currentTask
        if let current = currentTool,
           let prev = lastPushedState[session.sessionId]?.currentTask,
           current != prev, !prev.isEmpty {
            // Shift chain: previous → secondPrevious, current → previous
            secondPreviousTasks[session.sessionId] = previousTasks[session.sessionId]
            previousTasks[session.sessionId] = prev
        }

        let liveDuration = max(session.durationSec, Int(Date().timeIntervalSince(session.startedAt)))
        return .init(
            status: session.status.rawValue,
            model: session.model,
            tasks: session.truncatedTasks,
            completedCount: session.completedTaskCount,
            totalCount: session.totalTaskCount,
            currentTask: currentTool,
            previousTask: previousTasks[session.sessionId],
            secondPreviousTask: secondPreviousTasks[session.sessionId],
            contextPct: session.contextPct,
            costUSD: session.costUSD,
            durationSec: liveDuration,
            sessionStartDate: session.startedAt,
            isDark: isDarkMode
        )
    }

    private func sendCompletionNotification(_ session: SessionState) {
        let content = UNMutableNotificationContent()
        content.title = "Session Complete"
        content.body = "\(session.summary) finished · $\(String(format: "%.2f", session.costUSD))"
        content.sound = .default
        let request = UNNotificationRequest(identifier: session.sessionId, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
