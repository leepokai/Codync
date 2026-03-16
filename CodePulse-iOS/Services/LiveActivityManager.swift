import ActivityKit
import Foundation
import Combine
import UserNotifications
import CodePulseShared

@MainActor
final class LiveActivityManager: ObservableObject {
    @Published var trackedSessionId: String?
    private var currentActivity: Activity<CodePulseAttributes>?

    func startTracking(_ session: SessionState) {
        stopTracking()
        let attributes = CodePulseAttributes(
            sessionId: session.sessionId,
            projectName: session.projectName
        )
        let state = contentState(from: session)
        do {
            currentActivity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil
            )
            trackedSessionId = session.sessionId
        } catch {
            print("Failed to start Live Activity: \(error)")
        }
    }

    func stopTracking() {
        let activity = currentActivity
        currentActivity = nil
        trackedSessionId = nil
        Task { @MainActor in
            await activity?.end(nil, dismissalPolicy: .immediate)
        }
    }

    func update(sessions: [SessionState]) {
        guard let trackedId = trackedSessionId else {
            if let newest = sessions.first(where: { $0.status == .working }) {
                startTracking(newest)
            }
            return
        }
        guard let session = sessions.first(where: { $0.sessionId == trackedId }) else { return }
        if session.status == .completed {
            onSessionCompleted(session, allSessions: sessions)
        } else {
            let state = contentState(from: session)
            let activity = currentActivity
            Task { @MainActor in
                await activity?.update(.init(state: state, staleDate: nil))
            }
        }
    }

    private func onSessionCompleted(_ session: SessionState, allSessions: [SessionState]) {
        sendCompletionNotification(session)
        if let next = allSessions.first(where: { $0.status == .working && $0.sessionId != session.sessionId }) {
            startTracking(next)
        } else {
            let total = allSessions.reduce(0.0) { $0 + $1.costUSD }
            sendAllCompleteNotification(count: allSessions.count, totalCost: total)
            stopTracking()
        }
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

    private func sendAllCompleteNotification(count: Int, totalCost: Double) {
        let content = UNMutableNotificationContent()
        content.title = "All Sessions Complete"
        content.body = "All \(count) sessions finished · Total $\(String(format: "%.2f", totalCost))"
        content.sound = .default
        let request = UNNotificationRequest(identifier: "all-complete", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
