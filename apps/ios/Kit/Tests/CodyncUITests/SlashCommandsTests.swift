import Foundation
import SwiftUI
import Testing
@testable import CodyncKit
@testable import CodyncUI

private func bot(_ json: String) throws -> Bot { try JSONDecoder().decode(Bot.self, from: Data(json.utf8)) }

private let installed = [("pdf", "PDF"), ("code-review", "Code Review"), ("xlsx", "xlsx")].map {
    InstalledSkill(id: $0.0, name: $0.1, description: "", source: "custom", path: "")
}

@Test func aLeadingSlashOpensTheListUntilTheNameEnds() {
    #expect(SlashCommands.query("/") == "")
    #expect(SlashCommands.query("/pd") == "pd")
    #expect(SlashCommands.query("/pdf merge") == nil)
    #expect(SlashCommands.query("hi /pdf") == nil)
}

@Test func builtInCommandsComeFirstThenOnlyTheBotsOwnSkills() throws {
    let offered = try bot(#"{"id":"b","skills":["pdf","code-review"],"fastModeSupport":"offered"}"#)
    #expect(SlashCommands.list(for: offered, query: "", skills: installed).map(\.name) == ["fast", "pdf", "code-review"])
    #expect(SlashCommands.list(for: offered, query: "review", skills: installed).map(\.name) == ["code-review"])
    #expect(SlashCommands.list(for: offered, query: "new", skills: installed).isEmpty, "a new session is in the bot's menu")
    #expect(SlashCommands.list(for: try bot(#"{"id":"b","skills":["pdf"]}"#), query: "xl", skills: installed).isEmpty)
    #expect(SlashCommands.list(for: try bot(#"{"id":"b"}"#), query: "", skills: []).map(\.name) == ["fast"], "offered until the agent says otherwise")
    #expect(SlashCommands.list(for: try bot(#"{"id":"b","fastModeSupport":"unsupported"}"#), query: "", skills: []).isEmpty, "no Fast mode on offer")
}

@Test func aGroupListsItsMembersFastModeAndTheirSkills() throws {
    let group = try bot(#"{"id":"g","kind":"group","members":["a","b","c"]}"#)
    let a = try bot(#"{"id":"a","name":"Ann","skills":["pdf"],"fastModeSupport":"offered"}"#)
    let b = try bot(#"{"id":"b","name":"Bo","skills":["xlsx","pdf"],"fastModeSupport":"offered"}"#)
    let none = try bot(#"{"id":"c","name":"Cy","fastModeSupport":"unsupported"}"#)
    let list = SlashCommands.list(for: group, members: [a, b, none], query: "", skills: installed)
    #expect(list.map(\.name) == ["fast", "pdf", "xlsx"], "Fast mode, then every member's skills once")
    #expect(list.first?.detail == "Turn on for every bot here: faster replies and more usage")
    let skilledOnly = try bot(#"{"id":"d","skills":["code-review"],"fastModeSupport":"unsupported"}"#)
    #expect(SlashCommands.list(for: group, members: [none, skilledOnly], query: "", skills: installed).map(\.name) == ["code-review"],
            "no member shows Fast mode: only skills")
    #expect(SlashCommands.list(for: group, members: [none], query: "", skills: installed).isEmpty)
    #expect(SlashCommands.builtIn(for: group, members: [a, b], draft: "/fast")?.action == .fast)
    #expect(SlashCommands.builtIn(for: group, members: [none], draft: "/fast") == nil)
}

@Test func aGroupsFastModeIsOnOnlyWhenEveryMemberShowingItHasItOn() throws {
    let on = try bot(#"{"id":"a","name":"Ann","fastMode":true,"fastModeSupport":"offered"}"#)
    let off = try bot(#"{"id":"b","name":"Bo","fastModeSupport":"offered"}"#)
    let hidden = try bot(#"{"id":"c","fastModeSupport":"unsupported"}"#)
    let refused = try bot(#"{"id":"d","name":"Di","fastMode":true,"fastModeSupport":"refused","fastModeIssue":"requires extra usage"}"#)
    #expect(GroupFastMode([on, off]).state == .off)
    #expect(GroupFastMode([on, hidden]).state == .on, "members that don't show it don't count")
    #expect(GroupFastMode([on, hidden]).members.map(\.id) == ["a"])
    #expect(GroupFastMode([on, hidden]).detail == "Turn off for every bot here and return to standard speed")
    #expect(GroupFastMode([on, refused]).state == .refused)
    #expect(GroupFastMode([on, refused]).detail.hasPrefix("Di: The agent turned it off: requires extra usage"))
    #expect(GroupFastMode([hidden]).state == .off)
    let group = try bot(#"{"id":"g","kind":"group","members":["a","d"]}"#)
    #expect(SlashCommands.list(for: group, members: [on, refused], query: "", skills: []).first?.warning == true)
}

@Test func sendingJustABuiltInCommandRunsIt() throws {
    let offered = try bot(#"{"id":"b","fastModeSupport":"offered"}"#)
    #expect(SlashCommands.builtIn(for: offered, draft: "/fast ")?.action == .fast)
    #expect(SlashCommands.builtIn(for: offered, draft: "/new") == nil, "reaches the agent as typed")
    #expect(SlashCommands.builtIn(for: offered, draft: "/fast please") == nil)
    #expect(SlashCommands.builtIn(for: try bot(#"{"id":"b","fastModeSupport":"unsupported"}"#), draft: "/fast") == nil, "without Fast mode it reaches the agent")
}

@MainActor @Test func theCommandListHugsItsRowsAndScrollsPastItsCap() {
    func height(_ count: Int) -> CGFloat {
        let commands = (0..<count).map {
            SlashCommand(name: "s\($0)", icon: "bolt", title: "Skill \($0)", detail: "", warning: false, action: .skill("s\($0)"))
        }
        return UIHostingController(rootView: CommandList(commands: commands) { _ in })
            .sizeThatFits(in: CGSize(width: 390, height: 2000)).height
    }
    #expect(height(2) < 140, "two rows, not the 280-point cap")
    #expect(height(20) == 280)
}
