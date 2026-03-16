import Foundation
import CloudKit
import Combine
import CodePulseShared

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []
    private var pollTimer: Timer?

    func start() async {
        do {
            try await CloudKitManager.shared.subscribeToChanges()
        } catch {
            print("Failed to subscribe to CloudKit changes: \(error)")
        }
        await fetch()
    }

    func fetch() async {
        do {
            sessions = try await CloudKitManager.shared.fetchAll()
        } catch {
            print("Failed to fetch from CloudKit: \(error)")
        }
    }

    func onRemoteNotification() async {
        await fetch()
    }

    func startPolling() {
        guard pollTimer == nil else { return }
        pollTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.fetch()
            }
        }
    }

    func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }
}
