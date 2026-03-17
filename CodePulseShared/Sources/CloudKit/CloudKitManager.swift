import CloudKit
import Foundation
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKit")

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.CodePulse")
    public var database: CKDatabase { container.privateCloudDatabase }

    private init() {}

    // MARK: - Write (macOS)

    /// Batch save sessions using CKModifyRecordsOperation with .allKeys (force overwrite).
    /// This uses 1 CloudKit request for all records instead of N individual saves.
    public func saveBatch(_ sessions: [SessionState]) async throws {
        guard !sessions.isEmpty else { return }

        // First, try to fetch existing records to get their change tags
        let recordIDs = sessions.map { CKRecord.ID(recordName: $0.sessionId) }
        var existingRecords: [String: CKRecord] = [:]

        // Fetch existing records to get change tags (ignore errors for new records)
        let fetchResults = try? await database.records(for: recordIDs)
        if let fetchResults {
            for (recordID, result) in fetchResults {
                switch result {
                case .success(let record):
                    existingRecords[recordID.recordName] = record
                    logger.debug("Fetched existing record: \(recordID.recordName.prefix(8))")
                case .failure(let error):
                    logger.debug("Record \(recordID.recordName.prefix(8)) not found: \(error.localizedDescription)")
                }
            }
        }
        logger.info("Fetch phase: \(existingRecords.count) existing, \(sessions.count - existingRecords.count) new")
        // File-based debug log (os.Logger not visible from CLI)
        let debugLine = "[\(Date())] CK fetch: \(existingRecords.count) existing, \(sessions.count - existingRecords.count) new\n"
        let debugURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codepulse/cloudkit-debug.log")
        if let handle = try? FileHandle(forWritingTo: debugURL) {
            handle.seekToEndOfFile()
            handle.write(debugLine.data(using: .utf8)!)
            try? handle.close()
        }

        // Build records: update existing or create new
        let records: [CKRecord] = sessions.map { session in
            if let existing = existingRecords[session.sessionId] {
                // Update existing record (preserves change tag)
                CKRecordMapper.updateRecord(existing, with: session)
                return existing
            } else {
                // Create new record
                return CKRecordMapper.toRecord(session)
            }
        }

        let operation = CKModifyRecordsOperation(recordsToSave: records)
        operation.savePolicy = .allKeys
        operation.qualityOfService = .utility

        return try await withCheckedThrowingContinuation { continuation in
            var perRecordErrors: [String] = []
            var savedCount = 0

            operation.perRecordSaveBlock = { recordID, result in
                switch result {
                case .success:
                    savedCount += 1
                case .failure(let error):
                    perRecordErrors.append("\(recordID.recordName.prefix(8)): \(error.localizedDescription)")
                }
            }

            operation.modifyRecordsResultBlock = { result in
                if !perRecordErrors.isEmpty {
                    logger.error("Per-record errors: \(perRecordErrors.joined(separator: "; "))")
                }
                logger.info("Batch save: \(savedCount)/\(sessions.count) succeeded")

                switch result {
                case .success:
                    if savedCount > 0 {
                        continuation.resume()
                    } else {
                        continuation.resume(throwing: CKError(.partialFailure))
                    }
                case .failure(let error):
                    continuation.resume(throwing: error)
                }
            }
            self.database.add(operation)
        }
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
}
