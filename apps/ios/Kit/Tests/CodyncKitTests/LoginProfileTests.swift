import Foundation
import Testing
@testable import CodyncKit

private func decode<T: Decodable>(_ json: String) throws -> T { try JSONDecoder().decode(T.self, from: Data(json.utf8)) }

private func updateBody(_ draft: BotDraft) throws -> [String: Any] {
    try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
}

private let backendJSON = #""id":"claude","name":"Claude Code","available":true,"command":"claude","installHint":"""#

@Test func backendDecodesItsLoginProfiles() throws {
    let backend: Backend = try decode("""
    {\(backendJSON),"loginProfiles":[{"id":"a1","name":"me@example.com","signedIn":true},{"id":"a2","name":null,"signedIn":false},{"id":"a3"}]}
    """)
    #expect(backend.loginProfiles?.map(\.id) == ["a1", "a2", "a3"])
    #expect(backend.loginProfiles?.first?.name == "me@example.com" && backend.loginProfiles?.first?.signedIn == true)
    #expect(backend.loginProfiles?[1].name == nil && backend.loginProfiles?[2].signedIn == false)
    // A bot picks a signed-in one; its own stays listed even signed out.
    #expect(backend.loginProfileChoices(picked: nil).map(\.id) == ["a1"])
    #expect(backend.loginProfileChoices(picked: "a2").map(\.id) == ["a1", "a2"])
}

@Test func olderHostsHaveNoLoginProfiles() throws {
    let older: Backend = try decode("{\(backendJSON)}")
    #expect(older.loginProfiles == nil && older.loginProfileChoices(picked: nil).isEmpty)
    // The key alone says the agent can have login profiles.
    let none: Backend = try decode("{\(backendJSON),\"loginProfiles\":[]}")
    #expect(none.loginProfiles == [])
}

@Test func botLoginProfileIsSentAndClearedWithNull() throws {
    let bot: Bot = try decode(#"{"id":"b","backend":"claude","loginProfile":"a1"}"#)
    #expect(bot.loginProfile == "a1")
    var draft = BotDraft(bot)
    #expect(try updateBody(draft)["loginProfile"] as? String == "a1")
    // Back to the computer's own login: an explicit null, so the host clears it.
    draft.loginProfile = nil
    #expect(try updateBody(draft)["loginProfile"] is NSNull)
    #expect((try decode(#"{"id":"b"}"#) as Bot).loginProfile == nil)
    #expect((try decode(#"{"id":"b","loginProfile":7}"#) as Bot).loginProfile == nil)
}

@Test func switchingAgentClearsTheLoginProfile() {
    var draft = BotDraft(name: "Lead")
    draft.loginProfile = "a1"
    draft.setAgent("claude")
    #expect(draft.loginProfile == "a1")
    draft.setAgent("codex")
    #expect(draft.loginProfile == nil)
}

@Test func backendDecodesTheDefaultLoginProfileAndLabels() throws {
    let backend: Backend = try decode("""
    {\(backendJSON),"loginProfileSignIn":"browser","defaultLoginProfile":{"name":"Work","label":"Work","email":"me@example.com"},
     "loginProfiles":[{"id":"a1","name":"Side","label":"Side","email":"side@example.com","signedIn":true},{"id":"a2","name":"x@example.com","email":"x@example.com","label":null,"signedIn":true}]}
    """)
    #expect(backend.defaultLoginProfile?.name == "Work" && backend.defaultLoginProfile?.label == "Work" && backend.defaultLoginProfile?.email == "me@example.com")
    #expect(backend.loginProfiles?.first?.label == "Side" && backend.loginProfiles?.first?.email == "side@example.com")
    #expect(backend.loginProfiles?.last?.label == nil && backend.loginProfilesSignInInBrowser)
    // Unnamed, unknown and odd shapes still decode; older hosts send none.
    let bare: Backend = try decode("{\(backendJSON),\"defaultLoginProfile\":{\"name\":null,\"label\":7}}")
    #expect(bare.defaultLoginProfile != nil && bare.defaultLoginProfile?.name == nil && bare.defaultLoginProfile?.label == nil)
    #expect((try decode("{\(backendJSON)}") as Backend).defaultLoginProfile == nil)
}

@Test func loginProfilesDecodeTheirKindAndVariables() throws {
    let backend: Backend = try decode("""
    {\(backendJSON),"loginProfileSignIn":null,"loginKeys":["ANTHROPIC_API_KEY"],
     "loginProfiles":[{"id":"k1","kind":"apiKey","name":"Work key","signedIn":true,"vars":["ANTHROPIC_API_KEY","ANTHROPIC_BASE_URL"]},
                      {"id":"k2","kind":"apiKey","signedIn":false,"vars":[]},{"id":"a1","kind":"account"},{"id":"a2","kind":"later","vars":7}]}
    """)
    let profiles = try #require(backend.loginProfiles)
    #expect(profiles.map(\.kind) == [.apiKey, .apiKey, .account, .account])
    #expect(profiles[0].vars == ["ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL"] && profiles[2].vars.isEmpty && profiles[3].vars.isEmpty)
    // null: the agent keeps one sign-in of its own, so it takes API key profiles only.
    #expect(backend.loginProfileSignIn == nil && !backend.loginProfilesSignInInBrowser)
    #expect((try decode("{\(backendJSON),\"loginProfileSignIn\":\"terminal\"}") as Backend).loginProfileSignIn == "terminal")
    #expect(backend.loginKeys == ["ANTHROPIC_API_KEY"])
    #expect((try decode("{\(backendJSON)}") as Backend).loginKeys == nil)
    // A bot picks an API key profile once it has a key.
    #expect(backend.loginProfileChoices(picked: nil).map(\.id) == ["k1"])
}

@Test func keyFormStartsWithSavedThenSuggestedThenOneEmptyVariable() {
    let saved = LoginProfile(id: "k1", kind: .apiKey, vars: ["A", "B"])
    #expect(saved.keyNames(suggested: ["OPENAI_API_KEY"]) == ["A", "B"])
    let new = LoginProfile(id: "k2", kind: .apiKey)
    #expect(new.keyNames(suggested: ["OPENAI_API_KEY"]) == ["OPENAI_API_KEY"])
    #expect(new.keyNames(suggested: []) == [""])
}

@Test func saveSendsTypedValuesAndAnEmptyOneForARemovedVariable() {
    let rows = [(name: "A", value: ""), (name: "B", value: "two"), (name: " C ", value: "three"), (name: "", value: "lost"), (name: "D", value: "")]
    #expect(LoginProfile.keysToSave(rows: rows) == ["B": "two", "C": "three"])
    #expect(LoginProfile.keysToSave(rows: rows, removed: ["OLD"]) == ["OLD": "", "B": "two", "C": "three"])
    // A removed name typed again keeps the new value.
    #expect(LoginProfile.keysToSave(rows: rows, removed: ["B"]) == ["B": "two", "C": "three"])
    #expect(LoginProfile.keysToSave(rows: [(name: "A", value: "")]).isEmpty)
}
