import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

private func artifact(_ id: String = "p1", title: String = "Weekly report", updatedAt: Int64, entryId: String? = "e9") -> Artifact {
    Artifact(id: id, botId: "b1", title: title, size: 1, sha256: "", createdAt: 1, updatedAt: updatedAt, entryId: entryId)
}

@Test func aArtifactIsReadAgainOnlyWhenANewerVersionIsKnown() {
    var cache = ArtifactCache()
    #expect(cache.needsRead("p1"), "nothing held yet")
    cache.reading.insert("p1")
    #expect(!cache.needsRead("p1"), "one read at a time")
    cache.reading.remove("p1")

    cache.store(artifact(updatedAt: 5), html: "<h1>v5</h1>")
    #expect(!cache.needsRead("p1"))
    // The card a newer publish posted names its version before any artifact event does.
    cache.announce(ArtifactRef(id: "p1", title: "Weekly report", updatedAt: 9))
    #expect(cache.needsRead("p1"))
    cache.store(artifact(updatedAt: 9), html: "<h1>v9</h1>")
    #expect(!cache.needsRead("p1") && cache.html["p1"]?.text == "<h1>v9</h1>")

    // A late answer about an older version changes nothing; a rename keeps the version and still lands.
    cache.upsert(artifact(title: "Old", updatedAt: 5))
    #expect(cache.details["p1"]?.title == "Weekly report")
    cache.upsert(artifact(title: "Sales", updatedAt: 9))
    #expect(cache.details["p1"]?.title == "Sales" && !cache.needsRead("p1"))
    #expect(cache.title(ArtifactRef(id: "p1", title: "Weekly report", updatedAt: 9)) == "Sales", "an older card shows the newest title")
    #expect(cache.title(ArtifactRef(id: "p1", title: "Q4", updatedAt: 12)) == "Q4", "until the details catch up with a newer card")
}

@Test func theLibraryGroupsByWhenEachThingCame() throws {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
    calendar.firstWeekday = 2
    // Thursday 2026-10-22, noon.
    let now = Date(timeIntervalSince1970: 1_792_670_400)
    let hour: Int64 = 3_600_000
    let at = Int64(now.timeIntervalSince1970 * 1000)
    let photo = ChatLibraryItem(.attachment(Attachment(id: "photo", name: "a.png", size: 1), entryId: "e1"), at: at - 24 * hour)
    let items = [ChatLibraryItem(artifact("old", updatedAt: at - 30 * 24 * hour)), ChatLibraryItem(artifact("today", updatedAt: at - hour)),
                 ChatLibraryItem(artifact("monday", updatedAt: at - 3 * 24 * hour)), photo, ChatLibraryItem(artifact("noon", updatedAt: at))]
    let sections = ChatLibraryItem.sections(items, now: now, calendar: calendar)
    #expect(sections.map(\.period) == [.today, .yesterday, .thisWeek, .earlier])
    #expect(sections.map { $0.items.map(\.id) } == [["artifact/noon", "artifact/today"], ["attachment/e1/photo"], ["artifact/monday"], ["artifact/old"]])
}

@MainActor @Test func aMessageAboutAArtifactRepliesToItsCardFromItsOwnDraft() async throws {
    let suite = "ArtifactCacheTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: Computer(id: "artifacts", name: "Mac", signKey: ""), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
    await fake.emit(#"{"type":"artifact","artifact":{"id":"p1","botId":"b1","title":"Weekly report","size":1,"sha256":"","createdAt":1,"updatedAt":5,"entryId":"e9"}}"#)
    #expect(await until { store.artifacts.details["p1"] != nil })
    let report = try #require(store.artifacts.details["p1"])

    store.setComposerDraft("Half-written", for: "b1")
    let key = BotStore.artifactDraftKey(report)
    store.setComposerDraft("Add totals", for: key)
    #expect(store.sendComposerDraft(to: "b1", draft: key, replyTo: report.replyQuote))
    #expect(store.chat("b1").last?.data.replyTo?.artifact == "p1")
    #expect(store.composerDraft(for: key).isEmpty)
    #expect(store.composerDraft(for: "b1") == "Half-written", "the chat's own draft stays")
    #expect(await until { await fake.replyTos == ["e9"] })

    store.setComposerDraft("Later", for: key)
    store.removeComposerDrafts(for: "b1")
    #expect(store.composerDraft(for: key).isEmpty, "a deleted bot's artifact drafts go with it")
}
