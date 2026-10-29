import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func syncedEntriesOfferServerHistoryUntilItIsComplete() {
    let suite = "HistoryPagingTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = Computer(id: "history", name: "Test", signKey: "")
    let store = BotStore(computer: computer, storage: storage) { _ in throw HostError.unreachable }
    defer { store.retire() }
    store.entries["b"] = (0..<200).map { index in
        Entry(id: "e\(index)", seq: Int64(index + 100), botId: "b", rev: 1, kind: "notice", turn: 0, data: EntryData(),
              createdAt: 1, updatedAt: 1)
    }
    #expect(store.canLoadOlder("b"))
    store.historyComplete.insert("b")
    #expect(!store.canLoadOlder("b"))
}

@MainActor @Test func emptyChatsAndUnsentMessagesDoNotOfferServerHistory() {
    let suite = "HistoryPagingTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = Computer(id: "history", name: "Test", signKey: "")
    let store = BotStore(computer: computer, storage: storage) { _ in throw HostError.unreachable }
    defer { store.retire() }
    #expect(!store.canLoadOlder("b"))
    store.entries["b"] = [Entry(id: "local-draft", seq: Int64.max, botId: "b", rev: 1, kind: "user", turn: 0,
                                data: EntryData(), createdAt: 1, updatedAt: 1)]
    #expect(!store.canLoadOlder("b"))
}
