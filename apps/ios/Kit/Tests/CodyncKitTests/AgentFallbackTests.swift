import Foundation
import Testing
@testable import CodyncKit

private func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }

private extension Bot {
    /// The banner as English reads it: the phrase with its names put in.
    func stepBanner(_ name: (AgentLogin, Bool) -> String) -> String? {
        stepBannerPhrase(name: name).map { L10n.fill($0.key, $0.values) }
    }
}

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
    // Nor Advisor mode.
    #expect(older.threadAgent == nil && older.threadFallbacks == nil)
    #expect(body["threadAgent"] == nil && body["threadFallbacks"] == nil)
    // An agent it can't read leaves Advisor mode unknown, so saving never turns it off.
    let unreadable = try bot(#"{"id":"b","threadAgent":{"model":7},"threadFallbacks":[]}"#)
    #expect(unreadable.threadAgent == nil && unreadable.threadFallbacks == nil)
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

@Test func advisorModeStartsTheThreadsOnTheBotsOwnAgentAndChainAndAnotherThreadsAgentDropsItsLoginFromTheirs() throws {
    let codex = BotFallback(backend: "codex")
    let draft = BotDraft(try bot(#"{"id":"b","backend":"claude","model":"opus","fallbacks":[{"backend":"codex","model":null}],"threadAgent":null,"threadFallbacks":[]}"#))
    #expect(draft.threadAgent == nil && draft.threadFallbacks == [])
    #expect(try updateBody(draft)["threadAgent"] is NSNull, "a host with Advisor mode: off is sent as null")
    var on = draft
    on.setAdvisor(true, backends: [])
    #expect(on.threadAgent == BotFallback(backend: "claude", model: "opus") && on.threadFallbacks == [codex])
    var moved = on
    moved.setThreadAgent("codex")
    #expect(moved.threadAgent == BotFallback(backend: "codex") && moved.threadFallbacks == [])
    var again = moved
    again.setThreadAgent("codex")
    #expect(again == moved)
    #expect(moved.fallbacks == [codex], "the chat's chain is its own")
    let body = try updateBody(moved)
    #expect((body["threadAgent"] as? [String: Any])?["backend"] as? String == "codex")
    #expect((body["threadFallbacks"] as? [Any])?.isEmpty == true)
    var off = moved
    off.setAdvisor(false, backends: [])
    #expect(off.threadAgent == nil && off.threadFallbacks == [])
    let gemini = try JSONDecoder().decode(Backend.self, from: Data(#"{"id":"gemini","name":"Gemini","available":true,"command":"gemini","installHint":""}"#.utf8))
    var custom = draft
    custom.backend = "custom"
    custom.setAdvisor(true, backends: [gemini])
    #expect(custom.threadAgent?.backend == "gemini" && custom.threadFallbacks == [], "the picker has no custom command")
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

private let claudeJSON = #"{"id":"claude","name":"Claude Code","available":true,"command":"claude","installHint":"","loginProfiles":[{"id":"a1","signedIn":true},{"id":"a2","signedIn":false},{"id":"a3","signedIn":true}]}"#

@Test func fallbackStepsDecodeAndSendTheirLoginProfile() throws {
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    let b = try bot("""
    {"id":"b","backend":"claude","fallbacks":[{"backend":"claude","loginProfile":"a1","model":null},{"backend":"codex","loginProfile":7}],
     "agentStep":{"step":1,"agent":"claude","loginProfile":"a1","since":1},
     "resting":[{"agent":"claude","until":\(soon),"reason":"usageLimit"},{"agent":"claude","loginProfile":"a3","until":\(soon),"reason":"rateLimited"}]}
    """)
    #expect(b.fallbacks?.map(\.loginProfile) == ["a1", nil])
    #expect(b.stepLogin == AgentLogin(backend: "claude", loginProfile: "a1") && b.stepLogin != b.login)
    // A rest belongs to one login, not to the agent.
    #expect(b.rest(of: b.login)?.reason == "usageLimit")
    #expect(b.rest(of: AgentLogin(backend: "claude", loginProfile: "a3"))?.reason == "rateLimited")
    #expect(b.rest(of: AgentLogin(backend: "claude", loginProfile: "a1")) == nil)
    let sent = try #require(updateBody(BotDraft(b))["fallbacks"] as? [[String: Any]])
    // The default login sends null: a step without the key would keep its saved login profile on the host.
    #expect(sent[0]["loginProfile"] as? String == "a1" && sent[1]["loginProfile"] is NSNull)
}

@Test func theBannerTellsTwoLoginsOfOneAgentApart() throws {
    let name: (AgentLogin, Bool) -> String = { login, own in
        login.backend + (login.loginProfile.map { " (\($0))" } ?? (own ? " (Default login profile)" : ""))
    }
    let onProfile = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"claude","loginProfile":"Work","since":1}}"#)
    #expect(onProfile.stepBanner(name) == "Using claude (Work) while claude (Default login profile) can't answer")
    let onCodex = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}}"#)
    #expect(onCodex.stepBanner(name) == "Using codex while claude can't answer")
    #expect(try bot(#"{"id":"b","backend":"claude"}"#).stepBanner(name) == nil)
}

@Test func onlyAUsageLimitGivesTheBannerAResetTime() throws {
    let name: (AgentLogin, Bool) -> String = { login, _ in login.backend }
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    let resting = { (reason: String) in
        try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1},"resting":[{"agent":"claude","until":\#(soon),"reason":"\#(reason)"}]}"#)
    }
    let clock = RelativeTime.clock(Date(milliseconds: soon))
    #expect(try resting("usageLimit").stepBanner(name) == "Using codex until claude resets at \(clock)")
    for reason in ["signedOut", "rateLimited", "outOfCredits", "overloaded", "notStarting"] {
        #expect(try resting(reason).stepBanner(name)?.contains("resets") == false, "\(reason)")
    }
    #expect(try resting("signedOut").stepBanner(name) == "Using codex while claude is signed out")
}

@Test func everyBannerTheHostCanCauseIsTranslated() throws {
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    // The reasons `rest::record` rests an agent for, and none; a missing model or mode never rests one.
    for reason in [nil, "usageLimit", "outOfCredits", "rateLimited", "overloaded", "signedOut", "notStarting"] {
        let rest = reason.map { #","resting":[{"agent":"claude","until":\#(soon),"reason":"\#($0)"}]"# } ?? ""
        let b = try bot(#"{"id":"b","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}\#(rest)}"#)
        let key = try #require(b.stepBannerPhrase { login, _ in login.backend }).key
        #expect(L10n.tr(key, preference: .zhHant) != key, "\(reason ?? "none")")
        // A rest line goes through the catalog's "{time}" templates: a reworded line or key falls back to English.
        if reason != nil {
            let line = try #require(b.rest(of: b.login)).line
            #expect(L10n.tr(line, preference: .zhHant) != line, "\(reason ?? "")")
        }
    }
    #expect(L10n.fill(L10n.tr("Using {to} while {own} is signed out", preference: .zhHant), ["to": "Codex", "own": "Kimi Code"])
        == "正在使用 Codex，Kimi Code 已登出")
}

@Test func aLoginProfileNameHoldingWhileStaysOneName() throws {
    let name: (AgentLogin, Bool) -> String = { login, own in
        login.backend + (login.loginProfile.map { " (\($0))" } ?? (own ? " (Default login profile)" : ""))
    }
    let soon = Int64(Date.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000)
    let signingOut = try bot("""
    {"id":"b","backend":"claude","loginProfile":"Work while traveling","agentStep":{"step":1,"agent":"codex","since":1},
     "resting":[{"agent":"claude","loginProfile":"Work while traveling","until":\(soon),"reason":"signedOut"}]}
    """)
    let phrase = try #require(signingOut.stepBannerPhrase(name: name))
    // The phrase keeps its slot, and the whole name is one value: translating the phrase never sees the name.
    #expect(phrase.key == "Using {to} while {own} is signed out")
    #expect(phrase.values == ["to": "codex", "own": "claude (Work while traveling)"])
    #expect(L10n.fill(phrase.key, phrase.values) == "Using codex while claude (Work while traveling) is signed out")
}

@Test func freeLoginsAreTheDefaultAndSignedInLoginProfilesTheChainLacks() throws {
    let claude = try JSONDecoder().decode(Backend.self, from: Data(claudeJSON.utf8))
    #expect(claude.freeLogins(taken: []) == [nil, "a1", "a3"])
    // The bot on the default login, a step on a1: only a3 is left; another agent's logins don't count.
    let taken = [AgentLogin(backend: "claude"), AgentLogin(backend: "claude", loginProfile: "a1"), AgentLogin(backend: "codex", loginProfile: "a3")]
    #expect(claude.freeLogins(taken: taken) == ["a3"])
    #expect(claude.freeLogins(taken: taken + [AgentLogin(backend: "claude", loginProfile: "a3")]).isEmpty)
    let plain = try JSONDecoder().decode(Backend.self, from: Data(#"{"id":"codex","name":"Codex","available":true,"command":"codex","installHint":""}"#.utf8))
    #expect(plain.freeLogins(taken: []) == [nil] && plain.freeLogins(taken: [AgentLogin(backend: "codex")]).isEmpty)
}

@Test func aStepPickedInPlaceGetsItsOwnBackOnItsOwnAgent() throws {
    let claude = try JSONDecoder().decode(Backend.self, from: Data(claudeJSON.utf8))
    let codex = try JSONDecoder().decode(Backend.self, from: Data(#"{"id":"codex","name":"Codex","available":true,"command":"codex","installHint":""}"#.utf8))
    let taken = [AgentLogin(backend: "claude"), AgentLogin(backend: "codex")]
    #expect(BotFallback.on(claude, taken: taken) == BotFallback(backend: "claude", loginProfile: "a1"))
    let own = BotFallback(backend: "claude", loginProfile: "a2", model: "opus", modelOptions: ["effort": "high"])
    #expect(BotFallback.on(claude, taken: taken, own: own) == own, "back on its own agent, its model and login come back")
    #expect(BotFallback.on(claude, taken: taken + [own.login], own: own).loginProfile == "a1", "unless another step has its login now")
    // Its signed-out login stays a choice, and so does its agent.
    #expect(claude.logins(for: own.login, taken: taken) == ["a1", "a3", "a2"])
    let full = taken + [AgentLogin(backend: "claude", loginProfile: "a1"), AgentLogin(backend: "claude", loginProfile: "a3")]
    #expect([claude, codex].usable(taken: full).isEmpty)
    #expect([claude, codex].usable(taken: full, keeping: [own.login]).map(\.id) == ["claude"])
    // Its remembered login is gone once another step has it and no other is free: picking that agent would land on the
    // bot's own login.
    #expect([claude, codex].usable(taken: full + [own.login], keeping: [own.login]).isEmpty)
}

@Test func switchingAgentKeepsItsStepsOnOtherLoginProfiles() {
    var draft = BotDraft(name: "Lead")
    draft.fallbacks = [BotFallback(backend: "codex"), BotFallback(backend: "codex", loginProfile: "a1"), BotFallback(backend: "gemini")]
    draft.setAgent("codex")
    #expect(draft.fallbacks?.map(\.login) == [AgentLogin(backend: "codex", loginProfile: "a1"), AgentLogin(backend: "gemini")])
}
