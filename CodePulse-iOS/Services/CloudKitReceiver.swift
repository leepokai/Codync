import Foundation
import CloudKit
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse.ios", category: "CloudKitReceiver")

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []
    private var pollTimer: Timer?

    func start() async {
        do {
            try await CloudKitManager.shared.subscribeToChanges()
            logger.info("CloudKit subscription active")
        } catch {
            logger.error("Failed to subscribe to CloudKit: \(error.localizedDescription)")
        }
        await fetch()
        startPolling()
    }

    func fetch() async {
        do {
            sessions = try await CloudKitManager.shared.fetchAll()
            logger.debug("Fetched \(self.sessions.count) sessions")
        } catch {
            logger.error("CloudKit fetch failed: \(error.localizedDescription)")
        }
    }

    func onRemoteNotification() async {
        logger.debug("Remote notification received")
        await fetch()
    }

    func startPolling() {
        guard pollTimer == nil else { return }
        logger.info("Starting CloudKit polling (5s interval)")
        pollTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.fetch()
            }
        }
    }

    func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
        logger.info("Stopped CloudKit polling")
    }
}
