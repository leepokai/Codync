import Foundation
import Testing
@testable import CodyncKit

@Test func contextUsageKeepsUnknownDistinctFromAnEmptySession() throws {
    func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }
    #expect(try bot(#"{"id":"b"}"#).contextUsage == nil)
    #expect(try bot(#"{"id":"b","contextUsage":null}"#).contextUsage == nil)
    let reported = try bot(#"{"id":"b","contextUsage":{"usedTokens":0,"maxTokens":200000,"percent":0,"level":"normal"}}"#)
    let empty = try #require(reported.contextUsage)
    #expect(empty.usedTokens == 0)
    #expect(empty.maxTokens == 200_000)
    #expect(empty.percent == 0)
}

@Test func contextUsagePreservesTheHostsSnapshotAndFutureLevels() throws {
    let data = Data(#"{"id":"b","contextUsage":{"usedTokens":170000,"maxTokens":200000,"percent":85,"level":"future"}}"#.utf8)
    let bot = try JSONDecoder().decode(Bot.self, from: data)
    let usage = try #require(bot.contextUsage)
    #expect(usage.usedTokens == 170_000)
    #expect(usage.percent == 85)
    #expect(usage.level == "future")
    let roundTrip = try JSONDecoder().decode(Bot.self, from: JSONEncoder().encode(bot))
    #expect(roundTrip.contextUsage == usage)
}
