import Foundation
import Testing
@testable import CodyncKit

@Test func computerOrderKeepsUnavailableComputersAndAppendsNewOnes() {
    let order = ComputerOrder(ids: ["offline", "b", "a"])
    #expect(order.orderedIDs(["a", "new", "b"]) == ["b", "a", "new"])
    #expect(order.moving("a", to: "b", available: ["a", "b", "new"]) == ["offline", "a", "b", "new"])
    #expect(order.moving("missing", to: "a", available: ["a", "b"]) == nil)
    #expect(order.orderedIDs(["a", "a", "b"]) == ["b", "a"])
}

@Test func computerOrderPersistenceIsIsolatedByAccount() {
    let suite = "computer-order-\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let a = SharedStore.Context(accountID: "a", suite: suite)
    let b = SharedStore.Context(accountID: "b", suite: suite)
    a.computerOrder = ComputerOrder(ids: ["mini", "laptop"])
    #expect(SharedStore.Context(accountID: "a", suite: suite).computerOrder?.ids == ["mini", "laptop"])
    #expect(b.computerOrder == nil)
    #expect(SharedStore.Context(accountID: nil, suite: suite).computerOrder == nil)
}
