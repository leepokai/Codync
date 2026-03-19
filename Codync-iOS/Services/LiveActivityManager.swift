import ActivityKit
import CloudKit
import Foundation
import UserNotifications
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "LiveActivity")

enum TrackingMode: String, CaseIterable, Sendable {
    case auto
    case manual

    var label: String {
        switch self {
        case .auto: "Auto"
        case .manual: "Manual"
        }
    }

    var icon: String {
        switch self {
        case .auto: "antenna.radiowaves.left.and.right"
        case .manual: "hand.tap"
        }
    }
}

@MainActor
final class LiveActivityManager: ObservableObject {
    @Published private(set) var trackedSessionIds: Set<String> = []
    @Published var trackingMode: TrackingMode {
        didSet { UserDefaults.standard.set(trackingMode.rawValue, forKey: "codync_trackingMode") }
    }
    private var activities: [String: Activity<CodyncAttributes>] = [:]
    private var manualPins: Set<String> = [] // user-pinned sessions in manual mode
    private var sessionsByID: [String: SessionState] = [:]
    private var lastPushedState: [String: CodyncAttributes.ContentState] = [:]
    private var tickTimer: Timer?
    private var pushTokenTasks: [String: Task<Void, Never>] = [:]

    private static let maxActivities = 4

    init() {
        let saved = UserDefaults.standard.string(forKey: "codync_trackingMode") ?? "auto"
        self.trackingMode = TrackingMode(rawValue: saved) ?? .auto
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
        manualPins.contains(sessionId)
    }

    // Manual mode: toggle pin
    func togglePin(_ session: SessionState) {
        if manualPins.contains(session.sessionId) {
            manualPins.remove(session.sessionId)
            stopTracking(sessionId: session.sessionId)
        } else {
            guard manualPins.count < Self.maxActivities else { return }
            manualPins.insert(session.sessionId)
            startTracking(session)
        }
    }

    func updateSessions(_ sessions: [SessionState]) {
        sessionsByID = Dictionary(sessions.map { ($0.sessionId, $0) }, uniquingKeysWith: { _, last in last })
        // Clean up completed sessions (snapshot IDs first to avoid mutation during iteration)
        let completedIds = trackedSessionIds.filter { id in
            sessionsByID[id]?.status == .completed
        }
        for sessionId in completedIds {
            if let session = sessionsByID[sessionId] {
                sendCompletionNotification(session)
            }
            stopTracking(sessionId: sessionId)
            manualPins.remove(sessionId)
        }

        switch trackingMode {
        case .auto:
            autoUpdate(sessions)
        case .manual:
            manualUpdate(sessions)
        }
    }

    // MARK: - Auto mode

    /// Priority: active sessions (working > needsInput > compacting) sorted by progress desc, then startedAt desc.
    /// Includes non-working active states so the Dynamic Island persists through status transitions
    /// (e.g., working → needsInput between turns, or working → compacting).
    private func autoUpdate(_ sessions: [SessionState]) {
        let active = sessions
            .filter { $0.status == .working || $0.status == .needsInput || $0.status == .compacting }
            .sorted { a, b in
                let priorityOf: (SessionStatus) -> Int = { switch $0 {
                    case .working: 0
                    case .needsInput: 1
                    case .compacting: 2
                    default: 3
                }}
                let pa = priorityOf(a.status), pb = priorityOf(b.status)
                if pa != pb { return pa < pb }
                let progressA = a.totalTaskCount > 0 ? Double(a.completedTaskCount) / Double(a.totalTaskCount) : 0
                let progressB = b.totalTaskCount > 0 ? Double(b.completedTaskCount) / Double(b.totalTaskCount) : 0
                if progressA != progressB { return progressA > progressB }
                return a.startedAt > b.startedAt
            }

        let desired = Set(active.prefix(Self.maxActivities).map(\.sessionId))

        // Stop tracking sessions no longer in top N (snapshot to avoid mutation during iteration)
        let toRemove = trackedSessionIds.filter { !desired.contains($0) }
        for sessionId in toRemove {
            stopTracking(sessionId: sessionId)
        }

        // Start tracking new top sessions, update existing
        for session in active.prefix(Self.maxActivities) {
            if !isTracking(sessionId: session.sessionId) {
                startTracking(session)
            } else {
                pushUpdateIfChanged(session)
            }
        }
    }

    // MARK: - Manual mode

    private func manualUpdate(_ sessions: [SessionState]) {
        for sessionId in manualPins {
            guard let session = sessionsByID[sessionId] else { continue }
            if !isTracking(sessionId: sessionId) {
                startTracking(session)
            } else {
                pushUpdateIfChanged(session)
            }
        }
    }

    // MARK: - Core

    private func startTracking(_ session: SessionState) {
        guard !isTracking(sessionId: session.sessionId) else { return }
        guard trackedSessionIds.count < Self.maxActivities else { return }

        let attributes = CodyncAttributes(
            sessionId: session.sessionId,
            projectName: session.projectName
        )
        let state = contentState(from: session)
        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: .token
            )
            activities[session.sessionId] = activity
            lastPushedState[session.sessionId] = state
            trackedSessionIds.insert(session.sessionId)
            observePushToken(activity: activity, sessionId: session.sessionId)
            logger.info("Started Live Activity for \(session.sessionId) with push token support")
        } catch {
            logger.error("Failed to start Live Activity: \(error)")
        }
    }

    /// Observe push token updates and sync to CloudKit for the macOS app to read.
    private func observePushToken(activity: Activity<CodyncAttributes>, sessionId: String) {
        pushTokenTasks[sessionId]?.cancel()
        pushTokenTasks[sessionId] = Task {
            for await token in activity.pushTokenUpdates {
                let hex = token.map { String(format: "%02x", $0) }.joined()
                logger.info("Push token for \(sessionId): \(hex.prefix(8))...")
                await savePushToken(sessionId: sessionId, token: token)
            }
        }
    }

    /// Save push token to CloudKit so macOS can read it and POST to the Worker.
    private func savePushToken(sessionId: String, token: Data) async {
        let recordID = CKRecord.ID(
            recordName: "pushtoken-\(sessionId)",
            zoneID: CloudKitManager.zoneID
        )
        let record = CKRecord(recordType: "PushToken", recordID: recordID)
        record["sessionId"] = sessionId as CKRecordValue
        record["token"] = token.map { String(format: "%02x", $0) }.joined() as CKRecordValue
        record["updatedAt"] = Date() as CKRecordValue
        do {
            _ = try await CloudKitManager.shared.database.save(record)
            logger.info("Saved push token to CloudKit for \(sessionId)")
        } catch {
            logger.error("Failed to save push token: \(error.localizedDescription)")
        }
    }

    /// Push an activity update only if the content state actually changed.
    private func pushUpdateIfChanged(_ session: SessionState) {
        guard let activity = activities[session.sessionId] else { return }
        let state = contentState(from: session)
        guard state != lastPushedState[session.sessionId] else { return }
        lastPushedState[session.sessionId] = state
        Task {
            await activity.update(.init(state: state, staleDate: nil))
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

    /// When switching modes, stop all current activities and re-evaluate
    func onModeChanged(sessions: [SessionState]) {
        // Stop all
        for sessionId in Array(trackedSessionIds) {
            stopTracking(sessionId: sessionId)
        }
        if trackingMode == .auto {
            manualPins.removeAll()
        }
        updateSessions(sessions)
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
