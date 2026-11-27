import Foundation
import Testing
@testable import CodyncKit

private func task(_ id: String = "i1", _ status: WorkTask.Status = .working, title: String = "Fix login", activity: String = "",
                  thread: String? = nil, wait: WorkTask.Wait? = nil, cutIn: Bool = false, startedAt: Int64? = 100,
                  endedAt: Int64? = nil, result: String = "", source: WorkTask.Source? = nil) -> WorkTask {
    WorkTask(id: id, title: title, status: status, activity: activity, thread: thread, wait: wait, cutIn: cutIn,
             startedAt: startedAt, endedAt: endedAt, result: result, source: source)
}

private func entry(_ id: String, seq: Int64, rev: Int64, text: String = "", bot: String = "thread-1") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":\#(seq),"botId":"\#(bot)","rev":\#(rev),"kind":"agent","turn":1,"data":{"text":"\#(text)"},"createdAt":1,"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

private func card(_ id: String, _ data: String, kind: String = "permission") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":1,"botId":"b1","rev":1,"kind":"\#(kind)","turn":1,"data":\#(data),"createdAt":1,"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

private func bot(_ fields: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Bot","rev":1\#(fields)}"#.utf8))
}

private func sections(_ tasks: WorkTask...) -> WorkSections { WorkSections(tasks) }

private let english: @Sendable (String) -> String = { $0 }
private let traditional: @Sendable (String) -> String = { L10n.tr($0, preference: .zhHant) }

@Suite struct WorkTaskTests {
    // MARK: the wire

