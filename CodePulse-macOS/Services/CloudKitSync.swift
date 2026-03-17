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
        logger.info("CloudKitSync initialized")
    }

    private static let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".codepulse/cloudkit-debug.log")

    private static func log(_ msg: String) {
        let line = "[\(Date())] \(msg)\n"
        if let handle = try? FileHandle(forWritingTo: logURL) {
            handle.seekToEndOfFile()
            handle.write(line.data(using: .utf8)!)
            try? handle.close()
        } else {
            try? line.write(to: logURL, atomically: false, encoding: .utf8)
        }
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else { return }

        // Respect quota backoff
        if let backoff = quotaBackoffUntil, Date() < backoff {
            let remaining = Int(backoff.timeIntervalSinceNow)
            Self.log("Quota backoff: \(remaining)s remaining")
            return
        }
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
                Self.log("Batch saved \(changed.count) sessions OK")
                for session in changed {
                    previousStates[session.sessionId] = session
                }
            } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                let retryAfter = error.retryAfterSeconds ?? 300
                Self.log("Quota hit, backing off \(Int(retryAfter))s")
                quotaBackoffUntil = Date().addingTimeInterval(retryAfter)
            } catch {
                Self.log("ERROR: \(error)")
            }
        }
    }
}
