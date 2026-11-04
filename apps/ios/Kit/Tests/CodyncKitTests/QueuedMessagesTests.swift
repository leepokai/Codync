import Foundation
import Testing
@testable import CodyncKit

private func entry(_ id: String, seq: Int64, turn: Int64, _ json: String) throws -> Entry {
    let data = try JSONDecoder().decode(EntryData.self, from: Data(json.utf8))
    return Entry(id: id, seq: seq, botId: "b", rev: 1, kind: "user", turn: turn, data: data, createdAt: seq, updatedAt: seq)
}

@Test func whileTheBotWorksItsWaitingMessagesLeaveTheChatForTheQueue() throws {
    let entries = [
        try entry("asked", seq: 1, turn: 1, #"{"text":"first","status":"sent"}"#),
        try entry("later", seq: 3, turn: 3, #"{"text":"third","status":"queued"}"#),
        try entry("next", seq: 2, turn: 2, #"{"text":"second","status":"queued"}"#),
        try entry("taken", seq: 4, turn: 4, #"{"text":"gone","status":"cancelled","removed":true}"#),
    ]
    #expect(entries.queue(working: true).map(\.id) == ["next", "later"], "oldest first, never a taken-back one")
    #expect(entries.hidingQueue(working: true).map(\.id) == ["asked", "taken"])
    // A bot that isn't working starts a message at once: it stays in the chat.
    #expect(entries.queue(working: false).isEmpty)
    #expect(entries.hidingQueue(working: false).map(\.id) == entries.map(\.id))
}

@Test func aMessageTakenBackShowsNowhereAndHoldsNoTurn() throws {
    let entries = [
        try entry("asked", seq: 1, turn: 1, #"{"text":"first","status":"sent"}"#),
        try entry("taken", seq: 2, turn: 2, #"{"text":"gone","status":"cancelled","removed":true}"#),
        try entry("stopped", seq: 3, turn: 1, #"{"text":"not sent","status":"cancelled"}"#),
    ]
    #expect(entries.map(\.isChat) == [true, false, true])
    #expect(Entry.runningTurn(entries, working: true) == 1)
    #expect(Entry.traceTurns(entries).map(\.key) == [1])
}
