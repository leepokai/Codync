import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func reorderingKeepsPinnedBotsOnTop() throws {
    let suite = "RosterOrderTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "test"), storage: storage) {
        FakeRemote(.connecting)
    }
    defer { store.retire() }
    for (id, pinned, lastAt) in [("p", true, 0), ("a", false, 3), ("b", false, 2), ("c", false, 1)] {
        let json = #"{"id":"\#(id)","pinned":\#(pinned),"lastAt":\#(lastAt)}"#
        store.bots[id] = try JSONDecoder().decode(Bot.self, from: Data(json.utf8))
    }
    let order = { store.roster.map(\.id) }
    #expect(order() == ["p", "a", "b", "c"]) // Never arranged: recent activity.
    store.reorder(["p", "c", "a", "b"])
    #expect(order() == ["p", "c", "a", "b"])
    store.move("a", by: -1)
    #expect(order() == ["p", "a", "c", "b"])
    store.move("a", by: -1) // An unpinned bot stays below the pinned ones.
    #expect(order() == ["p", "a", "c", "b"])
    store.reorder(["c", "p", "a", "b"]) // Dropped above a pinned bot: still listed below it.
    #expect(order() == ["p", "c", "a", "b"])
}
