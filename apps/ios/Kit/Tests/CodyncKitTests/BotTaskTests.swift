import Foundation
import Testing
@testable import CodyncKit

private func task(_ id: String = "task-1", _ status: BotTask.Status = .working, title: String = "Fix login",
                  activity: String = "", startedAt: Int64 = 100) -> BotTask {
    BotTask(id: id, title: title, status: status, activity: activity, startedAt: startedAt)
}

private func entry(_ id: String, seq: Int64, rev: Int64, text: String = "", bot: String = "task-1") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":\#(seq),"botId":"\#(bot)","rev":\#(rev),"kind":"agent","turn":1,"data":{"text":"\#(text)"},"createdAt":1,"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

private func bot(_ fields: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Bot","rev":1\#(fields)}"#.utf8))
}

private let english: @Sendable (String) -> String = { $0 }
private let traditional: @Sendable (String) -> String = { L10n.tr($0, preference: .zhHant) }

@Suite struct BotTaskTests {
    @Test func aBotCarriesItsTasks() throws {
        let tasks = try bot(#","tasks":[{"id":"task-1","title":"Fix login","status":"needsInput","activity":"Reading","startedAt":5,"start":"fork"},{"id":"task-2","title":"Docs","status":"starting","activity":"","startedAt":9,"start":null}]"#).tasks
        #expect(tasks == [
            BotTask(id: "task-1", title: "Fix login", status: .needsInput, activity: "Reading", startedAt: 5),
            BotTask(id: "task-2", title: "Docs", status: .starting, startedAt: 9),
        ])
    }

    @Test func aHostFromBeforeTasksHasNone() throws {
        #expect(try bot("").tasks.isEmpty)
    }

    @Test func tasksThatDoNotReadLeaveTheBotUsable() throws {
        #expect(try bot(#","tasks":"soon""#).tasks.isEmpty)
        #expect(try bot(#","tasks":[{"title":"No id"}]"#).tasks.isEmpty)
        // A status a newer host adds, and missing fields, still read.
        #expect(try bot(#","tasks":[{"id":"t","status":"paused"}]"#).tasks == [BotTask(id: "t", title: "", status: .working, startedAt: 0)])
    }

    @Test func aTaskEntryEventCarriesItsTaskAndEntry() throws {
        let json = #"{"type":"taskEntry","rev":7,"task":"task-1","entry":{"id":"e1","seq":3,"botId":"task-1","rev":7,"kind":"tool","turn":1,"data":{"title":"Read file"},"createdAt":1,"updatedAt":1}}"#
        guard case let .taskEntry(task, entry)? = HostClient.parseEvent(Data(json.utf8)) else {
            Issue.record("not a task entry")
            return
        }
        #expect(task == "task-1")
        #expect(entry.id == "e1")
        #expect(entry.rev == 7)
        #expect(entry.data.title == "Read file")
    }

    @Test func aTaskEntryThatDoesNotReadIsPassedOver() {
        // Never a rewind: task entries are not in the catch-up.
        #expect(HostClient.parseEvent(Data(#"{"type":"taskEntry","rev":7,"task":"task-1"}"#.utf8)) == nil)
    }

    @Test func aReadTaskSaysHowItEnded() throws {
        let json = #"{"id":"task-1","title":"Fix login","status":"stopped","startedAt":5,"endedAt":65,"start":"brief"}"#
        let read = try JSONDecoder().decode(BotTask.self, from: Data(json.utf8))
        #expect(read.status == .stopped)
        #expect(!read.status.isLive)
        #expect(read.endedAt == 65)
    }

    @Test func oneTaskSaysWhatItDoes() {
        #expect([task("task-1", .needsInput, activity: "Reading")].capsuleLabel(english) == "In the background · Needs you")
        #expect([task("task-1", .starting, activity: "Reading")].capsuleLabel(english) == "In the background · Starting…")
        #expect([task(activity: "Reading files")].capsuleLabel(english) == "In the background · Reading files")
        #expect([task()].capsuleLabel(english) == "In the background · Fix login")
    }

    @Test func severalTasksSayHowManyAndWhetherOneNeedsYou() {
        let two = [task("a", startedAt: 300), task("b", .starting, startedAt: 200)]
        #expect(two.capsuleLabel(english) == "In the background · 2 tasks running")
        #expect(two.since == 200)
        let three = two + [task("c", .needsInput, startedAt: 400)]
        #expect(three.capsuleLabel(english) == "In the background · 3 tasks running · needs you")
        #expect([BotTask]().since == nil)
    }

    @Test func theRosterPreviewReadsLikeTheCapsule() {
        #expect([task(activity: "Reading files")].rosterPreview(english) == "In the background: Reading files")
        #expect([task("a"), task("b", .needsInput)].rosterPreview(english) == "In the background: 2 tasks running · needs you")
        #expect([task(activity: "Reading files")].rosterPreview(traditional) == "背景：Reading files")
    }

    @Test func theWordsAreInTraditionalChinese() {
        #expect([task("task-1", .needsInput)].capsuleLabel(traditional) == "背景 · 需要你")
        #expect([task("a"), task("b")].capsuleLabel(traditional) == "背景 · 2 件事")
        #expect(traditional("Stop the background work?") == "停止背景的工作？")
        #expect(traditional("What it already changed stays.") == "已經改的內容會留著。")
        for status in [BotTask.Status.starting, .working, .needsInput, .done, .failed, .stopped] {
            #expect(traditional(status.word) != status.word)
        }
    }

    @Test func aTranscriptKeepsEachEntryOnceAtItsHigherRev() throws {
        let held = [try entry("e1", seq: 1, rev: 5, text: "old"), try entry("e3", seq: 3, rev: 6)]
        let merged = held.mergingTaskEntries([
            try entry("e2", seq: 2, rev: 7),
            try entry("e1", seq: 1, rev: 8, text: "new"),
            try entry("e3", seq: 3, rev: 2, text: "late"),
        ])
        #expect(merged.map(\.id) == ["e1", "e2", "e3"])
        #expect(merged[0].data.text == "new")
        #expect(merged[2].rev == 6)
    }

    @Test func whatATaskWasToldIsNotTheUsersMessage() throws {
        let told = try JSONDecoder().decode(Entry.self, from: Data(#"{"id":"e1","seq":1,"botId":"task-1","rev":1,"kind":"user","turn":1,"data":{"text":"Fix it","from":"host"},"createdAt":1,"updatedAt":1}"#.utf8))
        #expect(told.isToldToTask)
        #expect(try !entry("e2", seq: 2, rev: 1).isToldToTask)
    }

    @Test func aCardNamesItsTaskAndReadsWithoutOne() throws {
        func data(_ json: String) throws -> EntryData { try JSONDecoder().decode(EntryData.self, from: Data(json.utf8)) }
        #expect(try data(#"{"task":{"id":"task-1","title":"Fix login"}}"#).task?.title == "Fix login")
        #expect(try data(#"{"title":"Run tests"}"#).task == nil)
        // Something else in its place never loses the card.
        #expect(try data(#"{"title":"Run tests","task":"task-1"}"#).task?.title == "")
        #expect(try data(#"{"title":"Run tests","task":{"id":7}}"#).title == "Run tests")
    }

    @Test func theDetailsReportTheirScreen() throws {
        #expect(AnalyticsEvent.Screen.task.rawValue == "task")
    }
}
