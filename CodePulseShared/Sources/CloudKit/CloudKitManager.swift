import CloudKit
import Foundation

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.CodePulse")
    private var database: CKDatabase { container.privateCloudDatabase }

    private init() {}

    public func save(_ session: SessionState) async throws {
        let record = CKRecordMapper.toRecord(session)
        _ = try await database.save(record)
    }

    public func saveIfChanged(_ session: SessionState, previous: SessionState?) async throws {
        guard session.updatedAt != previous?.updatedAt else { return }
        try await save(session)
    }

    public func fetchAll() async throws -> [SessionState] {
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
        query.sortDescriptors = [NSSortDescriptor(key: "updatedAt", ascending: false)]
        let (results, _) = try await database.records(matching: query, resultsLimit: 20)
        return results.compactMap { _, result in
            guard case .success(let record) = result else { return nil }
            return CKRecordMapper.fromRecord(record)
        }
    }

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
    }

    public func deleteCompleted(olderThan hours: Int = 24) async throws {
        let cutoff = Date().addingTimeInterval(TimeInterval(-hours * 3600))
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            NSPredicate(format: "status == %@", "completed"),
            NSPredicate(format: "updatedAt < %@", cutoff as NSDate),
        ])
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: predicate)
        let (results, _) = try await database.records(matching: query)
        for (id, _) in results {
            try await database.deleteRecord(withID: id)
        }
    }
}
