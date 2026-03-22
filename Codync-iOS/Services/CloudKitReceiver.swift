import Foundation
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "CloudKitReceiver")

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []
    private var isFetching = false

    private static let cacheURL: URL = {
        let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first!
        return dir.appendingPathComponent("codync_sessions.json")
    }()

    init() {
        // Load cached data synchronously so UI has content immediately
        if let data = try? Data(contentsOf: Self.cacheURL),
           let cached = try? JSONDecoder().decode([SessionState].self, from: data) {
            sessions = cached
            logger.info("Loaded \(cached.count) cached sessions")
        }
    }

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

    func fetch(source: String = "unknown", force: Bool = false) async {
        guard force || !isFetching else { return }
        isFetching = true
        defer { isFetching = false }
        do {
            sessions = try await CloudKitManager.shared.fetchAll()
            logger.info("Fetched \(self.sessions.count) sessions (source: \(source))")
            persistCache()
        } catch {
            logger.error("CloudKit fetch failed: \(error.localizedDescription)")
        }
    }

    private func persistCache() {
        let snapshot = sessions
        let url = Self.cacheURL
        Task.detached(priority: .utility) {
            if let data = try? JSONEncoder().encode(snapshot) {
                try? data.write(to: url, options: .atomic)
            }
        }
    }
}
