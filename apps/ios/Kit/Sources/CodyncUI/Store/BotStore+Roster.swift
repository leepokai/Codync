import CodyncKit
import Foundation

extension BotStore {
    public func continueRosterHit(_ hit: RosterSearchHit) { rosterThreadRequest = hit.threadId == nil ? nil : hit }

    public func searchRoster(query: String, bots: [String], before: Int64? = nil) async throws -> RosterSearchPage {
        struct Request: Encodable { let query: String; let botIds: [String]; let beforeSeq: Int64? }
        let client = try await ready()
        let response: RosterSearchPage = try await client.call("searchMessages", Request(query: query, botIds: bots, beforeSeq: before))
        try Task.checkCancellation()
        return response
    }

    public func rosterPreview(_ id: String) async throws -> [Entry] {
        struct Response: Decodable { let entries: [Entry] }
        let client = try await ready()
        let response: Response = try await client.call("rosterPreview", ["botId": id])
        try Task.checkCancellation()
        return response.entries.visibleEntries()
    }

    public func rosterContext(_ hit: RosterSearchHit) async throws -> [Entry] {
        struct Response: Decodable { let entries: [Entry] }
        let client = try await ready()
        let response: Response = try await client.call("messageContext", ["botId": hit.botId, "entryId": hit.id])
        try Task.checkCancellation()
        return response.entries.visibleEntries()
    }
}
