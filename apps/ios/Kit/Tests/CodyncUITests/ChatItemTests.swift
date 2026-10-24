import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// The chat shows user messages and the bot's messages (Grok Bot's `send_message`), each its
/// own bubble; what the bot writes along the way, and room passes, stay in the trace.
@Test func chatShowsOnlyMessagesForTheUser() {
    func entry(_ seq: Int64, _ kind: String, _ text: String? = nil, final: Bool? = nil) -> Entry {
        var data = EntryData(text: text)
        data.final = final
        return Entry(id: "e\(seq)", seq: seq, botId: "b", rev: seq, kind: kind, turn: 1, data: data, createdAt: 0, updatedAt: 0)
    }
    let ids = { (items: [ChatItem]) in items.compactMap { if case let .entry(e, _) = $0.kind { e.id } else { nil } } }

    let turn = [entry(1, "user", "Fix it"), entry(2, "agent", "I'll read it first.", final: false),
                entry(3, "agent", "Found it.", final: true), entry(4, "tool_call", "Edit"),
                entry(5, "agent", "Fixed.", final: true), entry(6, "agent", "(pass)", final: false)]
    #expect(ids(ChatItem.build(turn)) == ["e1", "e3", "e5"])
}

/// Tables, rules and nested lists, as agents write them.
@Test func markdownBlocksParseTablesRulesAndNesting() {
    let blocks = MarkdownBlocks.parse("""
    | Name | Size |
    |---|:-:|
    | a | 1 |
    ---
    - top
      - inner
    | not a table
    """)
    #expect(blocks == [
        .table(header: ["Name", "Size"], rows: [["a", "1"]]),
        .rule,
        .bullet("top", marker: "•", depth: 0),
        .bullet("inner", marker: "◦", depth: 1),
        .paragraph("| not a table"),
    ])
}

@Test func messagesWithoutClientNoncesHaveDistinctRowAndSeparatorIdentities() {
    let entries = ["", "", nil].enumerated().map { index, nonce in
        Entry(id: "incoming-\(index)", seq: Int64(index), botId: "b", rev: Int64(index), kind: "user", turn: 1,
              data: EntryData(text: "Incoming message \(index)", clientNonce: nonce),
              createdAt: Int64(index) * 7_200_000, updatedAt: 0)
    }
    let items = ChatItem.build(entries)
    let messageIDs = items.compactMap { item -> String? in
        if case .entry = item.kind { item.id } else { nil }
    }
    #expect(messageIDs == entries.map(\.id))
    #expect(Set(items.map(\.id)).count == items.count)
}

@Test func optimisticMessageKeepsItsRowIdentityAfterAcknowledgement() {
    let data = EntryData(text: "Sent message", clientNonce: "client-nonce")
    let optimistic = Entry(id: "local-client-nonce", seq: 0, botId: "b", rev: 0, kind: "user", turn: 1,
                           data: data, createdAt: 0, updatedAt: 0)
    var delivered = optimistic
    delivered.id = "server-entry"
    delivered.seq = 10
    delivered.rev = 11
    let pendingItems = ChatItem.build([optimistic])
    let deliveredItems = ChatItem.build([delivered])
    #expect(pendingItems.map(\.id) == deliveredItems.map(\.id))
}

/// The trace unfolds the turn running now, not a queued message that already holds the next turn.
@Test func traceRunningTurnSkipsQueuedMessages() {
    func entry(_ id: String, turn: Int64, status: String? = nil) -> Entry {
        Entry(id: id, seq: 0, botId: "b", rev: 0, kind: "user", turn: turn,
              data: EntryData(text: id, status: status), createdAt: 0, updatedAt: 0)
    }
    let entries = [entry("u1", turn: 1), entry("t1", turn: 1), entry("q", turn: 2, status: "queued")]
    #expect(TraceView.runningTurn(entries, working: true) == 1)
    #expect(TraceView.runningTurn(entries, working: false) == nil)
}
