import Foundation
import Testing
@testable import CodyncKit

private func bot(_ extra: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","name":"Ada","backend":"claude","cwd":"/tmp"\#(extra)}"#.utf8))
}

@Test func compactionReadsEachWireValue() throws {
    #expect(try bot(#","compaction":"early""#).compaction == .early)
    #expect(try bot(#","compaction":"nearLimit""#).compaction == .nearLimit)
    #expect(try bot(#","compaction":"off""#).compaction == .off)
    #expect(try bot(#","compaction":120000"#).compaction == .tokens(120_000))
    #expect(try bot(#","compaction":"later""#).compaction == .early, "a newer host's value reads as the default")
    #expect(try bot(#","compaction":null"#).compaction == nil, "no setting where Codync can't summarize")
    #expect(try bot("").compaction == nil, "an older host")

    let sized = try bot(#","compaction":"early","compactionTokens":{"min":50000,"max":80000,"step":10000}"#)
    #expect(sized.compactionTokens?.sizes == [50_000, 60_000, 70_000, 80_000])
    #expect(try bot(#","compaction":"early","compactionTokens":null"#).compactionTokens == nil, "window not known yet")
}

@Test func theDraftSendsCompactionOnlyWhereTheHostOffersIt() throws {
    func sent(_ draft: BotDraft) throws -> [String: Any] {
        try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
    }
    var draft = BotDraft(try bot(#","compaction":"early""#))
    draft.compaction = .tokens(300_000)
    #expect(try sent(draft)["compaction"] as? Int == 300_000)
    draft.compaction = .nearLimit
    #expect(try sent(draft)["compaction"] as? String == "nearLimit")
    #expect(try sent(BotDraft(try bot("")))["compaction"] == nil, "nothing to send to a host without it")
    #expect(try sent(BotDraft())["compaction"] == nil, "a new bot takes the host's default")

    // A change made elsewhere comes through unless this form changed it too.
    let base = BotDraft(try bot(#","compaction":"early""#))
    let fresh = BotDraft(try bot(#","compaction":"off""#))
    #expect(base.rebased(from: base, onto: fresh).compaction == .off)
    #expect(draft.rebased(from: base, onto: fresh).compaction == .nearLimit)
}
