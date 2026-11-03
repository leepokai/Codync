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

@Test func iconBadgeAddsThePushedCountToTheOtherComputersUnread() throws {
    let alert = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","body":"Done.","unread":2}"#.utf8))
    #expect(alert.unread == 2)
    func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }
    let stale = try bot(#"{"id":"a","unread":5}"#)
    let other = try bot(#"{"id":"b","unread":3}"#)
    let snapshot = [BotSnapshot(computerId: "this", bot: stale), BotSnapshot(computerId: "other", bot: other)]
    // The sending computer's count replaces its stale snapshot.
    #expect(IconBadge.count(pushed: alert.unread ?? 0, computerId: "this", snapshot: snapshot) == 5)
}
