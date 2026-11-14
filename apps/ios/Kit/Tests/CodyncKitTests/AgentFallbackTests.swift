import Foundation
import Testing
@testable import CodyncKit

private func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }

private func updateBody(_ draft: BotDraft) throws -> [String: Any] {
    try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
}

@Test func botDecodesItsFallbacksAndWhereItRuns() throws {
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    let b = try bot("""
    {"id":"b","backend":"claude","fallbacks":[{"backend":"codex","model":"gpt-5.5","modelOptions":{"effort":"high"}},{"backend":"gemini","model":null}],
     "selfFallbacks":false,"agentStep":{"step":1,"agent":"codex","model":"gpt-5.5","since":1,"reason":"usageLimit"},
     "resting":[{"agent":"claude","until":\(soon),"reason":"usageLimit"},{"agent":"gemini","until":1,"reason":"signedOut"}]}
    """)
    #expect(b.fallbacks?.map(\.backend) == ["codex", "gemini"])
    #expect(b.fallbacks?.first?.modelOptions == ["effort": "high"])
    #expect(b.fallbacks?.last?.model == nil && b.fallbacks?.last?.modelOptions == [:])
    #expect(b.selfFallbacks == false)
    #expect(b.agentStep?.agent == "codex" && b.agentStep?.reason == "usageLimit")
    #expect(b.rest(of: AgentLogin(backend: "claude"))?.line.hasPrefix("Usage resets ") == true)
    // A rest whose time has passed is over, whatever the host last said.
    #expect(b.rest(of: AgentLogin(backend: "gemini")) == nil)
    let draft = BotDraft(b)
    let body = try updateBody(draft)
    let sent = try #require(body["fallbacks"] as? [[String: Any]])
    #expect(sent.map { $0["backend"] as? String } == ["codex", "gemini"])
    #expect(sent.last?["modelOptions"] as? [String: String] == [:])
    #expect(body["selfFallbacks"] as? Bool == false)
}

