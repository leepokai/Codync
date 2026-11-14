import Foundation
import Testing
@testable import CodyncKit

@Test func modelCatalogDecodesAgentNamesAndIDs() throws {
    let data = Data(#"{"models":[{"id":"opus[1m]","name":"Agent model","description":"From the agent"}],"currentModelId":"opus[1m]"}"#.utf8)
    let catalog = try JSONDecoder().decode(AgentModels.self, from: data)
    #expect(catalog.models.first?.id == "opus[1m]")
    #expect(catalog.models.first?.name == "Agent model")
    #expect(catalog.currentModelId == "opus[1m]")
}

@Test func modelDefaultExplicitlyClearsSavedOverride() throws {
    var draft = BotDraft(name: "Example", cwd: "/tmp")
    draft.id = "bot-id"
    let data = try JSONEncoder().encode(BotUpdate(draft: draft))
    let body = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(body["model"] is NSNull)
    #expect(body["id"] as? String == "bot-id")
    #expect(body["name"] as? String == "Example")
    draft.model = "agent-model"
    let selected = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
    #expect(selected["model"] as? String == "agent-model")
}

@Test func modelOptionsDecodeAndRoundTripThroughDrafts() throws {
    let data = Data(#"{"models":[{"id":"grok-4.7","name":"Grok 4.7","options":[{"id":"fast","name":"Fast","default":"true","values":[{"id":"false","name":"Off"},{"id":"true","name":"On"}]}]},{"id":"old","name":"Old host"}]}"#.utf8)
    let catalog = try JSONDecoder().decode(AgentModels.self, from: data)
    #expect(catalog.models.first?.options?.first?.values.map(\.name) == ["Off", "On"])
    #expect(catalog.models.last?.options == nil)
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","modelOptions":{"fast":"false"}}"#.utf8))
    #expect(bot.modelOptions == ["fast": "false"])
    let body = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: BotDraft(bot)))) as? [String: Any])
    #expect(body["modelOptions"] as? [String: String] == ["fast": "false"])
}

@Test func modelCatalogReportsFastModeAndOlderHostsOmitIt() throws {
    let offered = try JSONDecoder().decode(AgentModels.self, from: Data(#"{"models":[],"fastMode":true}"#.utf8))
    #expect(offered.fastMode == true)
    let older = try JSONDecoder().decode(AgentModels.self, from: Data(#"{"models":[]}"#.utf8))
    #expect(older.fastMode == nil)
}

@Test func botFastModeReachesTheSettingsDraft() throws {
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","backend":"codex","fastMode":true}"#.utf8))
    #expect(bot.fastMode)
    let draft = BotDraft(bot)
    let body = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
    #expect(body["fastMode"] as? Bool == true)
    let older = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b"}"#.utf8))
    #expect(!older.fastMode)
}

@Test func composerOffersFastModeUntilTheAgentSaysItHasNone() throws {
    func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }
    #expect(try bot(#"{"id":"b"}"#).fastModeShown)
    #expect(try !bot(#"{"id":"b","fastModeSupport":"unsupported"}"#).fastModeShown)
    #expect(try bot(#"{"id":"b","fastModeSupport":"offered"}"#).fastModeShown)
    #expect(try bot(#"{"id":"b","fastMode":true,"fastModeSupport":"unsupported"}"#).fastModeShown)
    #expect(try !bot(#"{"id":"g","kind":"group","fastModeSupport":"offered"}"#).fastModeShown)
    let refused = try bot(#"{"id":"b","fastMode":true,"fastModeSupport":"refused","fastModeIssue":"requires extra usage"}"#)
    #expect(refused.fastModeState == .refused)
    #expect(refused.fastModeRefusal?.contains("requires extra usage") == true)
}

@Test func settingsDraftTakesChangesMadeElsewhereAndKeepsItsOwnEdits() {
    var base = BotDraft(name: "Lead", avatarColor: "red", avatarShape: "blob")
    base.model = "sonnet"
    var draft = base
    draft.notify = false
    var fresh = base
    fresh.model = "opus"
    fresh.name = "Boss"
    var expected = fresh
    expected.notify = false
    #expect(draft.rebased(from: base, onto: fresh) == expected)
    // A field edited here stays, even when the host changed it too.
    draft.model = "haiku"
    #expect(draft.rebased(from: base, onto: fresh).model == "haiku")
}
