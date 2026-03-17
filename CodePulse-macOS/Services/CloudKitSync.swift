import Foundation
import Combine
import CloudKit
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKitSync")

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var isSyncing = false
    private var quotaBackoffUntil: Date?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .throttle(for: .seconds(30), scheduler: RunLoop.main, latest: true)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else { return }

        // Respect quota backoff — do NOT send any requests during backoff
        if let backoff = quotaBackoffUntil, Date() < backoff { return }
        quotaBackoffUntil = nil

        let changed = sessions.filter { session in
            session.status != .completed
            && previousStates[session.sessionId]?.updatedAt != session.updatedAt
        }
        guard !changed.isEmpty else { return }

        isSyncing = true

        Task {
            defer { isSyncing = false }

            do {
                try await CloudKitManager.shared.saveBatch(changed)
                for session in changed {
                    previousStates[session.sessionId] = session
                }
            } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                let retryAfter = error.retryAfterSeconds ?? 600
                // Double the backoff to be safe
                quotaBackoffUntil = Date().addingTimeInterval(retryAfter * 2)
            } catch {
                // Unknown error — backoff 5 minutes
                quotaBackoffUntil = Date().addingTimeInterval(300)
            }
        }
    }
}
