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

@Test func pushContentCarriesTheComputersUnreadTotal() throws {
    let current = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","body":"Done.","unread":2}"#.utf8))
    #expect(current.unread == 2)
    let olderHost = try JSONDecoder().decode(PushAlert.self, from: Data(#"{"title":"Reviewer","body":"Done."}"#.utf8))
    #expect(olderHost.unread == nil)
}
