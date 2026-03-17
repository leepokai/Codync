import Foundation
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKitSync")

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var isSyncing = false

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .throttle(for: .seconds(30), scheduler: RunLoop.main, latest: true)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
        logger.info("CloudKitSync initialized")
        Self.debugLog("CloudKitSync initialized, subscribing to sessions publisher")
        // Also add a direct observation to debug
        stateManager.$sessions
            .sink { sessions in
                Self.debugLog("Publisher emitted \(sessions.count) sessions")
            }
            .store(in: &cancellables)
    }

    private static func debugLog(_ msg: String) {
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".codepulse/cloudkit-debug.log")
        let line = "[\(Date())] \(msg)\n"
        if let handle = try? FileHandle(forWritingTo: url) {
            handle.seekToEndOfFile()
            handle.write(line.data(using: .utf8)!)
            try? handle.close()
        } else {
            try? line.write(to: url, atomically: false, encoding: .utf8)
        }
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        Self.debugLog("syncToCloud called with \(sessions.count) sessions")

        guard !isSyncing else {
            Self.debugLog("Sync skipped — already in progress")
            return
        }

        // Filter to only changed active sessions (skip completed to save quota)
        let changed = sessions.filter { session in
            session.status != .completed
            && previousStates[session.sessionId]?.updatedAt != session.updatedAt
        }

        guard !changed.isEmpty else {
            Self.debugLog("Sync skipped — no changes (\(sessions.count) sessions, \(previousStates.count) prev)")
            return
        }

        isSyncing = true
        logger.info("Syncing \(changed.count) changed sessions to CloudKit...")
        Self.debugLog("Starting sync of \(changed.count) sessions")

        Task {
            defer { isSyncing = false }

            var syncedCount = 0
            for session in changed {
                do {
                    try await CloudKitManager.shared.save(session)
                    Self.debugLog("Saved \(session.sessionId.prefix(8)) OK")
                    previousStates[session.sessionId] = session
                    syncedCount += 1
                    // 3s delay between saves to avoid quota exhaustion
                    try? await Task.sleep(for: .seconds(3))
                } catch {
                    Self.debugLog("ERROR saving \(session.sessionId.prefix(8)): \(error)")
                    // On quota exceeded, stop trying more sessions this cycle
                    break
                }
            }
            if syncedCount > 0 {
                logger.info("Synced \(syncedCount)/\(changed.count) sessions to CloudKit")
                Self.debugLog("Synced \(syncedCount)/\(changed.count) sessions OK")
            } else {
                Self.debugLog("No sessions synced (all failed?)")
            }
        }
    }
}