@Test func olderHostsAndOddShapesStillDecode() throws {
    let older = try bot(#"{"id":"b"}"#)
    #expect(older.fallbacks == nil && older.selfFallbacks == nil && older.agentStep == nil && older.resting.isEmpty)
    // Nothing the host doesn't have is sent back.
    let body = try updateBody(BotDraft(older))
    #expect(body["fallbacks"] == nil && body["selfFallbacks"] == nil)
    let odd = try bot(#"{"id":"b","fallbacks":"x","agentStep":{"step":"one"},"resting":[1]}"#)
    #expect(odd.fallbacks == nil && odd.agentStep == nil && odd.resting.isEmpty)
}

@Test func agentSwitchNoticesAndAddFallbackDecode() throws {
    func entry(_ data: String) throws -> Entry {
        try JSONDecoder().decode(Entry.self, from: Data(#"{"id":"e","seq":1,"botId":"b","rev":1,"kind":"notice","turn":1,"createdAt":0,"updatedAt":0,"data":\#(data)}"#.utf8))
    }
    let line = try entry(#"{"text":"Claude Code reached its usage limit (resets in 2 h). Codex carried on.","style":"info","agentSwitch":{"from":"claude","to":"codex","back":false,"reason":"usageLimit","resetsAt":5,"carried":"This turn so far"}}"#)
    #expect(line.isChat && line.data.agentSwitch?.to == "codex" && line.data.agentSwitch?.carried == "This turn so far")
    let back = try entry(#"{"text":"Back on Claude Code.","style":"info","agentSwitch":{"from":"codex","to":"claude","back":true,"reason":null,"resetsAt":null,"carried":null}}"#)
    #expect(back.data.agentSwitch?.back == true && back.data.agentSwitch?.carried == nil)
    #expect(try entry(#"{"text":"x","agentSwitch":{"to":7,"newer":[1]}}"#).data.agentSwitch?.to == nil)
    #expect(try entry(#"{"text":"Usage limit","style":"error","addFallback":true}"#).data.addFallback == true)
}

@Test func choosingAFallbackAsTheAgentDropsItFromTheList() {
    var draft = BotDraft(name: "Lead")
    draft.fallbacks = [BotFallback(backend: "codex"), BotFallback(backend: "gemini")]
    draft.setAgent("codex")
    #expect(draft.backend == "codex" && draft.fallbacks?.map(\.backend) == ["gemini"])
    // A change made elsewhere (the bot set its own list) reaches an open form that didn't touch it.
    let base = draft
    var fresh = base
    fresh.fallbacks = [BotFallback(backend: "opencode")]
    var edited = draft
    edited.notify = false
    #expect(edited.rebased(from: base, onto: fresh).fallbacks == fresh.fallbacks)
}

@Test func switchingHarnessResetsModelAndEffortTogether() {
    var draft = BotDraft(name: "Reviewer")
    draft.backend = "claude"
    draft.model = "opus"
    draft.modelOptions = ["effort": "high"]
    draft.setAgent("claude")
    #expect(draft.model == "opus" && draft.modelOptions == ["effort": "high"])
    draft.setAgent("codex")
    #expect(draft.model == nil && draft.modelOptions.isEmpty)
    draft.modelOptions = ["reasoning_effort": "low"]
    draft.setAgent("claude")
    #expect(draft.model == nil && draft.modelOptions.isEmpty)
}

@Test func botsMixAgentsWhenTheyRunMoreThanOneCountingFallbacksNotGroups() throws {
    let claude = try bot(#"{"id":"a","backend":"claude"}"#)
    let onCodex = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}}"#)
    let group = try bot(#"{"id":"g","kind":"group","backend":"codex"}"#)
    #expect(onCodex.runningAgent == "codex")
    #expect(try ![claude, bot(#"{"id":"c","backend":"claude"}"#), group].mixesAgents)
    #expect([claude, onCodex].mixesAgents)
}

private let claudeJSON = #"{"id":"claude","name":"Claude Code","available":true,"command":"claude","installHint":"","accounts":[{"id":"a1","signedIn":true},{"id":"a2","signedIn":false},{"id":"a3","signedIn":true}]}"#

@Test func fallbackStepsDecodeAndSendTheirAccount() throws {
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    let b = try bot("""
    {"id":"b","backend":"claude","fallbacks":[{"backend":"claude","account":"a1","model":null},{"backend":"codex","account":7}],
     "agentStep":{"step":1,"agent":"claude","account":"a1","since":1},
     "resting":[{"agent":"claude","until":\(soon),"reason":"usageLimit"},{"agent":"claude","account":"a3","until":\(soon),"reason":"rateLimited"}]}
    """)
    #expect(b.fallbacks?.map(\.account) == ["a1", nil])
    #expect(b.stepLogin == AgentLogin(backend: "claude", account: "a1") && b.stepLogin != b.login)
    // A rest belongs to one login, not to the agent.
    #expect(b.rest(of: b.login)?.reason == "usageLimit")
    #expect(b.rest(of: AgentLogin(backend: "claude", account: "a3"))?.reason == "rateLimited")
    #expect(b.rest(of: AgentLogin(backend: "claude", account: "a1")) == nil)
    let sent = try #require(updateBody(BotDraft(b))["fallbacks"] as? [[String: Any]])
    // The default login sends null: a step without the key would keep its saved account on the host.
    #expect(sent[0]["account"] as? String == "a1" && sent[1]["account"] is NSNull)
}

@Test func theBannerTellsTwoLoginsOfOneAgentApart() throws {
    let name: (AgentLogin, Bool) -> String = { login, own in
        login.backend + (login.account.map { " (\($0))" } ?? (own ? " (Default account)" : ""))
    }
    let onAccount = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"claude","account":"Work","since":1}}"#)
    #expect(onAccount.stepBanner(name: name) == "Using claude (Work) while claude (Default account) can't answer")
    let onCodex = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}}"#)
    #expect(onCodex.stepBanner(name: name) == "Using codex while claude can't answer")
    #expect(try bot(#"{"id":"b","backend":"claude"}"#).stepBanner(name: name) == nil)
}

@Test func freeLoginsAreTheDefaultAndSignedInAccountsTheChainLacks() throws {
    let claude = try JSONDecoder().decode(Backend.self, from: Data(claudeJSON.utf8))
    #expect(claude.freeLogins(taken: []) == [nil, "a1", "a3"])
    // The bot on the default login, a step on a1: only a3 is left; another agent's logins don't count.
    let taken = [AgentLogin(backend: "claude"), AgentLogin(backend: "claude", account: "a1"), AgentLogin(backend: "codex", account: "a3")]
    #expect(claude.freeLogins(taken: taken) == ["a3"])
    #expect(claude.freeLogins(taken: taken + [AgentLogin(backend: "claude", account: "a3")]).isEmpty)
    let plain = try JSONDecoder().decode(Backend.self, from: Data(#"{"id":"codex","name":"Codex","available":true,"command":"codex","installHint":""}"#.utf8))
    #expect(plain.freeLogins(taken: []) == [nil] && plain.freeLogins(taken: [AgentLogin(backend: "codex")]).isEmpty)
}

@Test func switchingAgentKeepsItsStepsOnOtherAccounts() {
    var draft = BotDraft(name: "Lead")
    draft.fallbacks = [BotFallback(backend: "codex"), BotFallback(backend: "codex", account: "a1"), BotFallback(backend: "gemini")]
    draft.setAgent("codex")
    #expect(draft.fallbacks?.map(\.login) == [AgentLogin(backend: "codex", account: "a1"), AgentLogin(backend: "gemini")])
}
