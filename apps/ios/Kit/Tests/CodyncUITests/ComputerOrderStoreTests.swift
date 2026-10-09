import CodyncKit
import Foundation
import Testing
@testable import CodyncUI

@MainActor @Test func computerOrderSavesImmediatelyAndSurvivesRestart() {
    let suite = "order-\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "a", suite: suite)
    storage.computerOrder = ComputerOrder(ids: ["a", "hidden", "b"])
    let store = ComputerOrderStore(storage: storage)
    var changes = 0
    store.onChanged = { changes += 1 }
    store.move("b", to: "a", available: ["a", "b", "new"])
    #expect(storage.computerOrder?.ids == ["b", "a", "hidden", "new"])
    #expect(ComputerOrderStore(storage: storage).state == store.state)
    #expect(changes == 1)
}

@MainActor @Test func sameAccountOnDifferentDevicesKeepsIndependentOrders() {
    let phoneSuite = "phone-order-\(UUID().uuidString)"
    let desktopSuite = "desktop-order-\(UUID().uuidString)"
    defer {
        UserDefaults(suiteName: phoneSuite)?.removePersistentDomain(forName: phoneSuite)
        UserDefaults(suiteName: desktopSuite)?.removePersistentDomain(forName: desktopSuite)
    }
    let phoneStorage = SharedStore.Context(accountID: "same-account", suite: phoneSuite)
    let desktopStorage = SharedStore.Context(accountID: "same-account", suite: desktopSuite)
    phoneStorage.computerOrder = ComputerOrder(ids: ["a", "b", "c"])
    desktopStorage.computerOrder = ComputerOrder(ids: ["c", "b", "a"])
    let phone = ComputerOrderStore(storage: phoneStorage)
    let desktop = ComputerOrderStore(storage: desktopStorage)
    phone.move("b", to: "a", available: ["a", "b", "c"])
    #expect(desktop.state.ids == ["c", "b", "a"])
    desktop.move("a", to: "c", available: ["a", "b", "c"])
    #expect(phone.state.ids == ["b", "a", "c"])
    #expect(ComputerOrderStore(storage: phoneStorage).state.ids == ["b", "a", "c"])
    #expect(ComputerOrderStore(storage: desktopStorage).state.ids == ["a", "c", "b"])
}

@MainActor @Test func retiredComputerOrderCannotChangeLocalStorage() {
    let suite = "order-\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "a", suite: suite)
    storage.computerOrder = ComputerOrder(ids: ["a", "b"])
    let store = ComputerOrderStore(storage: storage)
    store.retire()
    store.move("b", to: "a", available: ["a", "b"])
    #expect(storage.computerOrder?.ids == ["a", "b"])
}

@Test func previouslyCachedOrderDecodesWithoutUploadState() throws {
    let cached = Data(#"{"ids":["b","a"],"pending":true}"#.utf8)
    let order = try JSONDecoder().decode(ComputerOrder.self, from: cached)
    #expect(order.ids == ["b", "a"])
    let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(order)) as? [String: Any]
    #expect(encoded?["pending"] == nil)
}
