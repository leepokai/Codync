import Foundation
import Testing
@testable import CodyncKit

private func bot(_ fields: String) throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Bot","rev":1\#(fields)}"#.utf8))
}

private func entry(_ id: String, seq: Int64, kind: String = "agent", data: String = "{}", at: Int64 = 1, botId: String = "task-1") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":\#(seq),"botId":"\#(botId)","rev":1,"kind":"\#(kind)","turn":1,"data":\#(data),"createdAt":\#(at),"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

/// An item's thread: who may write to it, where it works, and what its transcript is made of.
@Suite struct WorkThreadTests {
    @Test func theHostSaysWhichItemsTakeAMessage() throws {
        let work = try bot(#","work":[{"id":"a","status":"working","task":"task-1","talk":true,"folder":"/Users/kai/site"},{"id":"b","status":"done","task":"task-2","source":{"kind":"routine","id":"r1"}},{"id":"c","status":"working","task":"task-3","talk":"yes","source":{"kind":"request"}},{"id":"d","status":"waiting"}]"#).work
        #expect(work.map(\.talk) == [true, false, false, false])
        #expect(work.map(\.readOnlyLine) == [nil, "Read only: a routine's run", "Read only: another bot's request", nil])
        #expect(work[0].folder == "/Users/kai/site")
        #expect(work[0].thread(of: "b1") == WorkThread(botId: "b1", itemId: "a", task: "task-1"))
        #expect(work[0].thread(of: "b1")?.draftKey == "b1/item/a")
        // No task took it: nothing to write to.
        #expect(work[3].thread(of: "b1") == nil)
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
        #expect([local, other].mergingTaskEntries([hosts]).map(\.id) == ["e5", "local-n2"])
        #expect([hosts].mergingTaskEntries([local, other]).map(\.id) == ["e5", "local-n2"])
    }

    @Test func anItemSaysWhereItWorks() {
        func item(folder: String? = nil, worktree: WorkItem.Worktree? = nil) -> WorkItem {
            WorkItem(id: "i", title: "Fix login", status: .working, worktree: worktree, folder: folder)
        }
        let home = "/Users/kai"
        #expect(item().whereLine(home: home) == nil)
        #expect(item(folder: "").whereLine(home: home) == nil)
        #expect(item(folder: "/Users/kai/site").whereLine(home: home) == WhereLine(kind: .folder, text: "~/site", path: "/Users/kai/site"))
        // A worktree of its own: the branch, and the folder its task is in to copy.
        let tree = WorkItem.Worktree(branch: "codync/fix-login", path: "/w/a")
        #expect(item(folder: "/w/a/app", worktree: tree).whereLine(home: home) == WhereLine(kind: .branch, text: "codync/fix-login", path: "/w/a/app"))
        #expect(item(worktree: tree).whereLine(home: home) == WhereLine(kind: .branch, text: "codync/fix-login", path: "/w/a"))
        #expect(item(worktree: .init(branch: "main")).whereLine(home: home)?.path == nil)
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

    @Test func aThreadIsWhatItsTaskWasToldThenTheRest() throws {
        let told = try entry("e1", seq: 1, kind: "user", data: #"{"text":"Fix it","from":"host"}"#)
        #expect(told.isToldToTask)
        let entries = [
            told,
            try entry("e2", seq: 2, kind: "tool"),
            try entry("e3", seq: 3, kind: "user", data: #"{"text":"Also the docs","status":"queued"}"#),
            try entry("e4", seq: 4, kind: "user", data: #"{"text":"Never mind","status":"cancelled","removed":true}"#),
            try entry("e5", seq: 5),
            // Told again when the item was picked up: it stays where it happened.
            try entry("e6", seq: 6, kind: "user", data: #"{"text":"Pick it up","from":"host"}"#),
            try entry("e7", seq: 7),
        ]
        #expect(entries.threadEntries.told?.id == "e1")
        #expect(entries.threadEntries.rest.map(\.id) == ["e2", "e3", "e5", "e6", "e7"])
        #expect(entries.threadEntries.rest.filter(\.isToldToTask).map(\.id) == ["e6"])
        // A first one taken out leaves the next on top; none leaves nothing folded.
        #expect(Array(entries.dropFirst()).threadEntries.told?.id == "e6")
        #expect(Array(entries[1..<5]).threadEntries.told == nil)
    }

    @Test func aThreadShowsWhatItsTaskPutInTheChatWhereItCame() throws {
        let task = #""task":{"id":"task-1","title":"Fix login"}"#
        let own = [try entry("t1", seq: 1, kind: "tool", at: 10), try entry("t2", seq: 2, data: #"{"final":true}"#, at: 30)]
        let chat = [
            try entry("c1", seq: 1, data: #"{"final":true,"files":[],\#(task)}"#, at: 5, botId: "b1"),
            try entry("c2", seq: 2, kind: "permission", data: #"{"status":"allowed",\#(task)}"#, at: 20, botId: "b1"),
            try entry("c3", seq: 3, data: #"{"final":true}"#, at: 21, botId: "b1"),
            try entry("c4", seq: 4, kind: "notice", data: #"{"decision":true,"status":"pending",\#(task)}"#, at: 30, botId: "b1"),
        ]
        // Answered cards too, and the task's own first at the same moment; the chat's own entries stay in the chat.
        #expect(own.sharing(chat, of: "task-1").map(\.id) == ["c1", "t1", "c2", "t2", "c4"])
        #expect(own.sharing(chat, of: "task-9").map(\.id) == ["t1", "t2"])
    }

    @Test func whatATaskPutInTheChatIsOneLineOfItsThread() throws {
        func line(_ kind: String, _ data: String) throws -> SharedLine? {
            try entry("c", seq: 1, kind: kind, data: data, botId: "b1").sharedLine { $0 }
        }
        let options = #""options":[{"optionId":"ok","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Deny","kind":"reject_once"},{"optionId":"x","name":"Skip","kind":"other"}]"#
        func approval(_ status: String, _ selected: String = "") throws -> SharedLine? {
            try line("permission", #"{"status":"\#(status)","title":"npm test",\#(options),"selected":"\#(selected)"}"#)
        }
        #expect(try approval("pending") == SharedLine(symbol: "hand.raised", text: "Waiting for your approval: npm test"))
        #expect(try approval("answered", "ok") == SharedLine(symbol: "checkmark", text: "Approved: npm test"))
        #expect(try approval("answered", "no") == SharedLine(symbol: "xmark", text: "Denied: npm test"))
        #expect(try approval("answered", "x")?.text == "Answered: Skip")
        #expect(try approval("expired") == SharedLine(symbol: "clock", text: "Not answered: npm test"))
        func choice(_ status: String, _ selected: String = "") throws -> SharedLine? {
            try line("notice", #"{"decision":true,"status":"\#(status)","question":"Squash?","choices":["Yes","No"],"selected":"\#(selected)"}"#)
        }
        #expect(try choice("pending")?.text == "Asked: Squash?")
        #expect(try choice("answered", "No")?.text == "Answered: No")
        #expect(try choice("expired")?.text == "Not answered: Squash?")
        func signIn(_ status: String, kind: String = "agent") throws -> SharedLine? {
            try line("notice", #"{"text":"Needs you to sign in.","connectionRequest":{"kind":"\#(kind)","status":"\#(status)","title":"Kimi Code","botId":"b1"}}"#)
        }
        #expect(try signIn("pending") == SharedLine(symbol: "hand.raised", text: "Needs Kimi Code sign-in"))
        #expect(try signIn("ready") == SharedLine(symbol: "checkmark", text: "Signed in"))
        #expect(try signIn("cancelled") == SharedLine(symbol: "xmark", text: "Cancelled"))
        #expect(try signIn("pending", kind: "oauth") == nil, "a connector's card is not a task's")
        let file = #"{"id":"f","name":"log.txt","size":3,"sha256":""}"#
        #expect(try line("agent", #"{"final":true,"files":[\#(file)]}"#) == SharedLine(symbol: "doc", text: "Sent a file: log.txt"))
        #expect(try line("agent", #"{"final":true,"files":[\#(file)],"voiceNote":true,"text":"Hi"}"#) == SharedLine(symbol: "waveform", text: "Sent a voice note"))
        #expect(try line("agent", #"{"final":true,"artifact":{"id":"a","title":"Plan","updatedAt":0}}"#) == SharedLine(symbol: "doc.richtext", text: "Made an artifact: Plan"))
        #expect(try line("agent", #"{"text":"Just words","final":true}"#) == nil)
        #expect(try line("notice", #"{"text":"A notice"}"#) == nil)
    }

    @Test func theNewWordsAreInEveryLanguage() {
        for language in [AppLanguage.zhHant, .zh, .ja, .ko, .fr, .de, .es, .ptBR, .ru, .it] {
            // `Git repository` reads the same in Chinese.
            for word in ["Not a git repository", "Git isn't installed on this computer",
                         "About working folders and git", "Message this thread", "What it was told", "Back to details",
                         "Copy path", "Path copied", "Near the usage limit: one at a time", "Read only: a routine's run",
                         "Read only: another bot's request", "Waiting for your approval: {title}", "Approved: {title}",
                         "Denied: {title}", "Asked: {question}", "Answered: {choice}", "Not answered: {text}",
                         "Sent a file: {name}", "Sent a voice note", "Made an artifact: {title}", "Show in the chat"] {
                #expect(L10n.tr(word, preference: language) != word, "\(word) in \(language)")
            }
        }
    }
}
