import Foundation
import Testing
@testable import CodyncKit

@Test func decisionNoticeKeepsLegacyPermissionOptionsDecodable() throws {
    // A decoder with the old known fields must still accept a choice notice.
    struct LegacyData: Decodable {
        var text: String?
        var options: [PermissionOption]?
    }
    let wire = Data(#"{"decision":true,"text":"Ship?\n1. Ship\n2. Wait","question":"Ship?","choices":["Ship","Wait"],"recommended":1,"status":"pending","selected":null}"#.utf8)
    let legacy = try JSONDecoder().decode(LegacyData.self, from: wire)
    #expect(legacy.options == nil)
    #expect(legacy.text?.contains("2. Wait") == true)
    let current = try JSONDecoder().decode(EntryData.self, from: wire)
    #expect(current.decision == true)
    #expect(current.choices == ["Ship", "Wait"])
    #expect(current.recommended == 1)
    #expect(current.options == nil)
}

@Test func olderHostNoticesStillDecodeWithoutDecisionFields() throws {
    let data = try JSONDecoder().decode(EntryData.self, from: Data(#"{"text":"Renamed the bot"}"#.utf8))
    #expect(data.decision == nil)
    #expect(data.choices == nil)
}
