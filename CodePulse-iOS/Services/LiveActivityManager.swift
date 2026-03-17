import ActivityKit
import Foundation
import UserNotifications
import CodePulseShared

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
        didSet { UserDefaults.standard.set(trackingMode.rawValue, forKey: "codepulse_trackingMode") }
    }
    private var activities: [String: Activity<CodePulseAttributes>] = [:]
    private var manualPins: Set<String> = [] // user-pinned sessions in manual mode

    private static let maxActivities = 4

    init() {
        let saved = UserDefaults.standard.string(forKey: "codepulse_trackingMode") ?? "auto"
        self.trackingMode = TrackingMode(rawValue: saved) ?? .auto
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
        // Clean up completed sessions (snapshot IDs first to avoid mutation during iteration)
        let completedIds = trackedSessionIds.filter { id in
            sessions.first(where: { $0.sessionId == id })?.status == .completed
        }
        for sessionId in completedIds {
            if let session = sessions.first(where: { $0.sessionId == sessionId }) {
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

    /// Priority: working sessions sorted by progress desc (near-completion first), then startedAt desc (newer first)
    private func autoUpdate(_ sessions: [SessionState]) {
        let working = sessions
            .filter { $0.status == .working }
            .sorted { a, b in
                let progressA = a.totalTaskCount > 0 ? Double(a.completedTaskCount) / Double(a.totalTaskCount) : 0
                let progressB = b.totalTaskCount > 0 ? Double(b.completedTaskCount) / Double(b.totalTaskCount) : 0
                if progressA != progressB { return progressA > progressB }
                return a.startedAt > b.startedAt
            }

        let desired = Set(working.prefix(Self.maxActivities).map(\.sessionId))

        // Stop tracking sessions no longer in top N (snapshot to avoid mutation during iteration)
        let toRemove = trackedSessionIds.filter { !desired.contains($0) }
        for sessionId in toRemove {
            stopTracking(sessionId: sessionId)
        }

        // Start tracking new top sessions
        for session in working.prefix(Self.maxActivities) {
            if !isTracking(sessionId: session.sessionId) {
                startTracking(session)
            } else if let activity = activities[session.sessionId] {
                let state = contentState(from: session)
                Task { @MainActor in
                    await activity.update(.init(state: state, staleDate: nil))
                }
            }
        }
    }

    // MARK: - Manual mode

    private func manualUpdate(_ sessions: [SessionState]) {
        for sessionId in manualPins {
            guard let session = sessions.first(where: { $0.sessionId == sessionId }) else { continue }
            if !isTracking(sessionId: sessionId) {
                startTracking(session)
            } else if let activity = activities[sessionId] {
                let state = contentState(from: session)
                Task { @MainActor in
                    await activity.update(.init(state: state, staleDate: nil))
                }
            }
        }
    }

    // MARK: - Core

    private func startTracking(_ session: SessionState) {
        guard !isTracking(sessionId: session.sessionId) else { return }
        guard trackedSessionIds.count < Self.maxActivities else { return }

        let attributes = CodePulseAttributes(
            sessionId: session.sessionId,
            projectName: session.projectName
        )
        let state = contentState(from: session)
        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil
            )
            activities[session.sessionId] = activity
            trackedSessionIds.insert(session.sessionId)
        } catch {
            print("Failed to start Live Activity: \(error)")
        }
    }

    private func stopTracking(sessionId: String) {
        guard let activity = activities.removeValue(forKey: sessionId) else { return }
        trackedSessionIds.remove(sessionId)
        Task { @MainActor in
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

    private func contentState(from session: SessionState) -> CodePulseAttributes.ContentState {
        .init(
            status: session.status.rawValue,
            model: session.model,
            tasks: session.truncatedTasks,
            completedCount: session.completedTaskCount,
            totalCount: session.totalTaskCount,
            currentTask: session.currentTask,
            contextPct: session.contextPct,
            costUSD: session.costUSD,
            durationSec: session.durationSec
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
