import Foundation
import Combine
import CodePulseShared

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var syncTask: Task<Void, Never>?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .debounce(for: .seconds(2), scheduler: RunLoop.main)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        syncTask?.cancel()
        syncTask = Task {
            for session in sessions {
                let previous = previousStates[session.sessionId]
                do {
                    try await CloudKitManager.shared.saveIfChanged(session, previous: previous)
                    previousStates[session.sessionId] = session
                } catch {
                    print("CloudKit sync error for \(session.sessionId): \(error)")
                }
            }
            try? await CloudKitManager.shared.deleteCompleted()
        }
    }
}
