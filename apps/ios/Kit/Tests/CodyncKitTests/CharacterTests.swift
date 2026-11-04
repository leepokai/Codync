import Foundation
import Testing
@testable import CodyncKit

// Not compiled on this PC (no Xcode): written to mirror the Kotlin and TypeScript tests.

@Test func botsCarryWhatOnlyTheUserWritesAboutThem() throws {
    let bot = { (extra: String) in
        try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","name":"Reviewer","backend":"claude","cwd":"/tmp"\#(extra)}"#.utf8))
    }
    let old = try bot("")
    #expect(old.verifyCommand == nil && old.backgroundModel == nil && old.personality == nil, "an older host has none")
    let new = try bot(#","verifyCommand":"npm test","backgroundModel":"haiku","personality":{"soul":"honest","persona":"dry"}"#)
    #expect(new.verifyCommand == "npm test" && new.backgroundModel == "haiku")
    #expect(new.personality == Personality(soul: "honest", persona: "dry", heartbeat: ""), "a missing layer is empty")
    let draft = BotDraft(new)
    #expect(draft.verifyCommand == "npm test" && draft.personality?.persona == "dry")
    let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotDraft(old))) as? [String: Any]
    #expect(sent?["verifyCommand"] == nil && sent?["personality"] == nil, "nothing to send to a host without them")
}
