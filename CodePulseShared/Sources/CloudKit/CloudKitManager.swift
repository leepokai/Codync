import CloudKit
import Foundation
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKit")

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.CodePulse")
    public var database: CKDatabase { container.publicCloudDatabase }

    private init() {}

    // MARK: - Write (macOS)

    /// Save sessions to CloudKit. Uses individual saves (simple + reliable).
    public func saveBatch(_ sessions: [SessionState]) async throws {
        guard !sessions.isEmpty else { return }
        var savedCount = 0
        for session in sessions {
            let record = CKRecordMapper.toRecord(session)
            _ = try await database.save(record)
            savedCount += 1
        }
        logger.info("Saved \(savedCount)/\(sessions.count) sessions to CloudKit")
    }

    // MARK: - Read (iOS)

    public func fetchAll() async throws -> [SessionState] {
        // Fetch only current user's records
        let userID = try await container.userRecordID()
        let ownerRef = CKRecord.Reference(recordID: userID, action: .none)
        let predicate = NSPredicate(format: "creatorUserRecordID == %@", ownerRef)
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: predicate)
        query.sortDescriptors = [NSSortDescriptor(key: "updatedAt", ascending: false)]
        let (results, _) = try await database.records(matching: query, resultsLimit: 20)
        let sessions = results.compactMap { _, result in
            guard case .success(let record) = result else { return nil as SessionState? }
            return CKRecordMapper.fromRecord(record)
        }
        logger.info("Fetched \(sessions.count) sessions from CloudKit")
        return sessions
    }

    // MARK: - Subscribe (iOS)

    public func subscribeToChanges() async throws {
        let subscription = CKQuerySubscription(
            recordType: CKRecordMapper.recordType,
            predicate: NSPredicate(value: true),
            subscriptionID: "session-changes",
            options: [.firesOnRecordCreation, .firesOnRecordUpdate, .firesOnRecordDeletion]
        )
        let info = CKSubscription.NotificationInfo()
        info.shouldSendContentAvailable = true
        subscription.notificationInfo = info
        _ = try await database.save(subscription)
        logger.info("Subscribed to CloudKit changes")
    }

    // MARK: - Cleanup

    public func deleteCompleted(olderThan hours: Int = 24) async throws {
        let cutoff = Date().addingTimeInterval(TimeInterval(-hours * 3600))
        let userID = try await container.userRecordID()
        let ownerRef = CKRecord.Reference(recordID: userID, action: .none)
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            NSPredicate(format: "creatorUserRecordID == %@", ownerRef),
            NSPredicate(format: "status == %@", "completed"),
            NSPredicate(format: "updatedAt < %@", cutoff as NSDate),
        ])
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: predicate)
        let (results, _) = try await database.records(matching: query)
        for (id, _) in results {
            try? await database.deleteRecord(withID: id)
        }
        if !results.isEmpty {
            logger.info("Deleted \(results.count) completed sessions from CloudKit")
        }
    }
}
