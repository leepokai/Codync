import Foundation
import Testing
import UniformTypeIdentifiers
@testable import CodyncKit

private func bot(_ id: String, kind: String = "bot", members: [String] = [], hidden: Bool = false) throws -> Bot {
    let members = members.map { "\"\($0)\"" }.joined(separator: ",")
    return try JSONDecoder().decode(Bot.self, from: Data(
        #"{"id":"\#(id)","name":"\#(id)","kind":"\#(kind)","members":[\#(members)],"hidden":\#(hidden)}"#.utf8))
}

@Test func shareTargetsListRecentFirstThenEachComputerInOrder() throws {
    let laptop = Vectors.shared.computer
    var mini = laptop
    mini.id = "mini"
    mini.name = "Mini"
    let bots = [BotSnapshot(computerId: laptop.id, bot: try bot("a")),
                BotSnapshot(computerId: laptop.id, bot: try bot("b")),
                BotSnapshot(computerId: laptop.id, bot: try bot("hidden", hidden: true)),
                BotSnapshot(computerId: laptop.id, bot: try bot("empty", kind: "group")),
                BotSnapshot(computerId: laptop.id, bot: try bot("team", kind: "group", members: ["a", "b"])),
                BotSnapshot(computerId: mini.id, bot: try bot("c")),
                BotSnapshot(computerId: "removed", bot: try bot("d"))]
    let recent = [ShareRecipient(computerId: mini.id, botId: "c"), ShareRecipient(computerId: laptop.id, botId: "gone"),
                  ShareRecipient(computerId: laptop.id, botId: "b")]

    let targets = ShareTargets(computers: [laptop, mini], order: ComputerOrder(ids: [mini.id]), bots: bots, recent: recent)
    #expect(targets.recent.map(\.bot.id) == ["c", "b"], "recent order; a bot that's gone is skipped")
    #expect(targets.sections.map(\.title) == [laptop.name], "a computer left with no bots has no section")
    #expect(targets.sections.first?.rows.map(\.bot.id) == ["a", "team"], "no hidden bots, no empty groups, no repeats of Recent")
    #expect(targets.sections.first?.rows.last?.members.map(\.id) == ["a", "b"])
    #expect(targets.manyComputers)

    let single = ShareTargets(computers: [laptop], order: nil, bots: bots, recent: [])
    #expect(single.sections.map(\.title) == [nil], "one computer: no computer name")
    #expect(!single.manyComputers)
    #expect(ShareTargets(computers: [], order: nil, bots: bots, recent: recent).isEmpty)
}

@Test func shareRecipientsKeepTheLatestThreeWithoutRepeats() {
    let a = ShareRecipient(computerId: "m", botId: "a")
    let b = ShareRecipient(computerId: "m", botId: "b")
    let c = ShareRecipient(computerId: "m", botId: "c")
    let d = ShareRecipient(computerId: "n", botId: "a")
    #expect(a.remembered(in: []) == [a])
    #expect(b.remembered(in: [a]) == [b, a])
    #expect(a.remembered(in: [b, a]) == [a, b])
    #expect(d.remembered(in: [c, b, a]) == [d, c, b], "the same bot id on another computer is another chat")

    let suite = "ShareRecipients.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let alice = SharedStore.Context(accountID: "alice", suite: suite)
    alice.shareRecipients = [a]
    #expect(alice.shareRecipients == [a])
    #expect(SharedStore.Context(accountID: "bob", suite: suite).shareRecipients.isEmpty, "each account keeps its own")
    alice.erase()
    #expect(alice.shareRecipients.isEmpty)
}

@Test func sharedLinksAndTextBecomeTheMessageAndNamedTextStaysAFile() {
    #expect(IncomingShare.isLink([.url, .plainText]), "Safari's page")
    #expect(!IncomingShare.isLink([.fileURL]), "a file's URL is a file")
    #expect(!IncomingShare.isLink([.jpeg]))
    #expect(IncomingShare.isText([.utf8PlainText], suggestedName: nil))
    #expect(!IncomingShare.isText([.utf8PlainText], suggestedName: "Notes"), "a .txt from Files stays a file")
    #expect(!IncomingShare.isText([.png], suggestedName: nil))

    #expect(IncomingShare.lines(["https://example.com"], caption: " Look at this \n") == ["Look at this", "https://example.com"])
    #expect(IncomingShare.lines(["https://example.com"], caption: "https://example.com") == ["https://example.com"],
            "a caption that repeats the link isn't sent twice")
    #expect(IncomingShare.lines(["a"], caption: "  ") == ["a"])
    #expect(IncomingShare.lines(["a"], caption: nil) == ["a"])

    #expect(IncomingShare.fileName("Receipt", type: .pdf, fallback: "x.tmp") == "Receipt.pdf", "the type's extension added")
    #expect(IncomingShare.fileName("photo.HEIC", type: .jpeg, fallback: "x.tmp") == "photo.HEIC", "its own kept")
    #expect(IncomingShare.fileName(nil, type: .pdf, fallback: "IMG_1.pdf") == "IMG_1.pdf")
    #expect(IncomingShare.fileName("", type: .pdf, fallback: "IMG_1.pdf") == "IMG_1.pdf")

    let article = "Rust 2024 is out\nhttps://example.com/rust"
    #expect(IncomingShare.pasted(article, in: "Read this:\n" + article) == [article], "words added around it")
    #expect(IncomingShare.pasted(article, in: "Rust 2024 is out").isEmpty, "edited: words taken out never leave the phone")
    #expect(IncomingShare.pasted("", in: "typed").isEmpty)
}

@MainActor
@Test func sharesFromTheExtensionGoOutAsTheAppsOwn() throws {
    let shared = try #require(UserDefaults(suiteName: "ShareAnalytics-\(UUID())"))
    let analytics = Analytics(defaults: try #require(UserDefaults(suiteName: "AnalyticsTests-\(UUID())"))) { _ in false }
    Analytics.queueShare(.init(.sent, files: 2, text: true, recent: true), in: shared)
    Analytics.queueShare(.init(.failed, error: .offline, files: 1, text: false, recent: false), in: shared)
    analytics.takeQueuedShares(from: shared)
    #expect(analytics.pending.map(\.event) == ["share_sent", "share_sent"])
    let failed = try #require(analytics.pending.last?.properties)
    #expect(failed["outcome"] == .string("failed"))
    #expect(failed["error"] == .string("offline"))
    #expect(failed["files"] == .int(1))
    #expect(failed["text"] == .bool(false))
    #expect(failed["recent"] == .bool(false))
    #expect(analytics.pending.first?.properties["error"] == nil)
    analytics.takeQueuedShares(from: shared)
    #expect(analytics.pending.count == 2, "taken once")
}
