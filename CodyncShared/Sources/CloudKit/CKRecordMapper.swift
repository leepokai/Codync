import CloudKit
import Foundation

public enum CKRecordMapper {
    public static let recordType = "SessionState"

    public static func toRecord(_ session: SessionState, zoneID: CKRecordZone.ID = .default) -> CKRecord {
        let recordID = CKRecord.ID(recordName: session.sessionId, zoneID: zoneID)
        let record = CKRecord(recordType: recordType, recordID: recordID)
        record["sessionId"] = session.sessionId as CKRecordValue
        record["projectName"] = session.projectName as CKRecordValue
        record["gitBranch"] = session.gitBranch as CKRecordValue
        record["status"] = session.status.rawValue as CKRecordValue
        record["model"] = session.model as CKRecordValue
        record["summary"] = session.summary as CKRecordValue
        record["currentTask"] = (session.currentTask ?? "") as CKRecordValue
        record["waitingReason"] = (session.waitingReason?.rawValue ?? "") as CKRecordValue
        record["contextPct"] = session.contextPct as CKRecordValue
        record["costUSD"] = session.costUSD as CKRecordValue
        record["startedAt"] = session.startedAt as CKRecordValue
        record["durationSec"] = session.durationSec as CKRecordValue
        record["deviceId"] = session.deviceId as CKRecordValue
        record["updatedAt"] = session.updatedAt as CKRecordValue
        if let tasksData = try? JSONEncoder().encode(session.truncatedTasks) {
            record["tasks"] = tasksData as CKRecordValue
        }
        return record
    }

    /// Update an existing CKRecord's fields (preserves change tag for conflict-free saves)
    public static func updateRecord(_ record: CKRecord, with session: SessionState) {
        record["sessionId"] = session.sessionId as CKRecordValue
        record["projectName"] = session.projectName as CKRecordValue
        record["gitBranch"] = session.gitBranch as CKRecordValue
        record["status"] = session.status.rawValue as CKRecordValue
        record["model"] = session.model as CKRecordValue
        record["summary"] = session.summary as CKRecordValue
        record["currentTask"] = (session.currentTask ?? "") as CKRecordValue
        record["waitingReason"] = (session.waitingReason?.rawValue ?? "") as CKRecordValue
        record["contextPct"] = session.contextPct as CKRecordValue
        record["costUSD"] = session.costUSD as CKRecordValue
        record["startedAt"] = session.startedAt as CKRecordValue
        record["durationSec"] = session.durationSec as CKRecordValue
        record["deviceId"] = session.deviceId as CKRecordValue
        record["updatedAt"] = session.updatedAt as CKRecordValue
        if let tasksData = try? JSONEncoder().encode(session.truncatedTasks) {
            record["tasks"] = tasksData as CKRecordValue
        }
    }

    public static func fromRecord(_ record: CKRecord) -> SessionState? {
        guard let sessionId = record["sessionId"] as? String,
              let statusRaw = record["status"] as? String,
              let status = SessionStatus(rawValue: statusRaw) else { return nil }
        var tasks: [TaskItem] = []
        if let tasksData = record["tasks"] as? Data {
            tasks = (try? JSONDecoder().decode([TaskItem].self, from: tasksData)) ?? []
        }
        let waitingReason = (record["waitingReason"] as? String).flatMap(WaitingReason.init)
        return SessionState(
            sessionId: sessionId,
            projectName: record["projectName"] as? String ?? "",
            gitBranch: record["gitBranch"] as? String ?? "",
            status: status,
            model: record["model"] as? String ?? "Unknown",
            summary: record["summary"] as? String ?? "",
            currentTask: record["currentTask"] as? String,
            waitingReason: waitingReason,
            tasks: tasks,
            contextPct: record["contextPct"] as? Int ?? 0,
            costUSD: record["costUSD"] as? Double ?? 0,
            startedAt: record["startedAt"] as? Date ?? Date(),
            durationSec: record["durationSec"] as? Int ?? 0,
            deviceId: record["deviceId"] as? String ?? "",
            updatedAt: record["updatedAt"] as? Date ?? Date()
        )
    }
}
