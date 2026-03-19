import CloudKit
import Foundation
import os

private let logger = Logger(subsystem: "com.pokai.Codync", category: "CloudKit")

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.Codync")
    public var database: CKDatabase { container.privateCloudDatabase }

    /// Custom zone — required for reliable silent push notifications.
    /// Default zone does not support CKRecordZoneSubscription or change tokens.
    public static let zoneID = CKRecordZone.ID(zoneName: "Sessions", ownerName: CKCurrentUserDefaultName)

    private init() {}

    // MARK: - Zone Setup

    /// Create the custom zone if it doesn't exist. Idempotent — safe to call on every launch.
    public func ensureZoneExists() async throws {
        let zone = CKRecordZone(zoneID: Self.zoneID)
        _ = try await database.save(zone)
    }

    // MARK: - Write (macOS)

    /// Save sessions to CloudKit custom zone. Fetches existing records first to avoid conflicts.
    public func saveBatch(_ sessions: [SessionState]) async throws {
        guard !sessions.isEmpty else { return }

        let recordIDs = sessions.map { CKRecord.ID(recordName: $0.sessionId, zoneID: Self.zoneID) }
        var existing: [String: CKRecord] = [:]
        let results = (try? await database.records(for: recordIDs)) ?? [:]
        for (id, result) in results {
            if case .success(let record) = result {
                existing[id.recordName] = record
            }
        }

        var savedCount = 0
        for session in sessions {
            let record: CKRecord
            if let server = existing[session.sessionId] {
                CKRecordMapper.updateRecord(server, with: session)
                record = server
            } else {
                record = CKRecordMapper.toRecord(session, zoneID: Self.zoneID)
            }
            _ = try await database.save(record)
            savedCount += 1
        }
        logger.info("Saved \(savedCount)/\(sessions.count) sessions to CloudKit")
    }

    // MARK: - Read (iOS)

    public func fetchAll() async throws -> [SessionState] {
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
        query.sortDescriptors = [NSSortDescriptor(key: "updatedAt", ascending: false)]
        let (results, _) = try await database.records(
            matching: query, inZoneWith: Self.zoneID, resultsLimit: 20
        )
        let sessions = results.compactMap { _, result in
            guard case .success(let record) = result else { return nil as SessionState? }
            return CKRecordMapper.fromRecord(record)
        }
        logger.info("Fetched \(sessions.count) sessions from CloudKit")
        return sessions
    }

    // MARK: - Subscribe (iOS)

    private static let subscriptionKey = "codync_ck_zone_subscription_v2"

    public func subscribeToChanges() async throws {
        let subscriptionID = "session-zone-changes"

        if UserDefaults.standard.bool(forKey: Self.subscriptionKey) {
            logger.info("CloudKit zone subscription already created (cached)")
            return
        }

        // Delete old default-zone subscription if it exists
        try? await database.deleteSubscription(withID: "session-changes")

        if (try? await database.subscription(for: subscriptionID)) != nil {
            UserDefaults.standard.set(true, forKey: Self.subscriptionKey)
            logger.info("CloudKit zone subscription already exists")
            return
        }

        // CKRecordZoneSubscription on custom zone — this is the reliable pattern
        let subscription = CKRecordZoneSubscription(
            zoneID: Self.zoneID,
            subscriptionID: subscriptionID
        )
        let info = CKSubscription.NotificationInfo()
        info.shouldSendContentAvailable = true
        subscription.notificationInfo = info
        _ = try await database.save(subscription)
        UserDefaults.standard.set(true, forKey: Self.subscriptionKey)
        logger.info("Created CloudKit zone subscription")
    }

    // MARK: - Delete

    public func deleteByIds(_ sessionIds: [String]) async throws {
        guard !sessionIds.isEmpty else { return }
        let recordIDs = sessionIds.map { CKRecord.ID(recordName: $0, zoneID: Self.zoneID) }
        for id in recordIDs {
            try? await database.deleteRecord(withID: id)
        }
        logger.info("Deleted \(sessionIds.count) session records from CloudKit")
    }

    public func deleteOrphans(activeSessionIds: Set<String>) async throws {
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
        let (results, _) = try await database.records(
            matching: query, inZoneWith: Self.zoneID, resultsLimit: 50
        )
        var deletedCount = 0
        for (id, result) in results {
            guard case .success = result else { continue }
            if !activeSessionIds.contains(id.recordName) {
                try? await database.deleteRecord(withID: id)
                deletedCount += 1
            }
        }
        if deletedCount > 0 {
            logger.info("Cleaned up \(deletedCount) orphan records from CloudKit")
        }
    }
}
