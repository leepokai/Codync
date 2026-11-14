import Foundation
import Testing
@testable import CodyncKit

private func decode<T: Decodable>(_ json: String) throws -> T { try JSONDecoder().decode(T.self, from: Data(json.utf8)) }

private func updateBody(_ draft: BotDraft) throws -> [String: Any] {
    try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
}

private let backendJSON = #""id":"claude","name":"Claude Code","available":true,"command":"claude","installHint":"""#

@Test func backendDecodesItsAccounts() throws {
    let backend: Backend = try decode("""
    {\(backendJSON),"accounts":[{"id":"a1","name":"me@example.com","signedIn":true},{"id":"a2","name":null,"signedIn":false},{"id":"a3"}]}
    """)
    #expect(backend.accounts?.map(\.id) == ["a1", "a2", "a3"])
    #expect(backend.accounts?.first?.name == "me@example.com" && backend.accounts?.first?.signedIn == true)
    #expect(backend.accounts?[1].name == nil && backend.accounts?[2].signedIn == false)
    // A bot picks a signed-in one; its own stays listed even signed out.
    #expect(backend.accountChoices(picked: nil).map(\.id) == ["a1"])
    #expect(backend.accountChoices(picked: "a2").map(\.id) == ["a1", "a2"])
}

@Test func agentsWithoutAccountsAndOlderHostsHaveNone() throws {
    let older: Backend = try decode("{\(backendJSON)}")
    #expect(older.accounts == nil && older.accountChoices(picked: nil).isEmpty)
    // The key alone says the agent can have accounts.
    let none: Backend = try decode("{\(backendJSON),\"accounts\":[]}")
    #expect(none.accounts == [])
}

@Test func botAccountIsSentAndClearedWithNull() throws {
    let bot: Bot = try decode(#"{"id":"b","backend":"claude","account":"a1"}"#)
    #expect(bot.account == "a1")
    var draft = BotDraft(bot)
    #expect(try updateBody(draft)["account"] as? String == "a1")
    // Back to the computer's own login: an explicit null, so the host clears it.
    draft.account = nil
    #expect(try updateBody(draft)["account"] is NSNull)
    #expect((try decode(#"{"id":"b"}"#) as Bot).account == nil)
    #expect((try decode(#"{"id":"b","account":7}"#) as Bot).account == nil)
}

@Test func switchingAgentClearsTheAccount() {
    var draft = BotDraft(name: "Lead")
    draft.account = "a1"
    draft.setAgent("claude")
    #expect(draft.account == "a1")
    draft.setAgent("codex")
    #expect(draft.account == nil)
}
