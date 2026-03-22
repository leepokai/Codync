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
    private var lastPushedState: [String: CodyncAttributes.ContentState] = [:]
    private var previousTasks: [String: String] = [:]        // sessionId → last completed tool
    private var secondPreviousTasks: [String: String] = [:]  // sessionId → second-last tool
    private var tickTimer: Timer?
    @Published var mode: LiveActivityMode = .overall
    @Published var maxOverallSessions: Int = 4
    private var overallActivity: Activity<OverallAttributes>?
    private var lastOverallState: OverallAttributes.ContentState?
    private var overallTrackedIds: Set<String> = []
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
        let statuses = sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
        logger.debug("updateSessions (\(self.mode.rawValue)): \(statuses)")
        sessionsByID = Dictionary(sessions.map { ($0.sessionId, $0) }, uniquingKeysWith: { _, last in last })

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
            if let session = sessionsByID[sessionId] {
                sendCompletionNotification(session)
            }
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

        // 3. Build desired set: pinned first, then auto-fill
        var desired: [String] = []

        // Add pinned sessions (that still exist and aren't completed)
        for sessionId in pinnedSessionIds {
            guard desired.count < Self.maxActivities else { break }
            guard let session = sessionsByID[sessionId], session.status != .completed else { continue }
            desired.append(sessionId)
        }

        // Auto-fill remaining slots with best candidates
        let remainingSlots = Self.maxActivities - desired.count
        if remainingSlots > 0 {
            let candidates = sessions
                .filter { !desired.contains($0.sessionId) && $0.status != .completed }
                .sorted { autoFillPriority($0) < autoFillPriority($1) }

            for session in candidates.prefix(remainingSlots) {
                // Skip idle sessions past grace that aren't pinned
                if session.status == .idle && !isInGrace(session.sessionId) {
                    continue
                }
                desired.append(session.sessionId)
            }
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
        // Detect if iOS ended the activity (stale timeout, user dismissed, etc.)
        if let activity = overallActivity,
           activity.activityState == .ended || activity.activityState == .dismissed {
            logger.info("Overall Live Activity was ended by iOS — will recreate")
            overallActivity = nil
            lastOverallState = nil
        }

        let active = sessions
            .filter { $0.status != .completed }
            .sorted { autoFillPriority($0) < autoFillPriority($1) }
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
                durationSec: liveDuration
            )
        }

        let totalCost = sessions.reduce(0) { $0 + $1.costUSD }
        let primaryId = active.first { $0.status == .working }?.sessionId
            ?? active.first?.sessionId

        let state = OverallAttributes.ContentState(
            sessions: summaries,
            primarySessionId: primaryId,
            totalCost: totalCost,
            isDark: isDarkMode
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

        // Send completion notifications for newly completed sessions
        for session in sessions where session.status == .completed {
            if overallTrackedIds.contains(session.sessionId) {
                sendCompletionNotification(session)
                overallTrackedIds.remove(session.sessionId)
            }
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
        previousTasks.removeAll()
        secondPreviousTasks.removeAll()
        graceTimers.removeAll()
        lastPushedState.removeAll()
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
        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil)
            )
            activities[session.sessionId] = activity
            lastPushedState[session.sessionId] = state
            trackedSessionIds.insert(session.sessionId)
            logger.info("Started Live Activity for \(session.sessionId)")
        } catch {
            logger.error("Failed to start Live Activity: \(error)")
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
