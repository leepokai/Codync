import Foundation
import Testing
@testable import CodyncKit

@Test func pushContentAcceptsAnOptionalStatusSubtitle() throws {
    let current = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","subtitle":"Task complete","body":"All tests passed."}"#.utf8))
    #expect(current.subtitle == "Task complete")
    #expect(current.body == "All tests passed.")
    let olderHost = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","body":"All tests passed."}"#.utf8))
    #expect(olderHost.subtitle == nil)
    #expect(olderHost.from == nil)
    let group = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Bob in Team","body":"Done.","from":"b"}"#.utf8))
    #expect(group.from == "b")
}

@Test func iconBadgeAddsEveryComputersLatestAlertToTheRestOfTheRoster() throws {
    let alert = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","body":"Done.","unread":2}"#.utf8))
    #expect(alert.unread == 2)
    func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }
    let snapshot = [BotSnapshot(computerId: "a", bot: try bot(#"{"id":"x","unread":5}"#)),
                    BotSnapshot(computerId: "b", bot: try bot(#"{"id":"y","unread":0}"#)),
                    BotSnapshot(computerId: "c", bot: try bot(#"{"id":"z","unread":3}"#))]
    // A's alert replaces its stale snapshot.
    #expect(IconBadge.count(pushed: ["a": 2], snapshot: snapshot) == 5)
    // B's alert keeps A's earlier one instead of falling back to A's snapshot.
    #expect(IconBadge.count(pushed: ["a": 2, "b": 1], snapshot: snapshot) == 6)
}