    @Test func aThreadsBackgroundWorkReadsAsSentAndItsCapsuleSaysWhatRuns() throws {
        let work = try bot(#","work":[{"id":"t","title":"Review","status":"working","background":[{"id":"w1","kind":"workflow","name":"after-change","activity":"Review: diff","startedAt":7},{"id":"b1","kind":"future","name":"npm test","startedAt":9},{"name":"no id"}]},{"id":"o","title":"Old","status":"working"}]"#).work
        #expect(work[0].background == [
            .init(id: "w1", name: "after-change", activity: "Review: diff", startedAt: 7),
            .init(id: "b1", name: "npm test", startedAt: 9),
        ], "one without an id is left out")
        #expect(work[1].background.isEmpty, "an older host says nothing")
        #expect(work[1].backgroundLabel == nil)
        var one = work[0]
        one.background.removeLast()
        #expect(one.backgroundLabel?.text == "after-change · Review: diff")
        #expect(one.backgroundLabel?.since == 7)
        #expect(work[0].backgroundLabel?.text == "after-change, npm test")
    }

    @Test func aBotCarriesItsWork() throws {
        let work = try bot(#","work":[{"id":"i1","title":"Fix login","status":"needsYou","activity":"Reading","thread":"thread-1","wait":null,"cutIn":false,"createdAt":1,"startedAt":5,"endedAt":null,"result":""},{"id":"i2","title":"Docs","status":"waiting","activity":"","thread":null,"wait":{"kind":"behind","ahead":2},"cutIn":true,"createdAt":2},{"id":"i3","title":"CI","status":"failed","thread":"thread-2","createdAt":3,"startedAt":10,"endedAt":70,"result":"No network"}]"#).work
        #expect(work == [
            WorkTask(id: "i1", title: "Fix login", status: .needsYou, activity: "Reading", thread: "thread-1", createdAt: 1, startedAt: 5),
            WorkTask(id: "i2", title: "Docs", status: .waiting, wait: .init(.behind, ahead: 2), cutIn: true, createdAt: 2),
            WorkTask(id: "i3", title: "CI", status: .failed, thread: "thread-2", createdAt: 3, startedAt: 10, endedAt: 70, result: "No network"),
        ])
        #expect(work[2].timeTaken == .milliseconds(60))
        #expect(work[0].timeTaken == nil)
    }

    @Test func aTaskNamesTheAgentItsThreadRunsOn() throws {
        let owner = try bot(#","backend":"claude","work":[{"id":"a","status":"working","runsOn":"codex"},{"id":"b","status":"waiting"},{"id":"c","status":"working","runsOn":"","agent":"gemini"}]"#)
        #expect(owner.work.map { $0.agent(of: owner) } == ["codex", "claude", "gemini"])
        var plain = owner
        plain.plain = true
        #expect(owner.work[1].agent(of: plain) == nil, "the box of threads without a bot has no agent of its own")
    }

    @Test func aHostFromBeforeTheBoxHasNoWorkAndNoSetting() throws {
        let old = try bot(#","tasks":[{"id":"task-1","title":"Fix login","status":"working"}]"#)
        #expect(old.work.isEmpty)
        #expect(old.maxThreads == nil)
        #expect(BotDraft(old).maxThreads == nil)
    }

    @Test func workThatDoesNotReadLeavesTheBotUsable() throws {
        #expect(try bot(#","work":"soon""#).work.isEmpty)
        #expect(try bot(#","work":[{"title":"No id"}]"#).work.isEmpty)
        // A wait a newer host adds, and fields of the wrong kind, still read.
        let odd = try bot(#","work":[{"id":"i","status":"working","title":7,"thread":3,"wait":{"kind":"moon"},"cutIn":"yes","startedAt":"now","result":null}]"#).work
        #expect(odd == [WorkTask(id: "i", title: "", status: .working)])
        // A task whose status this app doesn't know, or that has none, is left out; the others stay.
        let newer = try bot(#","work":[{"id":"a","status":"paused"},{"id":"b","status":"waiting"},{"id":"c"},{"title":"No id","status":"done"},{"id":"d","status":7},"e",{"id":"f","status":"done"}]"#).work
        #expect(newer.map(\.id) == ["b", "f"])
        #expect(try bot(#","work":[{"id":"i","status":"waiting","wait":{"kind":"behind","ahead":"many"}}]"#).work[0].wait == .init(.behind))
        #expect(try bot(#","work":[{"id":"i","status":"waiting","wait":"next"}]"#).work[0].wait == nil)
    }

    @Test func aBotSaysHowManyThreadsRunAtOnce() throws {
        #expect(try bot(#","maxThreads":4"#).maxThreads == 4)
        #expect(try bot(#","maxThreads":"many""#).maxThreads == nil)
        #expect(try bot(#","maxThreads":null"#).maxThreads == nil)
        // Out of range reads as the nearest the setting allows.
        #expect(try bot(#","maxThreads":0"#).maxThreads == 1)
        #expect(try bot(#","maxThreads":40"#).maxThreads == 6)
    }

    @Test func theSettingIsSavedWithTheOtherFields() throws {
        var draft = BotDraft(try bot(#","maxThreads":3"#))
        #expect(draft.maxThreads == 3)
        draft.maxThreads = 5
        let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any]
        #expect(sent?["maxThreads"] as? Int == 5)
        // A change made elsewhere is kept unless this form changed it too.
        let base = BotDraft(try bot(#","maxThreads":3"#)), fresh = BotDraft(try bot(#","maxThreads":2"#))
        #expect(base.rebased(from: base, onto: fresh).maxThreads == 2)
        #expect(draft.rebased(from: base, onto: fresh).maxThreads == 5)
        // A host without the box is never sent one.
        let old = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: BotDraft(try bot(""))))) as? [String: Any]
        #expect(old?["maxThreads"] == nil)
    }

    @Test func aThreadEntryEventCarriesItsThreadAndEntry() throws {
        let json = #"{"type":"threadEntry","rev":7,"thread":"thread-1","entry":{"id":"e1","seq":3,"botId":"thread-1","rev":7,"kind":"tool","turn":1,"data":{"title":"Read file"},"createdAt":1,"updatedAt":1}}"#
        guard case let .threadEntry(thread, entry)? = HostClient.parseEvent(Data(json.utf8)) else {
            Issue.record("not a thread entry")
            return
        }
        #expect(thread == "thread-1")
        #expect(entry.id == "e1")
        #expect(entry.rev == 7)
        #expect(entry.data.title == "Read file")
    }

    @Test func aThreadEntryThatDoesNotReadIsPassedOver() {
        // Never a rewind: thread entries are not in the catch-up.
        #expect(HostClient.parseEvent(Data(#"{"type":"threadEntry","rev":7,"thread":"thread-1"}"#.utf8)) == nil)
    }

    /// Hosts before 2.51.0: goes when minHost reaches 2.51.0.
    @Test func aHostBeforeThreadsIsReadUnderItsOldNames() throws {
        let old = try bot(#","maxAgents":4,"taskWorktrees":true,"work":[{"id":"i1","status":"working","task":"task-1"}]"#)
        #expect(old.maxThreads == 4 && old.threadWorktrees == true)
        #expect(old.work.map(\.thread) == ["task-1"])

        let event = #"{"type":"taskEntry","rev":7,"task":"task-1","entry":{"id":"e1","seq":3,"botId":"task-1","rev":7,"kind":"tool","turn":1,"data":{"title":"Read file"},"createdAt":1,"updatedAt":1}}"#
        guard case let .threadEntry(thread, entry)? = HostClient.parseEvent(Data(event.utf8)) else {
            Issue.record("not a thread entry")
            return
        }
        #expect(thread == "task-1" && entry.id == "e1")

        // `task` as an object is the thread, `item` the task.
        let asked = try card("p1", #"{"status":"pending","task":{"id":"task-1","title":"Fix login"},"item":"i1"}"#)
        #expect(asked.data.thread?.id == "task-1" && asked.data.thread?.title == "Fix login")
        #expect(asked.data.task?.id == "i1")
        #expect(asked.data.isFromThread)
        #expect(try card("p2", #"{"status":"pending","item":"i2"}"#).data.task?.id == "i2")
        #expect([asked].card(for: task("i1", .needsYou, thread: "task-1"))?.id == "p1")
        // What the app keeps of it reads the same again, and the new shape is left as it is.
        let kept = try JSONDecoder().decode(Entry.self, from: JSONEncoder().encode(asked))
        #expect(kept == asked)
        let new = try card("p3", #"{"status":"pending","thread":{"id":"thread-1","title":"Docs"},"task":"i3"}"#)
        #expect(new.data.thread?.title == "Docs" && new.data.task?.id == "i3")
    }

    // MARK: the sections

    @Test func tasksGroupIntoThreadsEventBoxAndRecentInTheHostsOrder() {
        let work = sections(task("a"), task("b", .waiting), task("c", .needsYou), task("d", .done), task("e", .waiting),
                            task("f", .stopped), task("g"), task("h", .failed))
        #expect(work.needsYou.map(\.id) == ["c"])
        #expect(work.working.map(\.id) == ["a", "g"])
        // Threads: what needs the user, then what is worked on.
        #expect(work.live.map(\.id) == ["c", "a", "g"])
        // Event box: what waits, in the order it will be taken.
        #expect(work.waiting.map(\.id) == ["b", "e"])
        #expect(work.recent.map(\.id) == ["d", "f", "h"])
        #expect(!work.isIdle)
    }

    @Test func aBoxWithOnlyEndedTasksIsIdle() {
        #expect(sections().isIdle)
        #expect(sections(task("a", .done), task("b", .failed), task("c", .stopped)).isIdle)
        #expect(!sections(task("a", .waiting)).isIdle)
    }

    @Test func aRowOffersWhatItsTaskAllows() {
        let work = sections(task("a"), task("b", .needsYou), task("c", .waiting), task("d", .waiting), task("e", .done),
                            task("f", .failed), task("g", .stopped))
        func actions(_ id: String, max: Int?) -> [WorkAction] {
            work.actions(for: (work.live + work.waiting + work.recent).first { $0.id == id }!, maxThreads: max)
        }
        #expect(actions("a", max: 3) == [.stop])
        #expect(actions("b", max: 3) == [.stop])
        // First on every waiting row but the first; Cut in on the first while every thread is busy.
        #expect(actions("c", max: 3) == [.drop])
        #expect(actions("c", max: 2) == [.cutIn, .drop])
        #expect(actions("c", max: 1) == [.cutIn, .drop])
        #expect(actions("d", max: 2) == [.first, .drop])
        #expect(actions("e", max: 2) == [.drop])
        #expect(actions("f", max: 2) == [.retry, .drop])
        #expect(actions("g", max: 2) == [.retry, .drop])
        // A freed thread wouldn't start one that follows unfinished work or waits for usage: no Cut in.
        for kind in [WorkTask.Wait.Kind.after, .usage] {
            let held = task("h", .waiting, wait: .init(kind))
            #expect(sections(task("a"), held).actions(for: held, maxThreads: 1) == [.drop])
        }
    }

    @Test func threadsSayHowManyRunOfTheMostAllowedAndHowManyAreFree() {
        let work = sections(task("a"), task("b", .needsYou), task("c", .waiting), task("d", .done))
        #expect(work.threadsCount(maxThreads: 3) == "2 / 3")
        #expect(work.free(maxThreads: 3) == 1)
        #expect(work.free(maxThreads: 2) == 0)
        // More run than the setting now allows (it was lowered): none free, never a negative count.
        #expect(work.free(maxThreads: 1) == 0)
        #expect(work.threadsCount(maxThreads: 1) == "2 / 1")
        // A host that doesn't say: three.
        #expect(work.threadsCount(maxThreads: nil) == "2 / 3")
        #expect(sections(task("a", .waiting)).free(maxThreads: nil) == 3)
    }

    @Test func aThreadWaitingOnTheUserGivesItsSeatUpAsTheHostSays() throws {
        var working = task("a"), asking = task("b", .needsYou)
        working.seat = true
        asking.seat = false
        let work = sections(working, asking, task("c", .waiting))
        #expect(work.threadsCount(maxThreads: 2) == "1 / 2")
        #expect(work.free(maxThreads: 2) == 1)
        #expect(!work.canCutIn(maxThreads: 2), "a seat is free: nothing has to give way")
        // An older host says nothing: every live task takes one.
        #expect(sections(task("a"), task("b", .needsYou)).threadsCount(maxThreads: 2) == "2 / 2")
        let wire = try bot(#","work":[{"id":"b","status":"needsYou","seat":false}]"#)
        #expect(wire.work.first?.seat == false)
    }

    @Test func everyThreadIsBusyOnlyWhenAsManyAreLiveAsMayBe() {
        #expect(!sections(task("a", .waiting)).isFull(maxThreads: 1))
        #expect(sections(task("a"), task("b", .needsYou)).isFull(maxThreads: 2))
        #expect(!sections(task("a"), task("b", .needsYou)).isFull(maxThreads: 3))
        // A host that doesn't say: three.
        #expect(!sections(task("a"), task("b")).isFull(maxThreads: nil))
        #expect(sections(task("a"), task("b"), task("c")).isFull(maxThreads: nil))
    }

    // MARK: all bots' work

    @Test func theThreadsTabListsWhatNeedsYouThenTheGroupsThenTheRestNewestFirst() throws {
        func work(_ id: String, _ status: WorkTask.Status, at: Int64, folder: String? = nil, routine: String? = nil) -> WorkTask {
            WorkTask(id: id, title: routine == nil ? id : "Daily", status: status, createdAt: at, source: routine.map { .init(.routine, id: $0) },
                     folder: folder)
        }
        var a = try bot(#","cwd":"/a""#)
        a.work = [work("1", .done, at: 1, folder: "/p/x"), work("2", .needsYou, at: 5), work("3", .working, at: 3, routine: "r1"),
                  work("4", .done, at: 7, folder: "/a")]
        var b = try bot(#","cwd":"/b""#)
        b.name = "B"
        b.work = [work("5", .waiting, at: 4, folder: "/p/x"), work("6", .failed, at: 6)]
        let roster = [(key: "a", bot: a), (key: "b", bot: b)]
        func lists(_ grouping: ThreadGrouping, byStatus: Bool = false) -> (needsYou: [String], groups: [String], recent: [String]) {
            let made = ThreadSections(roster, grouping: grouping, byStatus: byStatus, key: \.key, computer: { _ in "Mac" }, bot: \.bot)
            return (made.needsYou.map(\.task.id), made.groups.map { "\($0.title):" + $0.rows.map(\.task.id).joined() }, made.recent.map(\.task.id))
        }
        // A folder of the bot's own, or none, is the rest; a routine's runs are their own group.
        let folders = lists(.folder)
        #expect(folders.needsYou == ["2"])
        #expect(folders.groups == ["x:51", "Daily:3"])
        #expect(folders.recent == ["4", "6"])
        // By bot, in the roster's order.
        #expect(lists(.bot).groups == ["Bot:431", "B:65"])
        #expect(lists(.computer).groups == ["Mac:46531"])
        // Sorted by status: working, then waiting, then the rest newest first.
        #expect(lists(.none, byStatus: true).recent == ["3", "5", "4", "6", "1"])
        // By last active: one added earlier but ended later comes first.
        var c = try bot(#","cwd":"/c""#)
        c.work = [WorkTask(id: "late", title: "late", status: .done, createdAt: 5), WorkTask(id: "early", title: "early", status: .done, createdAt: 1, endedAt: 9)]
        let moved = ThreadSections([(key: "c", bot: c)], grouping: .none, byActivity: true, key: \.key, computer: { _ in "Mac" }, bot: \.bot)
        #expect(moved.recent.map(\.task.id) == ["early", "late"])
        // Every thread in a folder Codync made for it, in one "Own folder" group, not one per folder.
        var box = try bot(#","cwd":"/home","plain":true"#)
        box.work = [WorkTask(id: "x", title: "x", status: .done, createdAt: 1, folder: "/home/.codync/threads/1", ownFolder: true),
                    WorkTask(id: "y", title: "y", status: .done, createdAt: 2, folder: "/home/.codync/threads/2", ownFolder: true)]
        let alone = ThreadSections([(key: "box", bot: box)], grouping: .folder, key: \.key, computer: { _ in "Mac" }, bot: \.bot)
        #expect(alone.groups.map { "\($0.key)=\($0.title):" + $0.rows.map(\.task.id).joined() } == ["folder:own=Own folder:yx"])
        // An empty folder names no place: the worktree's, if any.
        #expect(WorkTask(id: "x", title: "x", status: .done, folder: "").wherePath == nil)
        #expect(ThreadSections.working(in: roster, bot: \.bot) == 1)
        #expect(AnalyticsEvent.Screen.allWork.rawValue == "all_work")
    }

    @Test func threadSearchFindsThreadsByTitleOrBotNewestFirst() throws {
        var a = try bot("")
        a.name = "Ada"
        a.work = [WorkTask(id: "old", title: "Fix login", status: .done, thread: "t1", createdAt: 1),
                  WorkTask(id: "new", title: "Docs", status: .working, thread: "t2", createdAt: 3),
                  WorkTask(id: "none", title: "Fix CI", status: .waiting, createdAt: 4)]
        var box = try bot("")
        (box.name, box.plain) = ("Ada box", true)
        box.work = [WorkTask(id: "loose", title: "Loose fix", status: .done, thread: "t3", createdAt: 2)]
        let roster = [(key: "a", bot: a), (key: "box", bot: box)]
        let found = { (query: String) in ThreadSections.found(roster, matching: query, key: \.key, bot: \.bot).map(\.task.id) }
        // Only tasks with a thread, newest first, across bots.
        #expect(found("fix") == ["loose", "old"])
        // A bot's name finds its threads; the box's name finds none.
        #expect(found("ada") == ["new", "old"])
        #expect(found("zzz").isEmpty)
    }

    // MARK: the capsule

    @Test func oneWorkingTaskAndNothingWaitingSaysWhatItDoes() {
        let reading = sections(task(activity: "Reading files"))
        #expect(reading.capsuleLabel(english) == "Work · Reading files")
        #expect(reading.solo?.startedAt == 100)
        #expect(sections(task()).capsuleLabel(english) == "Work · Fix login")
        // What ended doesn't count.
        #expect(sections(task(activity: "Reading files"), task("b", .done)).capsuleLabel(english) == "Work · Reading files")
    }

    @Test func otherwiseTheCapsuleCounts() {
        #expect(sections(task("a"), task("b")).capsuleLabel(english) == "Work · 2 working")
        #expect(sections(task("a"), task("b"), task("c", .waiting), task("d", .waiting), task("e", .waiting)).capsuleLabel(english)
            == "Work · 2 working · 3 waiting")
        #expect(sections(task("a", activity: "Reading"), task("b", .waiting)).capsuleLabel(english) == "Work · 1 working · 1 waiting")
        #expect(sections(task("a", .waiting), task("b", .waiting)).capsuleLabel(english) == "Work · 2 waiting")
        #expect(sections(task("a"), task("b", .waiting)).solo == nil)
        #expect(sections(task("a"), task("b")).solo == nil)
    }

    @Test func needsYouComesFirstWithTheCountsAfterIt() {
        #expect(sections(task("a", .needsYou)).capsuleLabel(english) == "Work · Needs you")
        #expect(sections(task("a", .needsYou), task("b"), task("c")).capsuleLabel(english) == "Work · Needs you · 2 working")
        #expect(sections(task("a", .needsYou), task("b", .waiting)).capsuleLabel(english) == "Work · Needs you · 1 waiting")
        #expect(sections(task("a"), task("b", .needsYou), task("c", .waiting), task("d", .waiting), task("e", .waiting)).capsuleLabel(english)
            == "Work · Needs you · 1 working · 3 waiting")
        #expect(sections(task("a", .needsYou)).solo == nil)
    }

    @Test func theCapsulesStopIsForTheOneLiveTask() {
        #expect(sections(task("a"), task("b", .waiting)).stoppable?.id == "a")
        #expect(sections(task("a", .needsYou)).stoppable?.id == "a")
        #expect(sections(task("a"), task("b")).stoppable == nil)
        #expect(sections(task("a", .waiting)).stoppable == nil)
    }

    // MARK: the roster

    @Test func theRosterRowSaysTheMostPressingThing() throws {
        let work = #","work":[{"id":"a","status":"working"},{"id":"b","status":"working"},{"id":"c","status":"waiting"},{"id":"d","status":"done"}]"#
        let asking = #","work":[{"id":"a","status":"needsYou"},{"id":"b","status":"working"}]"#
        #expect(try bot(#","status":"needsInput""# + work).rosterLine == .needsYou)
        #expect(try bot(#","status":"working""# + asking).rosterLine == .needsYou)
        #expect(try bot(asking).rosterLine == .needsYou)
        #expect(try bot(#","status":"working""# + work).rosterLine == .activity)
        #expect(try bot(work).rosterLine == .work)
        #expect(try bot(#","status":"error""# + work).rosterLine == .work)
        #expect(try bot(#","work":[{"id":"c","status":"waiting"},{"id":"e","status":"waiting"},{"id":"d","status":"done"}]"#).rosterLine == .work)
        #expect(try bot(#","work":[{"id":"d","status":"done"}]"#).rosterLine == .lastMessage)
        #expect(try bot("").rosterLine == .lastMessage)
        // The chat's own request says what it asks; a task's only that it needs the user, as on every client.
        #expect(try bot(#","status":"needsInput","activity":"Run the tests""# + asking).needsYouLine == "Run the tests")
        #expect(try bot(#","status":"needsInput""#).needsYouLine == "Needs approval")
        #expect(try bot(asking).needsYouLine == "Needs you")
    }

    @Test func theRosterRowCountsWorkingAndWaitingEitherAloneWhenTheOtherIsZero() {
        #expect(sections(task("a"), task("b"), task("c", .waiting), task("d", .waiting), task("e", .waiting)).counts(english) == "2 working · 3 waiting")
        #expect(sections(task("a"), task("b"), task("d", .done)).counts(english) == "2 working")
        #expect(sections(task("c", .waiting)).counts(english) == "1 waiting")
        #expect(sections(task("d", .done)).counts(english) == "")
    }

    // MARK: the words

    @Test func aWaitingTaskSaysWhyItWaits() {
        #expect(task(wait: .init(.next)).waitLine(english) == "Next up")
        #expect(task(wait: .init(.behind, ahead: 2)).waitLine(english) == "2 ahead")
        #expect(task(wait: .init(.after)).waitLine(english) == "Follows what it works on")
        #expect(task(wait: .init(.usage)).waitLine(english) == "Near the usage limit: one at a time")
        #expect(task(wait: .init(.behind, ahead: 2), cutIn: true).waitLine(english) == "Was cut in on; starts again")
        // No reason given: it waits. None ahead after all: it is next.
        #expect(task().waitLine(english) == "Waiting")
        #expect(task(wait: .init(.behind)).waitLine(english) == "Next up")
    }

    @Test func anEndedTaskSaysHowItEnded() {
        #expect(task("a", .done, result: "Merged the fix").endLine(english) == "Done")
        #expect(task("a", .failed, result: "No network").endLine(english) == "No network")
        #expect(task("a", .failed).endLine(english) == "Failed")
        #expect(task("a", .stopped).endLine(english) == "Stopped")
    }

    @Test func theWordsAreInTraditionalChinese() {
        let words = [
            "Threads": "Threads", "Event box": "Event Box", "Free": "空位", "Waiting": "等待中", "Later": "之後", "Recent": "最近", "{count} working": "{count} 進行中",
            "{count} waiting": "{count} 等待中", "Next up": "下一個", "{count} ahead": "前面還有 {count} 件",
            "Follows what it works on": "接在進行中的事後面", "Near the usage limit: one at a time": "用量接近上限，一次只跑一個",
            "Was cut in on; starts again": "被插隊，會重新開始", "First": "先做", "Cut in": "插隊", "Drop": "丟掉",
            "Threads at once": "同時幾個 thread", "Let \"{title}\" cut in": "讓「{title}」插隊",
            "Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.": "所有 thread 都在忙。選一個讓位的 task：它的 thread 會關掉，task 之後重新開始。",
            "Stop this task?": "停止這個 task？", "What it already changed stays.": "已經改的內容會留著。",
        ]
        for (english, chinese) in words { #expect(traditional(english) == chinese) }
        #expect(traditional("Try again") != "Try again")
        #expect(sections(task("a", .needsYou), task("b"), task("c", .waiting)).capsuleLabel(traditional) == "工作 · 需要你 · 1 進行中 · 1 等待中")
        #expect(task(wait: .init(.behind, ahead: 3)).waitLine(traditional) == "前面還有 3 件")
        #expect(L10n.fill(traditional("Let \"{title}\" cut in"), ["title": "Fix login"]) == "讓「Fix login」插隊")
        for status in [WorkTask.Status.waiting, .working, .needsYou, .done, .failed, .stopped] {
            #expect(traditional(status.word) != status.word)
        }
        for action in [WorkAction.stop, .first, .cutIn, .drop, .retry] {
            #expect(traditional(action.title) != action.title)
        }
    }

    @Test func theNewWordsAreInEveryLanguage() {
        for language in [AppLanguage.zh, .ja, .ko, .fr, .de, .es, .ptBR, .ru, .it] {
            for word in ["Free", "Later", "{count} waiting", "Next up", "{count} ahead", "Follows what it works on",
                         "Near the usage limit: one at a time", "Was cut in on; starts again", "First", "Cut in", "Drop", "Threads at once",
                         "Let \"{title}\" cut in"] {
                #expect(L10n.tr(word, preference: language) != word, "\(word) in \(language)")
            }
        }
    }

    // MARK: transcripts and cards

    @Test func aTranscriptKeepsEachEntryOnceAtItsHigherRev() throws {
        let held = [try entry("e1", seq: 1, rev: 5, text: "old"), try entry("e3", seq: 3, rev: 6)]
        let merged = held.mergingThreadEntries([
            try entry("e2", seq: 2, rev: 7),
            try entry("e1", seq: 1, rev: 8, text: "new"),
            try entry("e3", seq: 3, rev: 2, text: "late"),
        ])
        #expect(merged.map(\.id) == ["e1", "e2", "e3"])
        #expect(merged[0].data.text == "new")
        #expect(merged[2].rev == 6)
    }

    @Test func aCardNamesItsThreadAndTaskAndReadsWithoutThem() throws {
        func data(_ json: String) throws -> EntryData { try JSONDecoder().decode(EntryData.self, from: Data(json.utf8)) }
        let named = try data(#"{"thread":{"id":"thread-1","title":"Fix login"},"task":"i1"}"#)
        #expect(named.thread?.title == "Fix login")
        #expect(named.task?.id == "i1")
        #expect(try data(#"{"title":"Run tests"}"#).thread == nil)
        #expect(try data(#"{"title":"Run tests"}"#).task == nil)
        // Something else in their place never loses the card.
        #expect(try data(#"{"title":"Run tests","thread":"thread-1","task":{"id":"i1"}}"#).thread?.title == "")
        #expect(try data(#"{"title":"Run tests","thread":{"id":7},"task":7}"#).title == "Run tests")
        #expect(try data(#"{"title":"Run tests","task":7}"#).task?.id == "")
    }

    @Test func aTaskThatNeedsYouFindsItsCardInTheChat() throws {
        let chat = [
            try card("p0", #"{"status":"answered","task":"i1"}"#),
            try card("p1", #"{"status":"pending","task":"i1","thread":{"id":"thread-1","title":"Fix login"}}"#),
            try card("p2", #"{"status":"pending","thread":{"id":"thread-2","title":"Docs"}}"#),
            try card("p3", #"{"status":"pending"}"#),
            try card("m1", #"{"status":"pending","task":"i9"}"#, kind: "agent"),
        ]
        #expect(chat.card(for: task("i1", .needsYou, thread: "thread-1"))?.id == "p1")
        // A host that names only the thread.
        #expect(chat.card(for: task("i2", .needsYou, thread: "thread-2"))?.id == "p2")
        #expect(chat.card(for: task("i9", .needsYou)) == nil)
    }

    @Test func aTaskThatNeedsYouIsAskedWhatItsCardInTheChatAsks() throws {
        let chat = [
            try card("e1", #"{"status":"pending","title":"npm test","task":"i1","thread":{"id":"thread-1","title":"Run the suite"}}"#),
            try card("e2", #"{"status":"allowed","title":"rm -rf build","task":"i2"}"#),
            try card("e3", #"{"status":"pending","title":"the chat's own card"}"#),
            try card("e5", #"{"status":"pending","decision":true,"question":"Which branch?","task":"i5"}"#, kind: "notice"),
            try card("e6", #"{"status":"answered","decision":true,"question":"Done already","task":"i6"}"#, kind: "notice"),
            try card("e4", #"{"status":"pending","title":"not a card","task":"i4"}"#, kind: "notice"),
            try card("e7", #"{"task":"i7","connectionRequest":{"kind":"agent","status":"pending","title":"Kimi Code","botId":"b"}}"#, kind: "notice"),
            try card("e8", #"{"task":"i8","connectionRequest":{"kind":"agent","status":"ready","title":"Codex","botId":"b"}}"#, kind: "notice"),
            // The turn left Kimi Code signed out, and its fallback was too: Kimi Code's card holds the task's message.
            try card("e9", #"{"task":"i9","connectionRequest":{"kind":"agent","status":"pending","title":"Kimi Code","botId":"b","entryIds":["m9"]}}"#, kind: "notice"),
            try card("e10", #"{"task":"i9","connectionRequest":{"kind":"agent","status":"pending","title":"Codex","botId":"b","entryIds":[]}}"#, kind: "notice"),
            // A sign-in that holds the task's message, then an approval its running turn now waits on.
            try card("e11", #"{"task":"i11","connectionRequest":{"kind":"agent","status":"pending","title":"Kimi Code","botId":"b","entryIds":["m11"]}}"#, kind: "notice"),
            try card("e12", #"{"status":"pending","title":"npm run build","task":"i11"}"#),
        ]
        // An approval's title, a choice's question, the agent a sign-in waits on.
        #expect(chat.card(for: task("i1", .needsYou))?.ask == "npm test")
        #expect(chat.card(for: task("i5", .needsYou))?.ask == "Which branch?")
        #expect(chat.card(for: task("i7", .needsYou))?.ask == "Needs Kimi Code sign-in")
        #expect(chat.card(for: task("i9", .needsYou))?.id == "e9", "the card whose sign-in sends the work again")
        #expect(chat.card(for: task("i11", .needsYou))?.id == "e12", "the approval the running turn waits on")
        for id in ["i2", "i4", "i6", "i8"] { #expect(chat.card(for: task(id, .needsYou)) == nil) }
    }

    @Test func whatAThreadPutInTheChatSaysSo() throws {
        func data(_ json: String) throws -> EntryData { try JSONDecoder().decode(EntryData.self, from: Data(json.utf8)) }
        #expect(try data(#"{"decision":true,"question":"Which?","thread":{"id":"thread-1","title":"Pick"},"task":"i1"}"#).isFromThread)
        #expect(try data(#"{"decision":true,"question":"Which?","task":"i1"}"#).isFromThread)
        // The chat's own card keeps the row for the user's own words.
        #expect(try !data(#"{"decision":true,"question":"Which?"}"#).isFromThread)
        #expect(try !data(#"{"decision":true,"question":"Which?","thread":"thread-1","task":7}"#).isFromThread)
        // Own words answer the chat's card unless only an option does (an agent's own question).
        #expect(try data(#"{"decision":true,"question":"Which?"}"#).takesOwnWords)
        #expect(try !data(#"{"decision":true,"question":"Which?","ownWords":false}"#).takesOwnWords)
        #expect(try !data(#"{"decision":true,"question":"Which?","task":"i1"}"#).takesOwnWords)
    }

    // MARK: worktrees

    @Test func aBotSaysWhetherItsThreadsGetWorktreesAndATaskNamesItsOwn() throws {
        let old = try bot("")
        #expect(old.threadWorktrees == nil && !old.gitRepo)
        #expect(BotDraft(old).threadWorktrees == nil)
        let repo = try bot(#","threadWorktrees":true,"gitRepo":true,"work":[{"id":"a","status":"working","worktree":{"branch":"codync/fix-login","path":"/w/a"}},{"id":"b","status":"working"},{"id":"c","status":"working","worktree":{"path":"/w/c"}},{"id":"d","status":"working","worktree":"main"}]"#)
        #expect(repo.threadWorktrees == true && repo.gitRepo)
        #expect(repo.work.map(\.worktree) == [.init(branch: "codync/fix-login", path: "/w/a"), nil, nil, nil])
        #expect(try bot(#","threadWorktrees":"yes","gitRepo":7"#).threadWorktrees == nil)
        // Saved with the other fields, and never sent to a host without the setting.
        var draft = BotDraft(repo)
        draft.threadWorktrees = false
        let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any]
        #expect(sent?["threadWorktrees"] as? Bool == false)
        let unsent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: BotDraft(old)))) as? [String: Any]
        #expect(unsent?["threadWorktrees"] == nil)
        let base = BotDraft(repo), fresh = BotDraft(try bot(#","threadWorktrees":false,"gitRepo":true"#))
        #expect(base.rebased(from: base, onto: fresh).threadWorktrees == false)
    }

    // MARK: outside the chat

    @Test func aBotSaysHowItStandsOutsideItsChatAndAnOlderHostsBotStandsAsItsChat() throws {
        let busy = try bot(#","status":"idle","outside":{"status":"working","startedAt":40},"work":[{"id":"a","status":"working"},{"id":"b","status":"working"},{"id":"c","status":"waiting"}]"#)
        #expect(busy.outside == .init(status: "working", startedAt: 40))
        #expect(busy.outsideStatus == "working" && busy.outsideStartedAt == 40)
        #expect(busy.isBusyOutside && !busy.needsYouOutside && !busy.isWorking)
        #expect(busy.outsideLine({ $0 }) == "2 working · 1 waiting")
        // The Live Activity from the app's own state stays up after the chat's turn, with the roster's line.
        let state = BotActivityAttributes.ContentState(outside: busy)
        #expect(state.status == "working" && state.startedAt == Date(milliseconds: 40))
        // An older host: the chat's own status and start.
        let old = try bot(#","status":"working","activity":"Reading","startedAt":7"#)
        #expect(old.outside == nil && old.outsideStatus == "working" && old.outsideStartedAt == 7 && old.isBusyOutside)
        #expect(old.outsideLine({ $0 }) == "Reading")
        #expect(try !bot("").isBusyOutside)
        #expect(try bot("").outsideLine({ $0 }) == "")
        // Something else in its place reads as none.
        #expect(try bot(#","status":"working","outside":"working""#).outsideStatus == "working")
        #expect(try bot(#","outside":{"startedAt":4}"#).outside == nil)
        #expect(try bot(#","outside":{"status":"working","startedAt":"now"}"#).outside == .init(status: "working"))
    }

    @Test func theLineOutsideTheChatSaysWhatIsMostPressing() throws {
        let asks = #","work":[{"id":"a","status":"needsYou"},{"id":"b","status":"working"}]"#
        // A task waits on the user, whatever the chat does.
        #expect(try bot(#","status":"working","activity":"Reading","outside":{"status":"needsInput"}"# + asks).outsideLine({ $0 }) == "Needs you")
        #expect(try bot(#","outside":{"status":"needsInput"}"# + asks).needsYouOutside)
        // The chat's own card, then its own activity.
        #expect(try bot(#","status":"needsInput","activity":"Run npm test""#).outsideLine({ $0 }) == "Run npm test")
        #expect(try bot(#","status":"working","activity":"Reading","work":[{"id":"b","status":"working"}]"#).outsideLine({ $0 }) == "Reading")
        // Over: the box is empty and the chat idle.
        let done = try bot(#","outside":{"status":"idle"},"work":[{"id":"a","status":"done"}]"#)
        #expect(!done.isBusyOutside && done.outsideLine({ $0 }) == "")
        // A task's report the chat hasn't taken yet keeps it up, with nothing to count.
        let reporting = try bot(#","outside":{"status":"working","startedAt":3},"work":[{"id":"a","status":"done"}]"#)
        #expect(reporting.isBusyOutside && reporting.outsideLine({ $0 }) == "")
        #expect(BotActivityAttributes.ContentState(outside: reporting).activity == "")
    }

    // MARK: sources

    @Test func aTaskSaysWhereItCameFrom() throws {
        let work = try bot(#","work":[{"id":"a","status":"working"},{"id":"b","status":"working","source":{"kind":"routine","id":"r1"}},{"id":"c","status":"working","source":{"kind":"request","id":"bot-2"}},{"id":"d","status":"working","source":{"kind":"request"}},{"id":"e","status":"working","source":{"kind":"webhook","id":"x"}},{"id":"f","status":"working","source":"routine"},{"id":"g","status":"working","source":{"kind":"request","id":7}}]"#).work
        #expect(work.map(\.source) == [nil, .init(.routine, id: "r1"), .init(.request, id: "bot-2"), .init(.request), nil, nil, .init(.request)])
    }

    @Test func tryAgainIsNotOfferedOnAnotherBotsRequestAndARoutinesRunOrARequestNeverGivesWay() {
        let routine = WorkTask.Source(.routine, id: "r1"), request = WorkTask.Source(.request, id: "bot-2")
        // Try again: failed or stopped, the bot's own or a routine's.
        #expect(task("a", .failed).canRetry)
        #expect(task("a", .stopped).canRetry)
        #expect(task("a", .stopped, source: routine).canRetry)
        #expect(!task("a", .failed, source: request).canRetry)
        #expect(!task("a", .stopped, source: .init(.request)).canRetry)
        for status in [WorkTask.Status.waiting, .working, .needsYou, .done] { #expect(!task("a", status).canRetry) }
        #expect(sections(task("a", .failed, source: request)).actions(for: task("a", .failed, source: request), maxThreads: 3) == [.drop])
        #expect(sections(task("a", .stopped, source: routine)).actions(for: task("a", .stopped, source: routine), maxThreads: 3) == [.retry, .drop])
        // Giving way: live, and the bot's own.
        #expect(task("a").givesWay)
        #expect(task("a", .needsYou).givesWay)
        #expect(!task("a", .waiting).givesWay)
        #expect(!task("a", source: routine).givesWay)
        #expect(!task("a", .needsYou, source: request).givesWay)
        // Cut in is offered only when some live task may give way, and only those are offered to give way.
        let waiting = task("w", .waiting, wait: .init(.next))
        #expect(!sections(task("a", source: routine), task("b", source: request), waiting).canCutIn(maxThreads: 2))
        let mixed = sections(task("a", source: routine), task("b"), waiting)
        #expect(mixed.canCutIn(maxThreads: 2))
        #expect(mixed.givingWay.map(\.id) == ["b"])
        #expect(mixed.actions(for: waiting, maxThreads: 2) == [.cutIn, .drop])
        // A routine task may cut in itself.
        #expect(sections(task("a"), task("w", .waiting, wait: .init(.next), source: routine)).canCutIn(maxThreads: 1))
        #expect(!sections(task("a")).canCutIn(maxThreads: 1))
    }

    @Test func onlyATaskHoldingASeatGivesWayAndAUsersThreadNever() throws {
        let work = try bot(#","work":[{"id":"own","status":"working","seat":true},{"id":"user","status":"working","seat":false},{"id":"asking","status":"needsYou","seat":false},{"id":"w","status":"waiting","wait":{"kind":"next"}}]"#).work
        let all = WorkSections(work)
        #expect(all.givingWay.map(\.id) == ["own"])
        #expect(all.canCutIn(maxThreads: 1))
        // Only the user's thread is busy: nothing may give way, so Cut in isn't offered.
        let usersOnly = WorkSections(work.filter { $0.id != "own" })
        #expect(usersOnly.givingWay.isEmpty && !usersOnly.canCutIn(maxThreads: 1))
        // An older host says nothing: every live task of the bot's own may give way.
        #expect(sections(task("a"), task("b", .needsYou)).givingWay.map(\.id) == ["b", "a"])
    }

    @Test func aBotsProjectsAndWorktreeModeAreSavedWhole() throws {
        func sent(_ draft: BotDraft) throws -> [String: Any] {
            try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
        }
        // An older host: neither is read nor sent.
        let old = try bot(#","cwd":"/p/app","gitRepo":true"#)
        #expect(old.projects == nil && old.worktreeMode == nil && old.folderName == "app")
        let unsent = try sent(BotDraft(old))
        #expect(unsent["projects"] == nil && unsent["worktreeMode"] == nil)

        let saved = try bot(#","cwd":"/h/bots/b1/workspace","managedWorkspace":true,"projects":["/p/a","/p/b"],"projectGitRepos":[true,false],"worktreeMode":"always","threadWorktrees":true"#)
        #expect(saved.projects == ["/p/a", "/p/b"] && saved.projectGitRepos == [true, false] && saved.worktreeMode == .always)
        #expect(saved.folderName == "a")
        #expect(saved.isGitProject("/p/a") && !saved.isGitProject("/p/b") && !saved.isGitProject("/p/c"))
        #expect(try bot(#","worktreeMode":"sometimes""#).worktreeMode == nil)
        var draft = BotDraft(saved)
        #expect(draft.cwd.isEmpty)
        // Add (once), make default, remove: the whole list goes, the default first.
        draft.addProject("/p/c")
        draft.addProject("/p/a")
        #expect(draft.projects == ["/p/a", "/p/b", "/p/c"])
        draft.makeDefaultProject("/p/c")
        #expect(draft.projects == ["/p/c", "/p/a", "/p/b"])
        draft.removeProject("/p/a")
        draft.worktreeMode = .off
        let body = try sent(draft)
        #expect(body["projects"] as? [String] == ["/p/c", "/p/b"])
        #expect(body["worktreeMode"] as? String == "off")
        // Removing the default makes the next one it; removing the last sends an empty list.
        draft.removeProject("/p/c")
        draft.removeProject("/p/b")
        #expect(try sent(draft)["projects"] as? [String] == [])
        // A new bot on a host with projects: its first folder is the default.
        var fresh = BotDraft(name: "New")
        fresh.addProject("/p/x")
        #expect(fresh.projects == ["/p/x"])
        // A change made elsewhere is kept unless the form changed the same field.
        let base = BotDraft(saved)
        let elsewhere = BotDraft(try bot(#","projects":["/p/b"],"worktreeMode":"off""#))
        #expect(base.rebased(from: base, onto: elsewhere).projects == ["/p/b"])
        #expect(base.rebased(from: base, onto: elsewhere).worktreeMode == .off)
        var edited = base
        edited.makeDefaultProject("/p/b")
        #expect(edited.rebased(from: base, onto: elsewhere).projects == ["/p/b", "/p/a"])
    }

    @Test func aBotsSharedFoldersAreSavedWholeAndStayInsideTheProject() throws {
        func sent(_ draft: BotDraft) throws -> [String: Any] {
            try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any])
        }
        // An older host: not read, not sent.
        #expect(try sent(BotDraft(try bot(#","projects":["/p/a"]"#)))["sharedFolders"] == nil)

        var draft = BotDraft(try bot(#","projects":["/p/a","/p/b"],"projectGitRepos":[true,true],"sharedFolders":{"/p/a":["node_modules"]}"#))
        #expect(draft.sharedFolders(of: "/p/a") == ["node_modules"] && draft.sharedFolders(of: "/p/b").isEmpty)
        // Trimmed, once; absolute names and `..` are refused, as the host would.
        let added = draft.addSharedFolder(" .cache/pip/ ", to: "/p/a")
        #expect(added)
        draft.addSharedFolder("node_modules", to: "/p/a")
        let refused = ["/etc", "C:\\x", "a/../..", "  "].filter { !draft.addSharedFolder($0, to: "/p/a") }
        #expect(refused.count == 4)
        #expect(BotDraft.sharedFolderName("./node_modules") == "node_modules")
        draft.addSharedFolder("vendor", to: "/p/b")
        var body = try sent(draft)
        #expect(body["sharedFolders"] as? [String: [String]] == ["/p/a": ["node_modules", ".cache/pip"], "/p/b": ["vendor"]])
        // Removing a project's last folder, or the project, takes its entry off.
        draft.removeSharedFolder("vendor", from: "/p/b")
        draft.removeProject("/p/a")
        body = try sent(draft)
        #expect(body["sharedFolders"] as? [String: [String]] == [:])
    }

    @Test func eachWorktreeModeIsTranslated() {
        for mode in WorktreeMode.allCases {
            #expect(L10n.tr(mode.line, preference: .zhHant) != mode.line)
            #expect(L10n.tr(mode.title, preference: .zhHant) != mode.title)
        }
    }

    @Test func theTranscriptReportsItsScreen() throws {
        #expect(AnalyticsEvent.Screen.thread.rawValue == "thread")
    }
}
