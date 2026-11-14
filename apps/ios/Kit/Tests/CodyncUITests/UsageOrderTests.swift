import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@Suite struct UsageOrderTests {
    @Test func aDroppedRowLandsWhereItWasLetGo() {
        let ids = ["claude", "codex", "cursor"]
        #expect(UsageOrder.moved(ids, from: [0], to: 2) == ["codex", "claude", "cursor"])
        #expect(UsageOrder.moved(ids, from: [2], to: 0) == ["cursor", "claude", "codex"])
    }

    @Test func aRowDroppedInItsOwnPlaceSendsNothing() {
        #expect(UsageOrder.moved(["claude", "codex"], from: [0], to: 0) == nil)
        #expect(UsageOrder.moved(["claude", "codex"], from: [0], to: 1) == nil)
    }
}

/// How many of the moves started in a test got back to their caller.
@MainActor private final class Returned {
    var count = 0
}

@MainActor private func onlineStore(_ fake: FakeRemote, _ storage: SharedStore.Context) async -> BotStore {
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: ""), storage: storage) { _ in fake }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    return store
}

@MainActor @Test func movesGoOutOneAtATimeAndOnlyTheNewest() async {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = await onlineStore(fake, storage)
    defer { store.retire() }
    await fake.holdUsageOrders(true)
    let returned = Returned()
    let move = { (ids: [String]) in
        Task {
            await store.setUsageOrder(ids)
            returned.count += 1
        }
    }

    let first = move(["a"])
    #expect(await until { await fake.usageOrders == [["a"]] })
    let second = move(["b"])
    #expect(await until { store.usageOrderWaiting == ["b"] })
    let third = move(["c"])
    #expect(await until { store.usageOrderWaiting == ["c"] })

    await fake.answerUsageOrder()
    #expect(await until { await fake.usageOrders == [["a"], ["c"]] }, "the one in between never goes out")
    #expect(returned.count == 0, "every caller waits for the last answer")

    await fake.holdUsageOrders(false)
    for task in [first, second, third] { await task.value }
    #expect(await fake.usageOrders == [["a"], ["c"]])

    await store.setUsageOrder(["d"])
    #expect(await fake.usageOrders == [["a"], ["c"], ["d"]], "a later move sends again")
}

@MainActor @Test func aFailedMoveShowsItsErrorAndDropsTheOneWaiting() async {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = await onlineStore(fake, storage)
    defer { store.retire() }
    await fake.holdUsageOrders(true)
    await fake.fail("setUsageOrder", times: 1)

    let first = Task { await store.setUsageOrder(["a"]) }
    #expect(await until { await fake.usageOrders == [["a"]] })
    let second = Task { await store.setUsageOrder(["b"]) }
    #expect(await until { store.usageOrderWaiting == ["b"] })

    await fake.holdUsageOrders(false)
    await first.value
    await second.value
    #expect(store.lastError != nil)
    #expect(await fake.usageOrders == [["a"]], "it was moved from an order that didn't land")

    await store.setUsageOrder(["c"])
    #expect(await fake.usageOrders == [["a"], ["c"]], "the next move still sends")
}
