import ActivityKit
import CloudKit
import Foundation
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
    private var tickTimer: Timer?

    private static let maxActivities = 4

    init() {
        startTicking()
    }

    /// Tick every second to keep durationSec fresh — drives sparkle animation in Dynamic Island.
    /// Timer callback is @Sendable, so we hop back to @MainActor via Task.
    private func startTicking() {
        tickTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.tickActivities()
            }
        }
    }

    private func tickActivities() {
        guard !activities.isEmpty else { return }
        for sessionId in activities.keys {
            guard let session = sessionsByID[sessionId] else { continue }
            pushUpdateIfChanged(session)
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

    // MARK: - Unified Update

    func updateSessions(_ sessions: [SessionState]) {
        let statuses = sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
        logger.debug("updateSessions: \(statuses)")
        sessionsByID = Dictionary(sessions.map { ($0.sessionId, $0) }, uniquingKeysWith: { _, last in last })

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
            projectName: session.projectName
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
        let meaningfulChange = prev == nil
            || prev?.status != state.status
            || prev?.model != state.model
            || prev?.completedCount != state.completedCount
            || prev?.totalCount != state.totalCount
            || prev?.currentTask != state.currentTask
            || prev?.contextPct != state.contextPct
            || prev?.costUSD != state.costUSD

        guard meaningfulChange else { return }

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
        // Compute durationSec locally so it ticks every second — drives sparkle animation
        let liveDuration = max(session.durationSec, Int(Date().timeIntervalSince(session.startedAt)))
        return .init(
            status: session.status.rawValue,
            model: session.model,
            tasks: session.truncatedTasks,
            completedCount: session.completedTaskCount,
            totalCount: session.totalTaskCount,
            currentTask: session.currentTask,
            contextPct: session.contextPct,
            costUSD: session.costUSD,
            durationSec: liveDuration,
            sessionStartDate: session.startedAt
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
