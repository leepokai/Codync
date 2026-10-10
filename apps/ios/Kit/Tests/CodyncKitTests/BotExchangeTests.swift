import Foundation
import Testing
@testable import CodyncKit

private func notice(_ id: String, chat: String = "egan", seq: Int64 = 1, rev: Int64 = 1, at: Int64 = 1_000,
                    source: String = "egan", target: String = "owen", status: String = "queued",
                    request: String = "Check this", reply: String? = nil, detail: String? = nil,
                    intent: String = "message") -> Entry {
    let outgoing = chat == source
    let verb = switch intent {
    case "ask": outgoing ? "Asked Owen" : "Request from Egan"
    default: outgoing ? "Messaged Owen" : "Message from Egan"
    }
    let heading = "\(verb): \(request)"
    var outcome = detail ?? ""
    if let reply { outcome = "Reply from Owen:\n\(reply)" }
    var data = EntryData(text: "\(heading)\n\(outcome)", status: status)
    data.heading = heading
    data.delegationId = id
    data.sourceBotId = source
    data.targetBotId = target
    data.botMessage = BotMessage(sourceBotId: source, targetBotId: target, text: request, reply: reply, detail: detail)
    return Entry(id: id, seq: seq, botId: chat, rev: rev, kind: "notice", turn: 1, data: data, createdAt: at, updatedAt: at)
}

