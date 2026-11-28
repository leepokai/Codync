import Foundation
import Testing
@testable import CodyncKit

private func bot(_ fields: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Bot","rev":1\#(fields)}"#.utf8))
}

private func entry(_ id: String, seq: Int64, kind: String = "agent", data: String = "{}", at: Int64 = 1, botId: String = "thread-1") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":\#(seq),"botId":"\#(botId)","rev":1,"kind":"\#(kind)","turn":1,"data":\#(data),"createdAt":\#(at),"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

/// A task's thread: who may write to it, where it works, and what its transcript is made of.
@Suite struct WorkThreadTests {
    @Test func theHostSaysWhichTasksTakeAMessage() throws {
        let work = try bot(#","work":[{"id":"a","status":"working","thread":"thread-1","talk":true,"folder":"/Users/kai/site"},{"id":"b","status":"done","thread":"thread-2","source":{"kind":"routine","id":"r1"}},{"id":"c","status":"working","thread":"thread-3","talk":"yes","source":{"kind":"request"}},{"id":"d","status":"waiting"}]"#).work
        #expect(work.map(\.talk) == [true, false, false, false])
        #expect(work.map(\.readOnlyLine) == [nil, "Read only: a routine's run", "Read only: another bot's request", nil])
        #expect(work[0].folder == "/Users/kai/site")
        #expect(work[0].workThread(of: "b1") == WorkThread(botId: "b1", taskId: "a", threadId: "thread-1"))
        #expect(work[0].workThread(of: "b1")?.draftKey == "b1/item/a")
        // No thread took it: nothing to write to.
        #expect(work[3].workThread(of: "b1") == nil)
    }

    @Test func aFolderStartsAtTheComputersHome() {
        #expect(WhereLine.homeRelative("/Users/kai/code/site", home: "/Users/kai") == "~/code/site")
        #expect(WhereLine.homeRelative("/Users/kai", home: "/Users/kai/") == "~")
        #expect(WhereLine.homeRelative("/root/app", home: "/root") == "~/app")
        #expect(WhereLine.homeRelative(#"C:\Users\kai\code\site"#, home: #"C:\Users\kai"#) == #"~\code\site"#)
        // Another folder that only starts the same, anywhere else, and a computer that names no home: as it is.
        #expect(WhereLine.homeRelative("/Users/kaito/site", home: "/Users/kai") == "/Users/kaito/site")
        #expect(WhereLine.homeRelative("/Users/Shared/site", home: "/Users/kai") == "/Users/Shared/site")
        #expect(WhereLine.homeRelative("/srv/www/app/site", home: "/Users/kai") == "/srv/www/app/site")
        #expect(WhereLine.homeRelative("/srv/site", home: "") == "/srv/site")
    }

    @Test func theComputersCopyOfAMessageReplacesTheOneShownAtOnce() throws {
        let local = try entry(Entry.localId("n1"), seq: 9, kind: "user", data: #"{"text":"hi","status":"failed","clientNonce":"n1"}"#)
        let other = try entry(Entry.localId("n2"), seq: 10, kind: "user", data: #"{"text":"yo","status":"sending","clientNonce":"n2"}"#)
        let hosts = try entry("e5", seq: 5, kind: "user", data: #"{"text":"hi","status":"sent","clientNonce":"n1"}"#)
        // Whichever side holds the computer's copy: a transcript read again after the send failed, too.
        #expect([local, other].mergingThreadEntries([hosts]).map(\.id) == ["e5", "local-n2"])
        #expect([hosts].mergingThreadEntries([local, other]).map(\.id) == ["e5", "local-n2"])
    }

    @Test func aTaskSaysWhereItWorks() {
        func task(folder: String? = nil, worktree: WorkTask.Worktree? = nil) -> WorkTask {
            WorkTask(id: "i", title: "Fix login", status: .working, worktree: worktree, folder: folder)
        }
        let home = "/Users/kai"
        #expect(task().whereLine(home: home) == nil)
        #expect(task(folder: "").whereLine(home: home) == nil)
        #expect(task(folder: "/Users/kai/site").whereLine(home: home) == WhereLine(kind: .folder, text: "~/site", path: "/Users/kai/site"))
        // A worktree of its own: the branch, and the folder its thread is in to copy.
        let tree = WorkTask.Worktree(branch: "codync/fix-login", path: "/w/a")
        #expect(task(folder: "/w/a/app", worktree: tree).whereLine(home: home) == WhereLine(kind: .branch, text: "codync/fix-login", path: "/w/a/app"))
        #expect(task(worktree: tree).whereLine(home: home) == WhereLine(kind: .branch, text: "codync/fix-login", path: "/w/a"))
        #expect(task(worktree: .init(branch: "main")).whereLine(home: home)?.path == nil)
        // The folder Codync made for a thread without a bot: named so, its path still the one to copy.
        let own = WorkTask(id: "o", title: "Alone", status: .working, folder: "/Users/kai/.codync/threads/1", ownFolder: true)
        #expect(own.whereLine(home: home) == WhereLine(kind: .folder, text: "Own folder", path: "/Users/kai/.codync/threads/1"))
        #expect(WhereLine.Kind.folder.symbol == "folder" && WhereLine.Kind.branch.symbol == "arrow.triangle.branch")
    }

    @Test func aBotSaysWhatItsFolderIsToGit() throws {
        #expect(try bot(#","gitRepo":true,"gitInstalled":true"#).gitState == .repo)
        #expect(try bot(#","gitRepo":false,"gitInstalled":true"#).gitState == .none)
        #expect(try bot(#","gitRepo":false,"gitInstalled":false"#).gitState == .missing)
        // A host that doesn't say has git.
        #expect(try bot(#","gitRepo":true"#).gitState == .repo)
        #expect(try bot(#","gitInstalled":"no""#).gitState == .none)
        #expect([Bot.GitState.repo, .none, .missing].map(\.line)
            == ["Git repository", "Not a git repository", "Git isn't installed on this computer"])
    }

    @Test func aThreadIsWhatItWasToldThenTheRest() throws {
        let told = try entry("e1", seq: 1, kind: "user", data: #"{"text":"Fix it","from":"host"}"#)
        #expect(told.isToldToThread)
        let entries = [
            told,
            try entry("e2", seq: 2, kind: "tool"),
            try entry("e3", seq: 3, kind: "user", data: #"{"text":"Also the docs","status":"queued"}"#),
            try entry("e4", seq: 4, kind: "user", data: #"{"text":"Never mind","status":"cancelled","removed":true}"#),
            try entry("e5", seq: 5),
            // Told again when the task was picked up: it stays where it happened.
            try entry("e6", seq: 6, kind: "user", data: #"{"text":"Pick it up","from":"host"}"#),
            try entry("e7", seq: 7),
        ]
        #expect(entries.threadTranscript.told?.id == "e1")
        #expect(entries.threadTranscript.rest.map(\.id) == ["e2", "e3", "e5", "e6", "e7"])
        #expect(entries.threadTranscript.rest.filter(\.isToldToThread).map(\.id) == ["e6"])
        // A first one taken out leaves the next on top; none leaves nothing folded.
        #expect(Array(entries.dropFirst()).threadTranscript.told?.id == "e6")
        #expect(Array(entries[1..<5]).threadTranscript.told == nil)
    }

    @Test func whatTheChatPassedOnIsQuotedNeverQueued() throws {
        let entries = [
            try entry("e1", seq: 1, kind: "user", data: #"{"text":"Fix it","from":"host"}"#),
            try entry("e2", seq: 2, kind: "user", data: #"{"text":"Also the docs","status":"queued"}"#),
            try entry("e3", seq: 3, kind: "user", data: #"{"text":"And the tests","from":"host","relay":true,"status":"queued"}"#),
        ].threadTranscript
        #expect(entries.queue(working: true).map(\.id) == ["e2"])
        #expect(entries.hidingQueue(working: true).map(\.id) == ["e3"])
        #expect(entries.queue(working: false).isEmpty)
        #expect(entries.hidingQueue(working: false).map(\.id) == ["e2", "e3"])
    }

    @Test func aThreadShowsWhatItPutInTheChatWhereItCame() throws {
        let thread = #""thread":{"id":"thread-1","title":"Fix login"}"#
        let own = [try entry("t1", seq: 1, kind: "tool", at: 10), try entry("t2", seq: 2, data: #"{"final":true}"#, at: 30)]
        let chat = [
            try entry("c1", seq: 1, data: #"{"final":true,"files":[],\#(thread)}"#, at: 5, botId: "b1"),
            try entry("c2", seq: 2, kind: "permission", data: #"{"status":"allowed",\#(thread)}"#, at: 20, botId: "b1"),
            try entry("c3", seq: 3, data: #"{"final":true}"#, at: 21, botId: "b1"),
            try entry("c4", seq: 4, kind: "notice", data: #"{"decision":true,"status":"pending",\#(thread)}"#, at: 30, botId: "b1"),
        ]
        // Answered cards too, and the thread's own first at the same moment; the chat's own entries stay in the chat.
        #expect(own.sharing(chat, of: "thread-1").map(\.id) == ["c1", "t1", "c2", "t2", "c4"])
        #expect(own.sharing(chat, of: "thread-9").map(\.id) == ["t1", "t2"])
    }

    @Test func theNewWordsAreInEveryLanguage() {
        for language in [AppLanguage.zhHant, .zh, .ja, .ko, .fr, .de, .es, .ptBR, .ru, .it] {
            // `Git repository` reads the same in Chinese.
            for word in ["Not a git repository", "Git isn't installed on this computer",
                         "About working folders and git", "Message this thread", "What it was told",
                         "Copy path", "Path copied", "Near the usage limit: one at a time", "Read only: a routine's run",
                         "Read only: another bot's request"] {
                #expect(L10n.tr(word, preference: language) != word, "\(word) in \(language)")
            }
        }
    }
}
