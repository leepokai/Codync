import Foundation
import Testing
@testable import CodyncKit

private func visibilityEntry(_ kind: String, hidden: Bool? = true, id: String = "e", turn: Int64 = 1) -> Entry {
    var data = EntryData(text: "Scheduled work", status: "pending")
    data.`internal` = hidden
    data.final = true
    data.bookkeeping = false
    return Entry(id: id, seq: turn, botId: "bot", rev: turn, kind: kind, turn: turn, data: data, createdAt: turn, updatedAt: turn)
}

@Test func internalEntriesStayOutOfChatTraceAndReplyControls() {
    for kind in ["user", "agent", "thought", "tool", "plan", "notice", "permission"] {
        let entry = visibilityEntry(kind)
        #expect(!entry.isChat)
        #expect(!entry.isWorkingStep)
        #expect(!entry.isReplyable)
    }
}

@Test func absentFlagsAndPromotedOutputRemainVisible() {
    let legacy = visibilityEntry("user", hidden: nil)
    let promoted = visibilityEntry("agent", hidden: false)
    var literal = legacy
    literal.data.text = "NO_UPDATE"
    #expect(legacy.isChat)
    #expect(promoted.isChat)
    #expect(promoted.isReplyable)
    #expect([visibilityEntry("thought"), legacy, promoted, literal].visibleEntries() == [legacy, promoted, literal])
}

@Test func cachedInternalFlagsDecodeWithoutBreakingOlderEntries() throws {
    let hidden = #"{"id":"hidden","seq":1,"botId":"bot","rev":4,"kind":"user","turn":1,"data":{"text":"Scheduled work","internal":true},"createdAt":1,"updatedAt":1}"#
    let legacy = #"{"id":"legacy","seq":2,"botId":"bot","rev":5,"kind":"user","turn":1,"data":{"text":"Visible"},"createdAt":2,"updatedAt":2}"#
    let entries = try [hidden, legacy].map { try JSONDecoder().decode(Entry.self, from: Data($0.utf8)) }
    #expect(entries[0].isInternal)
    #expect(entries.visibleEntries().map(\.id) == ["legacy"])
}

@Test func legacyScheduledInputsRemainPrivateWithoutHidingOrdinaryMessages() {
    var scheduled = visibilityEntry("user", hidden: nil)
    scheduled.data.from = "proactive"
    #expect(scheduled.isInternal)
    #expect(!scheduled.isChat)
    #expect(!scheduled.isReplyable)
    #expect([scheduled].visibleEntries().isEmpty)
    #expect(visibilityEntry("permission", hidden: false).isChat)
}

@Test func internalTurnsDoNotReplaceTheVisibleWorkingTrace() {
    var publicStep = visibilityEntry("tool", hidden: false, id: "tool", turn: 1)
    publicStep.data.status = "in_progress"
    let internalStep = visibilityEntry("thought", turn: 2)
    #expect(Entry.workingSteps([publicStep, internalStep]) == [publicStep])
}

@Test func internalEntriesStayOutOfCallConversationsAndCannotRestoreAnOlderCopy() {
    var hidden = visibilityEntry("agent")
    hidden.data.call = "c"
    var line = visibilityEntry("notice", hidden: false, id: "line")
    line.data.call = "c"
    #expect([hidden].hidingCalls().isEmpty)
    #expect([hidden].callConversation(line).isEmpty)
    var older = hidden
    older.data.`internal` = false
    older.rev = 0
    #expect([hidden].withCallEntries([older]).isEmpty)
    line.data.`internal` = true
    #expect([older].callConversation(line).isEmpty)
}
