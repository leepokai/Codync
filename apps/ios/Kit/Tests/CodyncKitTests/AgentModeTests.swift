import Foundation
import Testing
@testable import CodyncKit

private func catalog(_ json: String) throws -> AgentModels {
    try JSONDecoder().decode(AgentModels.self, from: Data(json.utf8))
}

@Test func newBotsStartInTheAgentsAutoModeAndAsk() throws {
    let draft = BotDraft(name: "New")
    #expect((draft.permission, draft.mode) == ("ask", "auto"))
    let body = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(draft)) as? [String: Any])
    #expect(body["mode"] as? String == "auto")
}

@Test func approvalsListTheAgentsModesThenApproveAutomatically() throws {
    let codex = try catalog(#"{"models":[],"modes":[{"id":"auto","name":"Auto review","description":"Only ask for actions detected as potentially unsafe"},{"id":"read-only","name":"Read-only"}]}"#)
    var draft = BotDraft(name: "Bot", backend: "codex")
    let (choices, selected) = ApprovalChoice.choices(permission: draft.permission, mode: draft.mode, catalog: codex)
    #expect(choices.map(\.label) == ["Auto review", "Read-only", "Approve automatically"])
    #expect((selected.label, selected.note) == ("Auto review", "Only ask for actions detected as potentially unsafe"))
    draft.setApproval(choices[1].id)
    #expect((draft.permission, draft.mode) == ("ask", "read-only"))
    draft.setApproval(ApprovalChoice.approve)
    #expect((draft.permission, draft.mode) == ("auto", nil))
    // Approve automatically clears the mode on the host too.
    draft.id = "b"
    let body = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
    #expect(body["mode"] is NSNull)
    #expect(ApprovalChoice.choices(permission: draft.permission, mode: draft.mode, catalog: codex).selected.label == "Approve automatically")
}

@Test func aBotNeverShowsAsMoreCarefulThanItIs() throws {
    let codex = try catalog(#"{"models":[],"modes":[{"id":"auto","name":"Auto review"},{"id":"read-only","name":"Read-only"}]}"#)
    // Without a mode it works in the agent's default, and asks.
    let unset = ApprovalChoice.choices(permission: "ask", mode: nil, catalog: codex)
    #expect(unset.all.map(\.label) == ["Auto review", "Read-only", "Ask me", "Approve automatically"])
    #expect(unset.selected.label == "Ask me")
    // A mode the list lacks (or that hasn't loaded) shows as itself.
    let bypass = ApprovalChoice.choices(permission: "ask", mode: "bypassPermissions", catalog: nil)
    #expect(bypass.all.map(\.label) == ["Auto", "Ask me", "bypassPermissions", "Approve automatically"])
    #expect(bypass.selected.label == "bypassPermissions")
}

@Test func anAgentWithoutAutoAsksAndOneStillLoadingOffersAuto() throws {
    let plain = try catalog(#"{"models":[],"modes":[]}"#)
    let asks = ApprovalChoice.choices(permission: "ask", mode: "auto", catalog: plain)
    #expect(asks.all.map(\.label) == ["Ask me", "Approve automatically"])
    #expect(asks.selected.label == "Ask me")
    #expect(ApprovalChoice.choices(permission: "ask", mode: "auto", catalog: nil).all.map(\.label) == ["Auto", "Ask me", "Approve automatically"])

    // Another agent's ids mean nothing: a picked mode becomes its Auto mode.
    var draft = BotDraft(name: "Bot")
    draft.mode = "plan"
    draft.setAgent("codex")
    #expect(draft.mode == "auto")
}
