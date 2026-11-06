import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func internalMirrorRowsKeepPaginationAndRejectOlderVisibleReplies() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    var data = EntryData(text: "NO_UPDATE")
    data.final = true
    data.`internal` = true
    var hidden = Entry(id: "scheduled", seq: 1, botId: "bot", rev: 4, kind: "agent", turn: 1,
                       data: data, createdAt: 1, updatedAt: 4)
    store.upsert(hidden)
    #expect(store.chat("bot").isEmpty)
    #expect(store.canLoadOlder("bot"))

    var stale = hidden
    stale.rev = 3
    stale.data.`internal` = false
    store.upsert(stale)
    #expect(store.chat("bot").isEmpty)
    #expect(store.entries["bot"]?.first?.rev == 4)

    hidden.rev = 5
    hidden.data.`internal` = false
    hidden.data.text = "Your build passed."
    store.upsert(hidden)
    #expect(store.chat("bot").map(\.data.text) == ["Your build passed."])
}
