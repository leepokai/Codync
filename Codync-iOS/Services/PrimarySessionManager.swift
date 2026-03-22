import Foundation
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "PrimarySession")

@MainActor
final class PrimarySessionManager: ObservableObject {
    @Published var primarySessionId: String?
    @Published var isManuallyLocked: Bool = false

    func autoSelect(from sessions: [SessionState]) {
        guard isManuallyLocked else {
            // No manual lock — no primary session
            if primarySessionId != nil {
                primarySessionId = nil
            }
            return
        }
        // If locked session disappeared, unlock
        if let lockedId = primarySessionId,
           !sessions.contains(where: { $0.sessionId == lockedId }) {
            logger.info("Locked primary session \(lockedId) no longer exists, unlocking")
            isManuallyLocked = false
            primarySessionId = nil
        }
    }

    func manualLock(_ sessionId: String) {
        if primarySessionId == sessionId && isManuallyLocked {
            // Tap again to deselect — let algorithm auto-decide
            isManuallyLocked = false
            primarySessionId = nil
            logger.info("Deselected primary: \(sessionId)")
            Task { await save() }
            return
        }
        primarySessionId = sessionId
        isManuallyLocked = true
        logger.info("Manually locked primary: \(sessionId)")
        Task { await save() }
    }

    func unlock() {
        isManuallyLocked = false
        logger.info("Unlocked primary session")
        Task { await save() }
    }

    func load() async {
        let result = await CloudKitManager.shared.fetchPrimarySession()
        primarySessionId = result.sessionId
        isManuallyLocked = result.locked
        if let id = result.sessionId {
            logger.info("Loaded primary: \(id), locked: \(result.locked)")
        }
    }

    func save() async {
        if let id = primarySessionId {
            await CloudKitManager.shared.setPrimarySession(id, locked: isManuallyLocked)
        } else {
            await CloudKitManager.shared.clearPrimarySession()
        }
    }

    private func autoFillPriority(_ s: SessionState) -> Int {
        switch s.status {
        case .working:    5
        case .needsInput: 4
        case .compacting: 3
        case .idle, .error: 2
        case .completed:  0
        }
    }
}
