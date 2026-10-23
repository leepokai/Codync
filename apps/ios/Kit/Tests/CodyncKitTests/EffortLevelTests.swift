import Foundation
import Testing
@testable import CodyncKit

private func option(_ json: String) throws -> AgentModels.Option {
    try JSONDecoder().decode(AgentModels.Option.self, from: Data(json.utf8))
}

private let level = #"{"id":"effort","name":"Thinking","category":"thought_level","default":"low","values":[{"id":"low","name":"Low"},{"id":"high","name":"High"}]}"#

private func bot(_ fields: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b",\#(fields)}"#.utf8))
}

@Test func theEffortLevelIsTheThoughtLevelSettingWhateverItsID() throws {
    let verbosity = try option(#"{"id":"verbosity","name":"Verbosity","category":"_custom","values":[]}"#)
    #expect(EffortLevel.setting(in: [verbosity, try option(level)])?.id == "effort")
    #expect(EffortLevel.setting(in: [verbosity]) == nil)
}

@Test func aBotCarriesItsSessionReportAndAnOlderHostReportsNothing() throws {
    let running = try bot(#""sessionLive":true,"sessionOptions":{"effort":"high"},"sessionSettings":[\#(level)],"settingIssues":{"effort":{"issue":"rejected","value":"max"}},"restartPending":true"#)
    #expect(running.sessionLive && running.restartPending)
    #expect(running.sessionOptions == ["effort": "high"])
    #expect(running.sessionSettings?.first?.category == "thought_level")
    #expect(running.settingIssues["effort"] == SettingIssue(issue: "rejected", value: "max"))
    let old = try bot("")
    #expect(!old.sessionLive && old.sessionOptions == nil && old.sessionSettings == nil && old.settingIssues.isEmpty && !old.restartPending)
}

@Test func aStepHearsOnlyItsOwnAgent() throws {
    let own = try bot(#""backend":"claude","sessionLive":true,"sessionOptions":{"effort":"low"}"#)
    #expect(own.report(for: "claude")?.options?["effort"] == "low")
    #expect(own.report(for: "codex") == nil, "a fallback that isn't running says nothing")
    let onFallback = try bot(#""backend":"claude","agentStep":{"step":1,"agent":"codex","since":0},"sessionLive":true,"sessionOptions":{"effort":"low"}"#)
    #expect(onFallback.report(for: "claude") == nil)
    #expect(onFallback.report(for: "codex") != nil)
}

@Test func theCaptionSaysWhatTheRunningSessionConfirms() throws {
    let catalog = [try option(level)]
    func caption(_ saved: String?, _ fields: String?, pending: Bool = false) throws -> String? {
        let report = try fields.map { try bot(#""backend":"claude",\#($0)"#).report(for: "claude") } ?? nil
        guard case .row(_, let caption) = EffortLevel.row(catalog: catalog, saved: saved, report: report, loading: false, failed: false, chosenModel: false) else { return "no row" }
        return caption
    }
    #expect(try caption("high", #""sessionLive":true,"sessionOptions":{"effort":"high"}"#) == "In use: High")
    #expect(try caption(nil, #""sessionLive":true,"sessionOptions":{"effort":"low"}"#) == "In use: Low (agent default)")
    #expect(try caption("high", #""sessionLive":true,"sessionOptions":{"effort":"low"}"#) == "Saved: High · not confirmed (in use: Low)")
    #expect(try caption("high", #""sessionLive":true,"sessionOptions":{}"#) == "Current: unknown")
    #expect(try caption("high", nil) == "Not running · Saved: High")
    #expect(try caption("high", #""sessionLive":true,"restartPending":true,"sessionOptions":{"effort":"low"}"#) == "In use: Low · High pending")
    #expect(try caption("high", #""settingIssues":{"effort":{"issue":"rejected","value":"high"}}"#) == "Saved: High · not applied (the agent refused it)")
    #expect(try caption("high", #""sessionLive":true,"sessionOptions":{"effort":"low"},"settingIssues":{"effort":{"issue":"not_offered","value":"high"}}"#) == "In use: Low · Saved: High not offered")
}

@Test func anUnknownOrAbsentEffortLevel() {
    #expect(EffortLevel.row(catalog: [], saved: nil, report: nil, loading: false, failed: false, chosenModel: true) == .later)
    #expect(EffortLevel.row(catalog: [], saved: nil, report: nil, loading: false, failed: false, chosenModel: false) == .none)
    #expect(EffortLevel.row(catalog: [], saved: "high", report: nil, loading: false, failed: true, chosenModel: false) == .row(setting: nil, caption: "Saved: high · couldn’t check this agent’s options"))
}

@Test func aFallbackRowSaysWhatItsOwnAgentReportsOrWhatIsSavedWithoutAskingIt() throws {
    let onIt = try bot(#""backend":"claude","agentStep":{"step":1,"agent":"codex","since":0},"sessionLive":true,"sessionOptions":{"effort":"high"},"sessionSettings":[\#(level)]"#)
    let running = onIt.report(for: "codex")
    #expect(EffortLevel.fallbackLine(saved: ["effort": "high"], report: running) == "Effort level · In use: High")
    #expect(EffortLevel.fallbackLine(saved: [:], report: running) == "Effort level · In use: High (agent default)")
    #expect(EffortLevel.fallbackLine(saved: ["effort": "high"], report: nil) == "Saved: high")
    #expect(EffortLevel.fallbackLine(saved: [:], report: nil) == nil)
}

@Test func aStepShowsEverySettingItsRunningSessionListsAndAnySavedOneNobodyListsStaysVisible() throws {
    // Greptile on #204: the session's verbosity was hidden, and a saved effort the agent stopped listing vanished.
    let verbosity = try option(#"{"id":"verbosity","name":"Verbosity","values":[{"id":"terse","name":"Terse"}]}"#)
    let effort = try option(level)
    let listed = try bot(#""backend":"claude","sessionSettings":[\#(level),{"id":"verbosity","name":"Verbosity","values":[{"id":"terse","name":"Terse"}]}]"#).report(for: "claude")
    #expect(EffortLevel.shown(catalog: [], report: listed, saved: [:]) == [effort, verbosity])
    let pending = try bot(#""backend":"claude","restartPending":true,"sessionSettings":[\#(level)]"#).report(for: "claude")
    #expect(EffortLevel.shown(catalog: [verbosity], report: pending, saved: [:]) == [verbosity])
    let unlisted = EffortLevel.shown(catalog: [], report: nil, saved: ["effort": "high"])
    #expect(unlisted.map(\.id) == ["effort"] && unlisted.first?.values == [])
    #expect(EffortLevel.shown(catalog: [effort], report: nil, saved: ["effort": "high"]) == [effort])
}

@Test func aSessionThatListsFewerSettingsThanTheCatalogKeepsTheCatalogsOthers() throws {
    // Greptile on #204: Cursor lists its settings per model, while its session lists only the model.
    let verbosity = try option(#"{"id":"verbosity","name":"Verbosity","values":[{"id":"terse","name":"Terse"}]}"#)
    let effort = try option(level)
    let quiet = try bot(#""backend":"claude","sessionSettings":[]"#).report(for: "claude")
    #expect(EffortLevel.shown(catalog: [effort, verbosity], report: quiet, saved: [:]) == [effort, verbosity])
    let max = try option(#"{"id":"effort","name":"Thinking","category":"thought_level","values":[{"id":"max","name":"Max"}]}"#)
    let own = try bot(#""backend":"claude","sessionSettings":[{"id":"effort","name":"Thinking","category":"thought_level","values":[{"id":"max","name":"Max"}]}]"#).report(for: "claude")
    #expect(EffortLevel.shown(catalog: [effort, verbosity], report: own, saved: [:]) == [max, verbosity])
}
