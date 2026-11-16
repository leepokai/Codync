import Foundation
import Testing
@testable import CodyncKit

private func item(_ id: String = "i1", _ status: WorkItem.Status = .working, title: String = "Fix login", activity: String = "",
                  task: String? = nil, wait: WorkItem.Wait? = nil, cutIn: Bool = false, startedAt: Int64? = 100,
                  endedAt: Int64? = nil, result: String = "", source: WorkItem.Source? = nil) -> WorkItem {
    WorkItem(id: id, title: title, status: status, activity: activity, task: task, wait: wait, cutIn: cutIn,
             startedAt: startedAt, endedAt: endedAt, result: result, source: source)
}

private func entry(_ id: String, seq: Int64, rev: Int64, text: String = "", bot: String = "task-1") throws -> Entry {
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

private func sections(_ items: WorkItem...) -> WorkSections { WorkSections(items) }

private let english: @Sendable (String) -> String = { $0 }
private let traditional: @Sendable (String) -> String = { L10n.tr($0, preference: .zhHant) }

@Suite struct WorkItemTests {
    // MARK: the wire

    @Test func aBotCarriesItsWork() throws {
        let work = try bot(#","work":[{"id":"i1","title":"Fix login","status":"needsYou","activity":"Reading","task":"task-1","wait":null,"cutIn":false,"createdAt":1,"startedAt":5,"endedAt":null,"result":""},{"id":"i2","title":"Docs","status":"waiting","activity":"","task":null,"wait":{"kind":"behind","ahead":2},"cutIn":true,"createdAt":2},{"id":"i3","title":"CI","status":"failed","task":"task-2","createdAt":3,"startedAt":10,"endedAt":70,"result":"No network"}]"#).work
        #expect(work == [
            WorkItem(id: "i1", title: "Fix login", status: .needsYou, activity: "Reading", task: "task-1", createdAt: 1, startedAt: 5),
            WorkItem(id: "i2", title: "Docs", status: .waiting, wait: .init(.behind, ahead: 2), cutIn: true, createdAt: 2),
            WorkItem(id: "i3", title: "CI", status: .failed, task: "task-2", createdAt: 3, startedAt: 10, endedAt: 70, result: "No network"),
        ])
        #expect(work[2].timeTaken == .milliseconds(60))
        #expect(work[0].timeTaken == nil)
    }

    @Test func aHostFromBeforeTheBoxHasNoWorkAndNoSetting() throws {
        let old = try bot(#","tasks":[{"id":"task-1","title":"Fix login","status":"working"}]"#)
        #expect(old.work.isEmpty)
        #expect(old.maxAgents == nil)
        #expect(BotDraft(old).maxAgents == nil)
    }

    @Test func workThatDoesNotReadLeavesTheBotUsable() throws {
        #expect(try bot(#","work":"soon""#).work.isEmpty)
        #expect(try bot(#","work":[{"title":"No id"}]"#).work.isEmpty)
        // A wait a newer host adds, and fields of the wrong kind, still read.
        let odd = try bot(#","work":[{"id":"i","status":"working","title":7,"task":3,"wait":{"kind":"moon"},"cutIn":"yes","startedAt":"now","result":null}]"#).work
        #expect(odd == [WorkItem(id: "i", title: "", status: .working)])
        // An item whose status this app doesn't know, or that has none, is left out; the others stay.
        let newer = try bot(#","work":[{"id":"a","status":"paused"},{"id":"b","status":"waiting"},{"id":"c"},{"title":"No id","status":"done"},{"id":"d","status":7},"e",{"id":"f","status":"done"}]"#).work
        #expect(newer.map(\.id) == ["b", "f"])
        #expect(try bot(#","work":[{"id":"i","status":"waiting","wait":{"kind":"behind","ahead":"many"}}]"#).work[0].wait == .init(.behind))
        #expect(try bot(#","work":[{"id":"i","status":"waiting","wait":"next"}]"#).work[0].wait == nil)
    }

    @Test func aBotSaysHowManyAgentsWorkAtOnce() throws {
        #expect(try bot(#","maxAgents":4"#).maxAgents == 4)
        #expect(try bot(#","maxAgents":"many""#).maxAgents == nil)
        #expect(try bot(#","maxAgents":null"#).maxAgents == nil)
        // Out of range reads as the nearest the setting allows.
        #expect(try bot(#","maxAgents":0"#).maxAgents == 1)
        #expect(try bot(#","maxAgents":40"#).maxAgents == 6)
    }

    @Test func theSettingIsSavedWithTheOtherFields() throws {
        var draft = BotDraft(try bot(#","maxAgents":3"#))
        #expect(draft.maxAgents == 3)
        draft.maxAgents = 5
        let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any]
        #expect(sent?["maxAgents"] as? Int == 5)
        // A change made elsewhere is kept unless this form changed it too.
        let base = BotDraft(try bot(#","maxAgents":3"#)), fresh = BotDraft(try bot(#","maxAgents":2"#))
        #expect(base.rebased(from: base, onto: fresh).maxAgents == 2)
        #expect(draft.rebased(from: base, onto: fresh).maxAgents == 5)
        // A host without the box is never sent one.
        let old = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: BotDraft(try bot(""))))) as? [String: Any]
        #expect(old?["maxAgents"] == nil)
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

    // MARK: the sections

    @Test func itemsGroupIntoThreadsEventBoxAndRecentInTheHostsOrder() {
        let work = sections(item("a"), item("b", .waiting), item("c", .needsYou), item("d", .done), item("e", .waiting),
                            item("f", .stopped), item("g"), item("h", .failed))
        #expect(work.needsYou.map(\.id) == ["c"])
        #expect(work.working.map(\.id) == ["a", "g"])
        // Threads: what needs the user, then what is worked on.
        #expect(work.live.map(\.id) == ["c", "a", "g"])
        // Event box: what waits, in the order it will be taken.
        #expect(work.waiting.map(\.id) == ["b", "e"])
        #expect(work.recent.map(\.id) == ["d", "f", "h"])
        #expect(!work.isIdle)
    }

    @Test func aBoxWithOnlyEndedItemsIsIdle() {
        #expect(sections().isIdle)
        #expect(sections(item("a", .done), item("b", .failed), item("c", .stopped)).isIdle)
        #expect(!sections(item("a", .waiting)).isIdle)
    }

    @Test func aRowOffersWhatItsItemAllows() {
        let work = sections(item("a"), item("b", .needsYou), item("c", .waiting), item("d", .waiting), item("e", .done),
                            item("f", .failed), item("g", .stopped))
        func actions(_ id: String, max: Int?) -> [WorkAction] {
            work.actions(for: (work.live + work.waiting + work.recent).first { $0.id == id }!, maxAgents: max)
        }
        #expect(actions("a", max: 3) == [.stop])
        #expect(actions("b", max: 3) == [.stop])
        // First on every waiting row but the first; Cut in on the first while every place is taken.
        #expect(actions("c", max: 3) == [.drop])
        #expect(actions("c", max: 2) == [.cutIn, .drop])
        #expect(actions("c", max: 1) == [.cutIn, .drop])
        #expect(actions("d", max: 2) == [.first, .drop])
        #expect(actions("e", max: 2) == [.drop])
        #expect(actions("f", max: 2) == [.retry, .drop])
        #expect(actions("g", max: 2) == [.retry, .drop])
        // A freed place wouldn't start one that follows unfinished work or waits for usage: no Cut in.
        for kind in [WorkItem.Wait.Kind.after, .usage] {
            let held = item("h", .waiting, wait: .init(kind))
            #expect(sections(item("a"), held).actions(for: held, maxAgents: 1) == [.drop])
        }
    }

    @Test func threadsSayHowManyRunOfTheMostAllowedAndHowManyPlacesAreFree() {
        let work = sections(item("a"), item("b", .needsYou), item("c", .waiting), item("d", .done))
        #expect(work.threadsCount(maxAgents: 3) == "2 / 3")
        #expect(work.free(maxAgents: 3) == 1)
        #expect(work.free(maxAgents: 2) == 0)
        // More run than the setting now allows (it was lowered): none free, never a negative count.
        #expect(work.free(maxAgents: 1) == 0)
        #expect(work.threadsCount(maxAgents: 1) == "2 / 1")
        // A host that doesn't say: three.
        #expect(work.threadsCount(maxAgents: nil) == "2 / 3")
        #expect(sections(item("a", .waiting)).free(maxAgents: nil) == 3)
    }

    @Test func everyPlaceIsTakenOnlyWhenAsManyAreLiveAsMayBe() {
        #expect(!sections(item("a", .waiting)).isFull(maxAgents: 1))
        #expect(sections(item("a"), item("b", .needsYou)).isFull(maxAgents: 2))
        #expect(!sections(item("a"), item("b", .needsYou)).isFull(maxAgents: 3))
        // A host that doesn't say: three.
        #expect(!sections(item("a"), item("b")).isFull(maxAgents: nil))
        #expect(sections(item("a"), item("b"), item("c")).isFull(maxAgents: nil))
    }

    // MARK: all bots' work

    @Test func theViewOfAllBotsListsTheOnesWithLiveOrWaitingItemsInRosterOrderAndCountsWhatIsWorkedOn() {
        let roster: [(ref: String, work: [WorkItem])] = [
            ("idle", []),
            ("ended", [item("a", .done), item("b", .stopped), item("c", .failed)]),
            ("busy", [item("d"), item("e"), item("f", .waiting), item("g", .done)]),
            ("asks", [item("h", .needsYou)]),
            ("queued", [item("i", .waiting)]),
            ("one", [item("j")]),
        ]
        let all = AllWork(roster) { $0.work }
        #expect(all.bots.map(\.ref) == ["busy", "asks", "queued", "one"])
        // Items that need the user or wait are not counted as working.
        #expect(all.working == 3)
        // Nothing live or waiting: no button, and an open view closes.
        let none = AllWork(Array(roster.prefix(2))) { $0.work }
        #expect(none.bots.isEmpty && none.working == 0)
        // The button shows without a count.
        let quiet = AllWork([roster[3], roster[4]]) { $0.work }
        #expect(quiet.bots.count == 2 && quiet.working == 0)
        #expect(AnalyticsEvent.Screen.allWork.rawValue == "all_work")
    }

    // MARK: the capsule

    @Test func oneWorkingItemAndNothingWaitingSaysWhatItDoes() {
        let reading = sections(item(activity: "Reading files"))
        #expect(reading.capsuleLabel(english) == "In the background · Reading files")
        #expect(reading.solo?.startedAt == 100)
        #expect(sections(item()).capsuleLabel(english) == "In the background · Fix login")
        // What ended doesn't count.
        #expect(sections(item(activity: "Reading files"), item("b", .done)).capsuleLabel(english) == "In the background · Reading files")
    }

    @Test func otherwiseTheCapsuleCounts() {
        #expect(sections(item("a"), item("b")).capsuleLabel(english) == "In the background · 2 working")
        #expect(sections(item("a"), item("b"), item("c", .waiting), item("d", .waiting), item("e", .waiting)).capsuleLabel(english)
            == "In the background · 2 working · 3 waiting")
        #expect(sections(item("a", activity: "Reading"), item("b", .waiting)).capsuleLabel(english) == "In the background · 1 working · 1 waiting")
        #expect(sections(item("a", .waiting), item("b", .waiting)).capsuleLabel(english) == "In the background · 2 waiting")
        #expect(sections(item("a"), item("b", .waiting)).solo == nil)
        #expect(sections(item("a"), item("b")).solo == nil)
    }

    @Test func needsYouComesFirstWithTheCountsAfterIt() {
        #expect(sections(item("a", .needsYou)).capsuleLabel(english) == "In the background · Needs you")
        #expect(sections(item("a", .needsYou), item("b"), item("c")).capsuleLabel(english) == "In the background · Needs you · 2 working")
        #expect(sections(item("a", .needsYou), item("b", .waiting)).capsuleLabel(english) == "In the background · Needs you · 1 waiting")
        #expect(sections(item("a"), item("b", .needsYou), item("c", .waiting), item("d", .waiting), item("e", .waiting)).capsuleLabel(english)
            == "In the background · Needs you · 1 working · 3 waiting")
        #expect(sections(item("a", .needsYou)).solo == nil)
    }

    @Test func theCapsulesStopIsForTheOneLiveItem() {
        #expect(sections(item("a"), item("b", .waiting)).stoppable?.id == "a")
        #expect(sections(item("a", .needsYou)).stoppable?.id == "a")
        #expect(sections(item("a"), item("b")).stoppable == nil)
        #expect(sections(item("a", .waiting)).stoppable == nil)
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
        // The chat's own request says what it asks; an item's only that it needs the user, as on every client.
        #expect(try bot(#","status":"needsInput","activity":"Run the tests""# + asking).needsYouLine == "Run the tests")
        #expect(try bot(#","status":"needsInput""#).needsYouLine == "Needs approval")
        #expect(try bot(asking).needsYouLine == "Needs you")
    }

    @Test func theRosterRowCountsWorkingAndWaitingEitherAloneWhenTheOtherIsZero() {
        #expect(sections(item("a"), item("b"), item("c", .waiting), item("d", .waiting), item("e", .waiting)).counts(english) == "2 working · 3 waiting")
        #expect(sections(item("a"), item("b"), item("d", .done)).counts(english) == "2 working")
        #expect(sections(item("c", .waiting)).counts(english) == "1 waiting")
        #expect(sections(item("d", .done)).counts(english) == "")
    }

    // MARK: the words

    @Test func aWaitingItemSaysWhyItWaits() {
        #expect(item(wait: .init(.next)).waitLine(english) == "Next up")
        #expect(item(wait: .init(.behind, ahead: 2)).waitLine(english) == "2 ahead")
        #expect(item(wait: .init(.after)).waitLine(english) == "Follows what it works on")
        #expect(item(wait: .init(.usage)).waitLine(english) == "Waiting for usage")
        #expect(item(wait: .init(.behind, ahead: 2), cutIn: true).waitLine(english) == "Was cut in on; starts again")
        // No reason given: it waits. None ahead after all: it is next.
        #expect(item().waitLine(english) == "Waiting")
        #expect(item(wait: .init(.behind)).waitLine(english) == "Next up")
    }

    @Test func anEndedItemSaysHowItEnded() {
        #expect(item("a", .done, result: "Merged the fix").endLine(english) == "Done")
        #expect(item("a", .failed, result: "No network").endLine(english) == "No network")
        #expect(item("a", .failed).endLine(english) == "Failed")
        #expect(item("a", .stopped).endLine(english) == "Stopped")
    }

    @Test func theWordsAreInTraditionalChinese() {
        let words = [
            "Threads": "Threads", "Event box": "Event Box", "Free": "空位", "Waiting": "等待中", "Later": "之後", "Recent": "最近", "{count} working": "{count} 進行中",
            "{count} waiting": "{count} 等待中", "Next up": "下一個", "{count} ahead": "前面還有 {count} 件",
            "Follows what it works on": "接在進行中的事後面", "Waiting for usage": "等用量恢復",
            "Was cut in on; starts again": "被插隊，會重新開始", "First": "先做", "Cut in": "插隊", "Drop": "丟掉",
            "Agents at once": "同時幾個 agent", "Let \"{title}\" cut in": "讓「{title}」插隊",
            "Every place is taken. Pick what gives way: its session closes and it starts again later.": "名額滿了。選一個讓位：它的 session 會關掉，之後重新開始。",
            "Stop the background work?": "停止背景的工作？", "What it already changed stays.": "已經改的內容會留著。",
        ]
        for (english, chinese) in words { #expect(traditional(english) == chinese) }
        #expect(traditional("Try again") != "Try again")
        #expect(sections(item("a", .needsYou), item("b"), item("c", .waiting)).capsuleLabel(traditional) == "背景 · 需要你 · 1 進行中 · 1 等待中")
        #expect(item(wait: .init(.behind, ahead: 3)).waitLine(traditional) == "前面還有 3 件")
        #expect(L10n.fill(traditional("Let \"{title}\" cut in"), ["title": "Fix login"]) == "讓「Fix login」插隊")
        for status in [WorkItem.Status.waiting, .working, .needsYou, .done, .failed, .stopped] {
            #expect(traditional(status.word) != status.word)
        }
        for action in [WorkAction.stop, .first, .cutIn, .drop, .retry] {
            #expect(traditional(action.title) != action.title)
        }
    }

    @Test func theNewWordsAreInEveryLanguage() {
        for language in [AppLanguage.zh, .ja, .ko, .fr, .de, .es, .ptBR, .ru, .it] {
            for word in ["Free", "Later", "{count} waiting", "Next up", "{count} ahead", "Follows what it works on",
                         "Waiting for usage", "Was cut in on; starts again", "First", "Cut in", "Drop", "Agents at once",
                         "Let \"{title}\" cut in"] {
                #expect(L10n.tr(word, preference: language) != word, "\(word) in \(language)")
            }
        }
    }

    // MARK: transcripts and cards

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

    @Test func aCardNamesItsTaskAndItemAndReadsWithoutThem() throws {
        func data(_ json: String) throws -> EntryData { try JSONDecoder().decode(EntryData.self, from: Data(json.utf8)) }
        let named = try data(#"{"task":{"id":"task-1","title":"Fix login"},"item":"i1"}"#)
        #expect(named.task?.title == "Fix login")
        #expect(named.item?.id == "i1")
        #expect(try data(#"{"title":"Run tests"}"#).task == nil)
        #expect(try data(#"{"title":"Run tests"}"#).item == nil)
        // Something else in their place never loses the card.
        #expect(try data(#"{"title":"Run tests","task":"task-1","item":{"id":"i1"}}"#).task?.title == "")
        #expect(try data(#"{"title":"Run tests","task":{"id":7},"item":7}"#).title == "Run tests")
        #expect(try data(#"{"title":"Run tests","item":7}"#).item?.id == "")
    }

    @Test func anItemThatNeedsYouFindsItsCardInTheChat() throws {
        let chat = [
            try card("p0", #"{"status":"answered","item":"i1"}"#),
            try card("p1", #"{"status":"pending","item":"i1","task":{"id":"task-1","title":"Fix login"}}"#),
            try card("p2", #"{"status":"pending","task":{"id":"task-2","title":"Docs"}}"#),
            try card("p3", #"{"status":"pending"}"#),
            try card("m1", #"{"status":"pending","item":"i9"}"#, kind: "agent"),
        ]
        #expect(chat.card(for: item("i1", .needsYou, task: "task-1"))?.id == "p1")
        // A host that names only the task.
        #expect(chat.card(for: item("i2", .needsYou, task: "task-2"))?.id == "p2")
        #expect(chat.card(for: item("i9", .needsYou)) == nil)
    }

    @Test func anItemThatNeedsYouIsAskedWhatItsCardInTheChatAsks() throws {
        let chat = [
            try card("e1", #"{"status":"pending","title":"npm test","item":"i1","task":{"id":"task-1","title":"Run the suite"}}"#),
            try card("e2", #"{"status":"allowed","title":"rm -rf build","item":"i2"}"#),
            try card("e3", #"{"status":"pending","title":"the chat's own card"}"#),
            try card("e5", #"{"status":"pending","decision":true,"question":"Which branch?","item":"i5"}"#, kind: "notice"),
            try card("e6", #"{"status":"answered","decision":true,"question":"Done already","item":"i6"}"#, kind: "notice"),
            try card("e4", #"{"status":"pending","title":"not a card","item":"i4"}"#, kind: "notice"),
        ]
        // An approval's title, a choice's question.
        #expect(chat.card(for: item("i1", .needsYou))?.ask == "npm test")
        #expect(chat.card(for: item("i5", .needsYou))?.ask == "Which branch?")
        for id in ["i2", "i4", "i6"] { #expect(chat.card(for: item(id, .needsYou)) == nil) }
    }

    @Test func whatATaskPutInTheChatSaysSo() throws {
        func data(_ json: String) throws -> EntryData { try JSONDecoder().decode(EntryData.self, from: Data(json.utf8)) }
        #expect(try data(#"{"decision":true,"question":"Which?","task":{"id":"task-1","title":"Pick"},"item":"i1"}"#).isFromTask)
        #expect(try data(#"{"decision":true,"question":"Which?","item":"i1"}"#).isFromTask)
        // The chat's own card keeps the row for the user's own words.
        #expect(try !data(#"{"decision":true,"question":"Which?"}"#).isFromTask)
        #expect(try !data(#"{"decision":true,"question":"Which?","task":"task-1","item":7}"#).isFromTask)
    }

    // MARK: worktrees

    @Test func aBotSaysWhetherItsAgentsGetWorktreesAndAnItemNamesItsOwn() throws {
        let old = try bot("")
        #expect(old.taskWorktrees == nil && !old.gitRepo)
        #expect(BotDraft(old).taskWorktrees == nil)
        let repo = try bot(#","taskWorktrees":true,"gitRepo":true,"work":[{"id":"a","status":"working","worktree":{"branch":"codync/fix-login","path":"/w/a"}},{"id":"b","status":"working"},{"id":"c","status":"working","worktree":{"path":"/w/c"}},{"id":"d","status":"working","worktree":"main"}]"#)
        #expect(repo.taskWorktrees == true && repo.gitRepo)
        #expect(repo.work.map(\.worktree) == [.init(branch: "codync/fix-login", path: "/w/a"), nil, nil, nil])
        #expect(try bot(#","taskWorktrees":"yes","gitRepo":7"#).taskWorktrees == nil)
        // Saved with the other fields, and never sent to a host without the setting.
        var draft = BotDraft(repo)
        draft.taskWorktrees = false
        let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: draft))) as? [String: Any]
        #expect(sent?["taskWorktrees"] as? Bool == false)
        let unsent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotUpdate(draft: BotDraft(old)))) as? [String: Any]
        #expect(unsent?["taskWorktrees"] == nil)
        let base = BotDraft(repo), fresh = BotDraft(try bot(#","taskWorktrees":false,"gitRepo":true"#))
        #expect(base.rebased(from: base, onto: fresh).taskWorktrees == false)
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
        // An item waits on the user, whatever the chat does.
        #expect(try bot(#","status":"working","activity":"Reading","outside":{"status":"needsInput"}"# + asks).outsideLine({ $0 }) == "Needs you")
        #expect(try bot(#","outside":{"status":"needsInput"}"# + asks).needsYouOutside)
        // The chat's own card, then its own activity.
        #expect(try bot(#","status":"needsInput","activity":"Run npm test""#).outsideLine({ $0 }) == "Run npm test")
        #expect(try bot(#","status":"working","activity":"Reading","work":[{"id":"b","status":"working"}]"#).outsideLine({ $0 }) == "Reading")
        // Over: the box is empty and the chat idle.
        let done = try bot(#","outside":{"status":"idle"},"work":[{"id":"a","status":"done"}]"#)
        #expect(!done.isBusyOutside && done.outsideLine({ $0 }) == "")
        // An item's report the chat hasn't taken yet keeps it up, with nothing to count.
        let reporting = try bot(#","outside":{"status":"working","startedAt":3},"work":[{"id":"a","status":"done"}]"#)
        #expect(reporting.isBusyOutside && reporting.outsideLine({ $0 }) == "")
        #expect(BotActivityAttributes.ContentState(outside: reporting).activity == "")
    }

    // MARK: sources

    @Test func anItemSaysWhereItCameFrom() throws {
        let work = try bot(#","work":[{"id":"a","status":"working"},{"id":"b","status":"working","source":{"kind":"routine","id":"r1"}},{"id":"c","status":"working","source":{"kind":"request","id":"bot-2"}},{"id":"d","status":"working","source":{"kind":"request"}},{"id":"e","status":"working","source":{"kind":"webhook","id":"x"}},{"id":"f","status":"working","source":"routine"},{"id":"g","status":"working","source":{"kind":"request","id":7}}]"#).work
        #expect(work.map(\.source) == [nil, .init(.routine, id: "r1"), .init(.request, id: "bot-2"), .init(.request), nil, nil, .init(.request)])
    }

    @Test func tryAgainIsNotOfferedOnAnotherBotsRequestAndARoutinesRunOrARequestNeverGivesWay() {
        let routine = WorkItem.Source(.routine, id: "r1"), request = WorkItem.Source(.request, id: "bot-2")
        // Try again: failed or stopped, the bot's own or a routine's.
        #expect(item("a", .failed).canRetry)
        #expect(item("a", .stopped).canRetry)
        #expect(item("a", .stopped, source: routine).canRetry)
        #expect(!item("a", .failed, source: request).canRetry)
        #expect(!item("a", .stopped, source: .init(.request)).canRetry)
        for status in [WorkItem.Status.waiting, .working, .needsYou, .done] { #expect(!item("a", status).canRetry) }
        #expect(sections(item("a", .failed, source: request)).actions(for: item("a", .failed, source: request), maxAgents: 3) == [.drop])
        #expect(sections(item("a", .stopped, source: routine)).actions(for: item("a", .stopped, source: routine), maxAgents: 3) == [.retry, .drop])
        // Giving way: live, and the bot's own.
        #expect(item("a").givesWay)
        #expect(item("a", .needsYou).givesWay)
        #expect(!item("a", .waiting).givesWay)
        #expect(!item("a", source: routine).givesWay)
        #expect(!item("a", .needsYou, source: request).givesWay)
        // Cut in is offered only when some live item may give way, and only those are offered to give way.
        let waiting = item("w", .waiting, wait: .init(.next))
        #expect(!sections(item("a", source: routine), item("b", source: request), waiting).canCutIn(maxAgents: 2))
        let mixed = sections(item("a", source: routine), item("b"), waiting)
        #expect(mixed.canCutIn(maxAgents: 2))
        #expect(mixed.givingWay.map(\.id) == ["b"])
        #expect(mixed.actions(for: waiting, maxAgents: 2) == [.cutIn, .drop])
        // A routine item may cut in itself.
        #expect(sections(item("a"), item("w", .waiting, wait: .init(.next), source: routine)).canCutIn(maxAgents: 1))
        #expect(!sections(item("a")).canCutIn(maxAgents: 1))
    }

    @Test func theTranscriptReportsItsScreen() throws {
        #expect(AnalyticsEvent.Screen.task.rawValue == "task")
    }
}
