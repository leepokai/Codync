import Foundation
import Testing
@testable import CodyncKit

private func entry(_ id: String, _ kind: String, at: Int64, _ json: String) throws -> Entry {
    let data = try JSONDecoder().decode(EntryData.self, from: Data(json.utf8))
    return Entry(id: id, seq: at, botId: "b", rev: 1, kind: kind, turn: 1, data: data, createdAt: at, updatedAt: at)
}

@Test func aCallsMessagesHideBehindItsLine() throws {
    let entries = [
        try entry("u1", "user", at: 1, #"{"text":"spoken","call":"c"}"#),
        try entry("a1", "agent", at: 2, #"{"text":"reply","final":true,"call":"c"}"#),
        try entry("u2", "user", at: 3, #"{"text":"no line yet","call":"lost"}"#),
        try entry("u3", "user", at: 4, #"{"text":"on this phone","call":"live"}"#),
        try entry("local-n", "user", at: 5, #"{"text":"didn't go out","call":"live","status":"failed"}"#),
        try entry("f1", "agent", at: 6, #"{"text":"log.txt (3 bytes)","final":true,"call":"c","files":[{"id":"f","name":"log.txt","size":3,"sha256":""}]}"#),
        try entry("n1", "notice", at: 7, #"{"text":"Release plan","callSeconds":26,"call":"c"}"#),
    ]
    #expect(entries.hidingCalls().map(\.id) == ["local-n", "f1", "n1"])
}

@Test func aCallsConversationMergesItsTranscriptWithWhatItCarried() throws {
    let line = try entry("n1", "notice", at: 10, #"""
    {"text":"Git","call":"c","transcript":[{"from":"user","text":"看一下 git","at":1},{"from":"voice","text":"Clean.","at":9}]}
    """#)
    let entries = [
        try entry("u1", "user", at: 2, #"{"text":"check git","call":"c","from":"voice"}"#),
        try entry("a1", "agent", at: 3, #"{"text":"Working tree clean","final":true,"call":"c"}"#),
        try entry("t1", "agent", at: 4, #"{"text":"narration","call":"c"}"#),
        try entry("x1", "user", at: 5, #"{"text":"typed"}"#),
        line,
    ]
    let said = entries.callConversation(line).map { "\($0.speaker): \($0.text)" }
    #expect(said == ["user: 看一下 git", "voiceToBot: check git", "bot: Working tree clean", "voice: Clean."])
}

@Test func aCallsOwnEntriesFillInWhatTheChatDoesNotHold() throws {
    let line = try entry("n1", "notice", at: 10, #"{"text":"Old call","callSeconds":30,"call":"c"}"#)
    let fetched = [
        try entry("u1", "user", at: 2, #"{"text":"from before the loaded chat","call":"c"}"#),
        try entry("a1", "agent", at: 3, #"{"text":"its reply","final":true,"call":"c"}"#),
    ]
    #expect([line].callConversation(line).isEmpty)
    let merged = [line].withCallEntries(fetched)
    #expect(merged.callConversation(line).map { "\($0.speaker): \($0.text)" } == ["user: from before the loaded chat", "bot: its reply"])
    #expect(merged.withCallEntries(fetched).count == merged.count)
}
