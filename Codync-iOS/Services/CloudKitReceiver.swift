import Foundation
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "CloudKitReceiver")

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []
    @Published var syncError: SyncError?
    private var isFetching = false

    enum SyncError {
        case quotaExceeded
        case networkUnavailable
        case other(String)

        var title: String {
            switch self {
            case .quotaExceeded: "iCloud Storage Full"
            case .networkUnavailable: "No Network"
            case .other: "Sync Error"
            }
        }
    }

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
            syncError = nil
            logger.info("Fetched \(self.sessions.count) sessions (source: \(source))")
            persistCache()
        } catch let error as CKError where error.code == .quotaExceeded {
            syncError = .quotaExceeded
            logger.error("CloudKit quota exceeded")
        } catch let error as CKError where error.code == .networkUnavailable || error.code == .networkFailure {
            syncError = .networkUnavailable
            logger.error("CloudKit network unavailable")
        } catch {
            syncError = .other(error.localizedDescription)
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
