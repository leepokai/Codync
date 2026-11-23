import Foundation
import SwiftUI
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
    #expect(Entry.runningTurn(entries, working: true) == 1)
    #expect(Entry.runningTurn(entries, working: false) == nil)
}

/// The working indicator unfolds the running turn's work, without what the chat or memory already show.
@Test func workingStepsAreTheRunningTurnsWork() {
    func entry(_ id: String, _ kind: String, turn: Int64 = 2, text: String? = nil, status: String? = nil,
               final: Bool? = nil, bookkeeping: Bool? = nil) -> Entry {
        var data = EntryData(text: text ?? id, status: status)
        data.final = final
        data.bookkeeping = bookkeeping
        return Entry(id: id, seq: 0, botId: "b", rev: 0, kind: kind, turn: turn, data: data, createdAt: 0, updatedAt: 0)
    }
    let entries = [
        entry("old", "tool", turn: 1), entry("u2", "user"), entry("thinking", "thought"), entry("blank", "thought", text: "  "),
        entry("says", "agent"), entry("terminal", "tool", status: "in_progress", bookkeeping: false),
        entry("memory", "tool", bookkeeping: true), entry("unflagged", "tool"),
        entry("plan", "plan"), entry("reply", "agent", final: true), entry("card", "permission"),
        entry("pass", "agent", text: "(pass)"),
        entry("q", "user", turn: 3, status: "queued"),
    ]
    #expect(Entry.workingSteps(entries).map(\.id) == ["thinking", "says", "terminal", "plan"])
}

/// A subagent's steps sit under its launch in its turn, even when other work came between them; one whose launch isn't
/// there stays.
@Test func aSubagentsStepsSitUnderItsLaunch() {
    func tool(_ id: String, parent: String? = nil, turn: Int64 = 1) -> Entry {
        var data = EntryData(text: id, status: "completed")
        data.toolCallId = id
        data.parent = parent
        return Entry(id: id, seq: 0, botId: "b", rev: 0, kind: "tool", turn: turn, data: data, createdAt: 0, updatedAt: 0)
    }
    let steps = [tool("a"), tool("b"), tool("a1", parent: "a"), tool("b1", parent: "b"), tool("a2", parent: "a"),
                 tool("lost", parent: "gone"), tool("self", parent: "self"), tool("later", parent: "a", turn: 2)]
    #expect(Entry.nested(steps).map { "\($0.entry.id):\($0.depth)" } == ["a:0", "a1:1", "a2:1", "b:0", "b1:1", "lost:0", "self:0", "later:0"])
}

/// A subagent's launch reads its kind, tokens and time once finished, or that it runs in the background; a field of a
/// shape the app doesn't know leaves the rest.
@Test func aSubagentsLaunchReadsItsTotals() throws {
    #expect(Subagent(kind: "general-purpose", background: false).line == nil)
    #expect(Subagent(kind: "general-purpose", tokens: 42310, durationMs: 6287).line == "general-purpose · 42.3k tokens · 6 s")
    #expect(Subagent(kind: "Explore", tokens: 812, durationMs: 125_000).line == "Explore · 812 tokens · 2 min")
    #expect(Subagent(kind: "general-purpose", background: true).line == "general-purpose · in background")
    #expect([1150, 2000, 123_456, 1_200_000].map { Subagent(tokens: $0).line } == ["1.2k tokens", "2k tokens", "123k tokens", "1.2M tokens"])
    let odd = try JSONDecoder().decode(EntryData.self, from: Data(#"{"subagent": {"kind": "x", "tokens": "many"}}"#.utf8))
    #expect(odd.subagent?.kind == "x" && odd.subagent?.tokens == nil)
    let notAnObject = try JSONDecoder().decode(EntryData.self, from: Data(#"{"subagent": 3}"#.utf8))
    #expect(notAnObject.subagent?.kind == nil)
}

/// The unfolded steps are one scroll view that hugs them and scrolls past its cap.
@MainActor @Test func aCappedScrollViewHugsItsContentAndScrollsPastTheCap() {
    func height(_ content: CGFloat) -> CGFloat {
        UIHostingController(rootView: CappedHeight(max: 280) { ScrollView { Color.clear.frame(height: content) } })
            .sizeThatFits(in: CGSize(width: 390, height: 2000)).height
    }
    #expect(height(100) == 100)
    #expect(height(1000) == 280)
}

/// The queue's card hugs its rows, and past its cap they scroll instead of pushing the chat away.
@MainActor @Test func theQueueCardHugsItsRowsAndScrollsPastTheCap() {
    let suite = "ChatItemTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: Computer(id: "queue", name: "Mac", signKey: ""), storage: storage) { _ in
        throw HostError.unreachable
    }
    defer { store.retire() }
    func height(_ rows: Int) -> CGFloat {
        let entries = (0..<rows).map {
            Entry(id: "e\($0)", seq: Int64($0), botId: "b", rev: 0, kind: "user", turn: 1,
                  data: EntryData(text: "Waiting \($0)", status: "queued"), createdAt: 0, updatedAt: 0)
        }
        return UIHostingController(rootView: QueuedMessages(entries: entries).environment(store))
            .sizeThatFits(in: CGSize(width: 390, height: 2000)).height
    }
    #expect(height(1) < 60)
    #expect(height(3) > height(1) && height(3) < 190)
    #expect(height(25) == 190)
}

/// Find searches what messages say: not time separators, notices or the bot's work along the way.
@MainActor @Test func findSearchesOnlyMessageText() {
    func entry(_ seq: Int64, _ kind: String, _ text: String, final: Bool? = nil) -> Entry {
        var data = EntryData(text: text)
        data.final = final
        return Entry(id: "e\(seq)", seq: seq, botId: "b", rev: seq, kind: kind, turn: 1, data: data, createdAt: 0, updatedAt: 0)
    }
    let items = ChatItem.build([entry(1, "user", "find the plan"), entry(2, "agent", "planning", final: false),
                                entry(3, "agent", "The plan, and the Plan", final: true), entry(4, "notice", "plan saved")])
    let found = ChatView.findable(items)
    #expect(found.map(\.text) == ["find the plan", "The plan, and the Plan"])
    #expect(ConversationFind.matches(in: found, query: "plan").count == 3)
}
