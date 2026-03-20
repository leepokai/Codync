import Foundation
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "CloudKitReceiver")

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []
    private var isFetching = false

    func start() async {
        // Ensure custom zone exists before subscribing
        do {
            try await CloudKitManager.shared.ensureZoneExists()
        } catch {
            logger.warning("Zone creation failed: \(error.localizedDescription)")
        }
        do {
            try await CloudKitManager.shared.subscribeToChanges()
            logger.info("CloudKit zone subscription active")
        } catch {
            logger.error("Failed to subscribe to CloudKit: \(error.localizedDescription)")
        }
        await fetch(source: "initial")
    }

    func fetch(source: String = "unknown") async {
        guard !isFetching else { return }
        isFetching = true
        defer { isFetching = false }
        do {
            sessions = try await CloudKitManager.shared.fetchAll()
            logger.info("Fetched \(self.sessions.count) sessions (source: \(source))")
        } catch {
            logger.error("CloudKit fetch failed: \(error.localizedDescription)")
        }
    }
}
