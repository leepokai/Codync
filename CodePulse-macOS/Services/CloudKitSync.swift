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
    private var previousSessionIds: Set<String> = []
    private var isSyncing = false
    private var quotaBackoffUntil: Date?

    private static func log(_ msg: String) {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codepulse/sync.log")
        let line = "[\(Date())] \(msg)\n"
        if let h = try? FileHandle(forWritingTo: url) { h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close() }
        else { try? line.write(to: url, atomically: false, encoding: .utf8) }
    }

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .throttle(for: .seconds(5), scheduler: RunLoop.main, latest: true)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else { return }

        // Respect quota backoff
        if let backoff = quotaBackoffUntil, Date() < backoff { return }
        quotaBackoffUntil = nil

        let currentIds = Set(sessions.map(\.sessionId))

        // Sessions that disappeared or completed since last sync → delete from CloudKit
        let completedIds = sessions.filter { $0.status == .completed }.map(\.sessionId)
        let disappearedIds = previousSessionIds.subtracting(currentIds)
        let toDelete = Array(Set(completedIds).union(disappearedIds))

        // Active sessions with changed content → save to CloudKit
        let toSave = sessions.filter { session in
            session.status != .completed
            && previousStates[session.sessionId]?.updatedAt != session.updatedAt
        }

        guard !toDelete.isEmpty || !toSave.isEmpty else {
            previousSessionIds = currentIds
            return
        }

        isSyncing = true

        Task {
            defer {
                isSyncing = false
                previousSessionIds = currentIds
            }

            do {
                if !toDelete.isEmpty {
                    Self.log("deleteByIds \(toDelete.count) sessions")
                    try await CloudKitManager.shared.deleteByIds(toDelete)
                    Self.log("SUCCESS: deleted \(toDelete.count)")
                    for id in toDelete {
                        previousStates.removeValue(forKey: id)
                    }
                }

                if !toSave.isEmpty {
                    Self.log("saveBatch \(toSave.count) sessions")
                    try await CloudKitManager.shared.saveBatch(toSave)
                    Self.log("SUCCESS: saved \(toSave.count)")
                    for session in toSave {
                        previousStates[session.sessionId] = session
                    }
                }
            } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                let retryAfter = error.retryAfterSeconds ?? 600
                Self.log("QUOTA: backoff \(Int(retryAfter * 2))s")
                quotaBackoffUntil = Date().addingTimeInterval(retryAfter * 2)
            } catch {
                Self.log("ERROR: \(error)")
                quotaBackoffUntil = Date().addingTimeInterval(300)
            }
        }
    }
}