@Test func noticeDecodesTheHostContract() throws {
    let json = #"""
    {"id":"n1","seq":3,"botId":"egan","rev":9,"kind":"notice","turn":1,"createdAt":5,"updatedAt":6,
     "data":{"text":"Asked Owen: Review\nReply from Owen:\nLGTM","heading":"Asked Owen: Review","style":"info",
      "status":"completed","delegationId":"d1","sourceBotId":"egan","targetBotId":"owen","botMessage":{"sourceBotId":"egan","targetBotId":"owen","text":"Review","reply":"LGTM"}}}
    """#
    let entry = try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
    let x = try #require(BotExchange(entry))
    #expect(x.text == "Review" && x.reply == "LGTM" && x.outcome == .done)
}

@Test func botRepliesStayInThePopupAndIndependentReportsStayInMainChat() {
    let ask = notice("ask", chat: "owen", status: "completed", reply: "Answer for Egan", intent: "ask")
    var trace = ask
    trace.id = "trace"
    trace.seq = 2
    trace.kind = "agent"
    trace.data = EntryData(text: "Answer for Egan")
    let independent = notice("message", chat: "owen", seq: 3, status: "completed")
    var report = trace
    report.id = "report"
    report.seq = 4
    report.data = EntryData(text: "Report for the user")
    report.data.final = true
    let entries = [ask, trace, independent, report]
    #expect(entries.filter(\.isChat).map(\.id) == ["ask", "message", "report"])
    let rows = BotConversationRow.build(BotExchange.conversation(fetched: [], live: entries, peer: "egan"))
    let bodies = rows.compactMap { row -> String? in
        if case .message(_, _, let text, _) = row { text } else { nil }
    }
    #expect(bodies == ["Check this", "Answer for Egan", "Check this"])
}

@Test func structuredRequestIsIndependentOfDisplayedVerb() throws {
    for (intent, chat) in [("ask", "egan"), ("ask", "owen"), ("message", "egan"), ("message", "owen")] {
        let x = try #require(BotExchange(notice("a", chat: chat, request: "Do it", intent: intent)))
        #expect(x.text == "Do it")
    }
}

@Test func requestKeepsColonsAndNewlinesAndNamesMaySpace() throws {
    var e = notice("a", request: "Plan: step 1\nstep 2: go", intent: "ask")
    e.data.heading = "Asked Owen the Bot: Plan: step 1\nstep 2: go"
    e.data.text = "\(e.data.heading!)\n"
    #expect(BotExchange(e)?.text == "Plan: step 1\nstep 2: go")
    let multi = try #require(BotExchange(notice("b", request: "Plan: a\nb: c", intent: "ask")))
    #expect(multi.text == "Plan: a\nb: c")
}

@Test func completedAskExtractsAMultiLineReply() throws {
    let x = try #require(BotExchange(notice("a", status: "completed", reply: "Line 1\nLine 2: ok", intent: "ask")))
    #expect(x.reply == "Line 1\nLine 2: ok" && x.outcome == .done)
}

@Test func completedMessageHasNoReply() throws {
    var e = notice("a", status: "completed", intent: "message")
    e.data.text = "\(e.data.heading!)\nCompleted."
    let x = try #require(BotExchange(e))
    #expect(x.reply == nil && x.outcome == .done)
}

@Test func failedAndCancelledCarryTheDetail() {
    #expect(BotExchange(notice("a", status: "failed", detail: "boom"))?.outcome == .failed("boom"))
    #expect(BotExchange(notice("a", status: "cancelled", detail: "Cancelled by user."))?.outcome == .failed("Cancelled by user."))
}

@Test func queuedAndSentArePending() {
    #expect(BotExchange(notice("a", status: "queued"))?.outcome == .pending)
    #expect(BotExchange(notice("a", status: "sent"))?.outcome == .pending)
}

@Test func structuredRequestDoesNotDependOnNoticeHeading() throws {
    var e = notice("a")
    e.data.heading = "Something else: odd"
    e.data.text = "Something else: odd\n"
    #expect(try #require(BotExchange(e)).text == "Check this")
}

@Test func existingNoticeWithoutStructuredDataStaysPlain() {
    var e = notice("a")
    e.data.botMessage = nil
    #expect(BotExchange(e) == nil)
}

@Test func directionAndVerbFollowTheChat() throws {
    let sender = try #require(BotExchange(notice("a", chat: "egan")))
    #expect(sender.isOutgoing && sender.verb == "Messaged" && sender.peerId == "owen")
    let recipient = try #require(BotExchange(notice("a", chat: "owen")))
    #expect(!recipient.isOutgoing && recipient.verb == "Message from" && recipient.peerId == "egan")
}

@Test func outcomeFollowsTheStatus() throws {
    #expect(BotExchange(notice("a", status: "sent"))?.outcome == .pending)
    #expect(BotExchange(notice("a", status: "completed"))?.outcome == .done)
    #expect(BotExchange(notice("a", status: "cancelled"))?.outcome == .failed(""))
}

@Test func onlyNoticesWithStructuredDataAreExchanges() {
    var plain = notice("a")
    plain.data.botMessage = nil
    #expect(BotExchange(plain) == nil)
    var agent = notice("b")
    agent.kind = "agent"
    #expect(BotExchange(agent) == nil)
}

@Test func mergeKeepsTheHigherRevAndFiltersThePeer() {
    let fetched = [notice("a", seq: 1, rev: 1), notice("b", seq: 2, rev: 5, status: "completed"),
                   notice("c", seq: 3, target: "dex")]
    let live = [notice("b", seq: 2, rev: 3, status: "queued"), notice("d", seq: 4, rev: 2)]
    let merged = BotExchange.conversation(fetched: fetched, live: live, peer: "owen")
    #expect(merged.map(\.id) == ["a", "b", "d"])
    #expect(merged[1].outcome == .done)
}

@Test func rowsGroupAuthorsAndSeparateTimeBlocks() {
    let xs = [
        notice("a", seq: 1, at: 1_000, status: "completed", reply: "Done", intent: "ask"),
        notice("b", seq: 2, at: 2_000, status: "completed"),
        notice("c", seq: 3, at: 2_000 + 3_600_001, status: "queued"),
    ].compactMap(BotExchange.init)
    let rows = BotConversationRow.build(xs)
    #expect(rows.map(\.id) == ["sep-a", "a-request", "a-reply", "b-request", "sep-c", "c-request", "c-status"])
    #expect(rows[1] == .message(id: "a-request", author: "egan", text: "Check this", showsAuthor: true))
    #expect(rows[2] == .message(id: "a-reply", author: "owen", text: "Done", showsAuthor: true))
    #expect(rows[3] == .message(id: "b-request", author: "egan", text: "Check this", showsAuthor: true))
    #expect(rows[5] == .message(id: "c-request", author: "egan", text: "Check this", showsAuthor: true))
    #expect(rows[6] == .status(id: "c-status", outcome: .pending, awaiting: "owen"))
}

@Test func consecutiveRequestsFromOneBotShareAnAuthor() {
    let xs = [notice("a", seq: 1, status: "completed"), notice("b", seq: 2, at: 1_500, status: "completed")].compactMap(BotExchange.init)
    let rows = BotConversationRow.build(xs)
    #expect(rows[2] == .message(id: "b-request", author: "egan", text: "Check this", showsAuthor: false))
}

@Test func failedExchangeAddsACaptionAndResetsTheAuthor() {
    let xs = [notice("a", seq: 1, status: "failed", detail: "Recipient stopped."),
              notice("b", seq: 2, at: 1_500, status: "completed")].compactMap(BotExchange.init)
    let rows = BotConversationRow.build(xs)
    #expect(rows[2] == .status(id: "a-status", outcome: .failed("Recipient stopped."), awaiting: "owen"))
    #expect(rows[3] == .message(id: "b-request", author: "egan", text: "Check this", showsAuthor: true))
}

private func items(_ entries: [Entry]) -> [BotExchangeGroup.Item] { BotExchangeGroup.collapse(entries) }

private func plain(_ id: String, kind: String = "user", seq: Int64) -> Entry {
    Entry(id: id, seq: seq, botId: "egan", rev: 1, kind: kind, turn: 1, data: EntryData(text: "hi"), createdAt: 1_000, updatedAt: 1_000)
}

@Test func sameRunWithOnePeerFormsOneGroupAtItsFirstExchange() {
    let out = items([notice("a", seq: 1), notice("b", seq: 2, status: "completed"), notice("c", seq: 3)])
    #expect(out.count == 1)
    guard case .exchanges(let g) = out[0] else { Issue.record("not a group"); return }
    #expect(g.id == "a" && g.count == 3 && g.peerId == "owen" && g.title == "3 messages with")
    #expect(g.accessibilityLabel(peerName: "Owen") == "3 messages with Owen")
}

@Test func aSingleExchangeKeepsTheVerb() {
    guard case .exchanges(let g) = items([notice("a")])[0] else { Issue.record("not a group"); return }
    #expect(g.count == 1 && g.title == "Messaged")
}

@Test func aDifferentPeerSplitsTheRun() {
    let out = items([notice("a", seq: 1), notice("b", seq: 2, target: "dex"), notice("c", seq: 3, target: "dex")])
    #expect(out.map(\.id) == ["a", "b"])
}

@Test func aVisibleEntryBetweenExchangesSplitsTheRun() {
    let out = items([notice("a", seq: 1), plain("u", seq: 2), notice("b", seq: 3)])
    #expect(out.map(\.id) == ["a", "u", "b"])
    var routine = notice("r", seq: 2)
    routine.data.botMessage = nil
    #expect(items([notice("a", seq: 1), routine, notice("b", seq: 3)]).count == 3)
}

@Test func traceEntriesDoNotSplitTheRun() {
    let out = items([notice("a", seq: 1), plain("t", kind: "tool", seq: 2), plain("n", kind: "agent", seq: 3), notice("b", seq: 4)])
    #expect(out.map(\.id) == ["a"])
    guard case .exchanges(let g) = out[0] else { Issue.record("not a group"); return }
    #expect(g.count == 2)
}

@Test func directionsMixInOneGroup() {
    let out = items([notice("a", seq: 1, source: "egan", target: "owen"), notice("b", seq: 2, source: "owen", target: "egan")])
    guard case .exchanges(let g) = out[0] else { Issue.record("not a group"); return }
    #expect(out.count == 1 && g.count == 2 && g.peerId == "owen")
}

@Test func anyFailureMarksTheGroup() {
    let ok = items([notice("a", seq: 1, status: "completed"), notice("b", seq: 2, status: "completed")])
    let bad = items([notice("a", seq: 1, status: "completed"), notice("b", seq: 2, status: "failed")])
    guard case .exchanges(let good) = ok[0], case .exchanges(let failed) = bad[0] else { Issue.record("not groups"); return }
    #expect(!good.failed && failed.failed)
    #expect(failed.accessibilityLabel(peerName: "Owen") == "2 messages with Owen, failed")
}
