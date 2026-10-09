import Foundation

public struct MemoryFact: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var content: String
    public var createdAt: Int64
    public var kind: String
    public var memoryType: String
    public var scope: String
    public var topicKey: String?
    public var pinned: Bool
    public var reviewAfter: String?
    public var revisionCount: Int
    public var source: String?
}

public struct MemoryListing: Decodable, Sendable {
    public var facts: [MemoryFact]
    public var total: Int
    public var nextOffset: Int?
}

public struct MemoryDetail: Decodable, Sendable {
    public var fact: MemoryFact
    public var history: History
    public var timeline: Timeline

    public struct History: Decodable, Sendable {
        public var result: String
        public var history_cursor: Int?
    }
    public struct Timeline: Decodable, Sendable { public var result: String }
}

public struct MemoryDraft: Encodable, Sendable {
    public var botId: String
    public var id: String?
    public var title: String
    public var content: String
    public var scope: String
    public var memoryType: String
    public var topicKey: String

    public init(botId: String, fact: MemoryFact? = nil) {
        self.botId = botId
        id = fact?.id
        title = fact?.title ?? ""
        content = fact?.content ?? ""
        scope = fact?.scope ?? "project"
        memoryType = fact?.memoryType ?? "learning"
        topicKey = fact?.topicKey ?? ""
    }
}

extension HostClient {
    public func memory(botId: String, query: String = "", filter: String = "all", offset: Int = 0) async throws -> MemoryListing {
        struct Body: Encodable { var botId: String; var query: String; var filter: String; var offset: Int }
        return try await call("memory", Body(botId: botId, query: query, filter: filter, offset: offset), timeout: 240)
    }

    public func saveMemory(_ draft: MemoryDraft) async throws {
        let _: Empty = try await call("saveMemory", draft, timeout: 240)
    }

    public func memoryAction(_ method: String, botId: String, id: String? = nil, pinned: Bool? = nil) async throws {
        struct Body: Encodable { var botId: String; var id: String?; var pinned: Bool? }
        let _: Empty = try await call(method, Body(botId: botId, id: id, pinned: pinned), timeout: 240)
    }

    public func memoryDetail(botId: String, id: String, cursor: Int? = nil) async throws -> MemoryDetail {
        struct Body: Encodable { var botId: String; var id: String; var historyCursor: Int? }
        return try await call("memoryDetail", Body(botId: botId, id: id, historyCursor: cursor), timeout: 240)
    }

    public func exportMemory(botId: String) async throws -> String {
        struct Response: Decodable { var json: String?; var uploadId: String? }
        let result: Response = try await call("exportMemory", ["botId": botId], timeout: 240)
        if let id = result.uploadId {
            let data = try await readUpload(botId: botId, id: id)
            guard let text = String(data: data, encoding: .utf8) else { throw CocoaError(.fileReadInapplicableStringEncoding) }
            return text
        }
        return result.json ?? ""
    }

    public func importMemory(botId: String, json: String) async throws {
        let data = Data(json.utf8)
        if data.count > 384 * 1024 {
            let id = UUID().uuidString.lowercased()
            try await upload(botId: botId, id: id, name: "engram-backup.json", data: data)
            let _: Empty = try await call("importMemory", ["botId": botId, "uploadId": id], timeout: 240)
        } else {
            let _: Empty = try await call("importMemory", ["botId": botId, "json": json], timeout: 240)
        }
    }
}
