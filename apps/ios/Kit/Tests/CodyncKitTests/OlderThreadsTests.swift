import Foundation
import Testing
@testable import CodyncKit

private func row(_ id: String, bot: String = "b1", endedAt: Int64) -> String {
    #"{"botId":"\#(bot)","id":"\#(id)","title":"T \#(id)","status":"done","thread":"th-\#(id)","createdAt":1,"startedAt":1,"endedAt":\#(endedAt)}"#
}

private func page(_ rows: [String], more: Bool) throws -> OlderThreadsPage {
    try JSONDecoder().decode(OlderThreadsPage.self, from: Data(#"{"threads":[\#(rows.joined(separator: ","))],"more":\#(more)}"#.utf8))
}

private let hello = #"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0"#

@Suite struct OlderThreadsTests {
    @Test func helloSaysHowLongThreadsAreKept() throws {
        #expect(try JSONDecoder().decode(Hello.self, from: Data((hello + #","threadKeepDays":30}"#).utf8)).threadKeepDays == 30)
        // An older host lists no older threads.
        #expect(try JSONDecoder().decode(Hello.self, from: Data((hello + "}").utf8)).threadKeepDays == nil)
    }

    @Test func parsesKeepDaysEvents() {
        guard case .threadKeepDays(7) = HostClient.parseEvent(Data(#"{"type":"threadKeepDays","days":7}"#.utf8)) else {
            Issue.record("threadKeepDays"); return
        }
        #expect(HostClient.parseEvent(Data(#"{"type":"threadKeepDays"}"#.utf8)) == nil)
    }

    @Test func aPageKeepsTheRowsThatRead() throws {
        let read = try page([row("t1", endedAt: 50), #"{"botId":"b1","id":"bad"}"#, row("t2", bot: "box", endedAt: 40)], more: true)
        #expect(read.more)
        #expect(read.threads.map(\.botId) == ["b1", "box"])
        #expect(read.threads[0].task.thread == "th-t1")
        #expect(read.threads[0].task.status == .done)
    }

    @Test func pagesGoFurtherBackUntilTheHostHasNoMore() throws {
        var list = OlderThreadList()
        #expect(list.cursor == nil && list.more)
        list.add(try page([row("t1", endedAt: 50), row("t2", endedAt: 40)], more: true))
        #expect(list.cursor == 40 && list.more)
        // A page that doesn't reach further back stops the paging.
        list.add(try page([row("t2", endedAt: 40)], more: true))
        #expect(!list.more)
        var empty = OlderThreadList()
        empty.add(try page([], more: true))
        #expect(!empty.more)
        list.remove("t1", of: "b1")
        #expect(list.byBot["b1"]?.map(\.id) == ["t2", "t2"])
    }

    @Test func aThreadThatLeavesItsBotsWorkStaysAsItLastReadWithoutMovingTheCursor() throws {
        var list = OlderThreadList()
        list.add(try page([row("t1", endedAt: 50), row("t2", endedAt: 40)], more: true))
        let reopened = WorkTask(id: "t1", title: "Reopened", status: .done, endedAt: 90)
        let live = WorkTask(id: "t3", title: "Live", status: .working)
        list.keep("b1", was: [reopened, live], now: [live])
        #expect(list.byBot["b1"].map { WorkTask.merging([], older: $0).map(\.title) } == ["Reopened", "T t2"])
        #expect(list.cursor == 40)
        list.remove("t1", of: "b1")
        #expect(list.kept.isEmpty)
        // A dropped thread that then leaves its bot's work isn't kept.
        list.keep("b1", was: [reopened], now: [])
        #expect(list.kept.isEmpty)
    }

    @Test func theBotsOwnCopyWins() {
        let live = WorkTask(id: "t1", title: "Reopened", status: .working)
        let old = WorkTask(id: "t1", title: "Old", status: .done, endedAt: 5)
        let older = WorkTask(id: "t2", title: "Older", status: .done, endedAt: 3)
        #expect(WorkTask.merging([live], older: [old, older, older]).map(\.title) == ["Reopened", "Older"])
    }

    @Test func keepChoicesAndTheirWords() {
        #expect(ThreadKeep.choices(with: 30) == [0, 7, 30, 90])
        #expect(ThreadKeep.choices(with: 365) == [0, 7, 30, 90, 365])
        #expect(ThreadKeep.label(0) == "Keep forever")
        #expect(L10n.tr(ThreadKeep.label(45), preference: .zhHant) == "45 天後")
        #expect(L10n.tr(ThreadKeep.label(7), preference: .de) == "Nach 7 Tagen")
    }
}
