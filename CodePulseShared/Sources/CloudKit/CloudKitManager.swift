import CloudKit
import Foundation
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKit")

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.CodePulse")
    private var database: CKDatabase { container.privateCloudDatabase }

    private init() {}

    // MARK: - Write (macOS)

    public func save(_ session: SessionState) async throws {
        let record = CKRecordMapper.toRecord(session)
        try await retryOnQuotaExceeded {
            _ = try await self.database.save(record)
        }
        logger.debug("Saved session \(session.sessionId) to CloudKit")
    }

    public func saveIfChanged(_ session: SessionState, previous: SessionState?) async throws {
        guard session.updatedAt != previous?.updatedAt else { return }
        try await save(session)
    }

    // MARK: - Read (iOS)

    public func fetchAll() async throws -> [SessionState] {
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
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
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
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

    // MARK: - Retry Logic

    private func retryOnQuotaExceeded(maxRetries: Int = 3, operation: @Sendable () async throws -> Void) async throws {
        for attempt in 0..<maxRetries {
            do {
                try await operation()
                return
            } catch let error as CKError where error.code == .requestRateLimited || error.code == .zoneBusy || error.code == .serviceUnavailable {
                let retryAfter = error.retryAfterSeconds ?? Double(attempt + 1) * 5
                logger.warning("CloudKit rate limited (attempt \(attempt + 1)/\(maxRetries)), retrying after \(retryAfter)s")
                try await Task.sleep(for: .seconds(min(retryAfter, 30))) // cap at 30s
            } catch let error as CKError where error.code == .quotaExceeded {
                let retryAfter = error.retryAfterSeconds ?? 60
                logger.warning("CloudKit quota exceeded (attempt \(attempt + 1)/\(maxRetries)), retrying after \(min(retryAfter, 60))s")
                try await Task.sleep(for: .seconds(min(retryAfter, 60)))
            }
        }
        // Final attempt, let error propagate
        try await operation()
    }
}
