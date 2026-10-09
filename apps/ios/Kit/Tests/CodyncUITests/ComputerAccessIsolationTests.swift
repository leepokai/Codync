import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

private func isolationComputer(_ name: String, keyByte: UInt8) -> Computer {
    let key = Data(repeating: keyByte, count: 32)
    let encoded = key.base64EncodedString()
        .replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_")
        .replacingOccurrences(of: "=", with: "")
    return Computer(id: RelayCrypto.computerId(signKey: key), name: name, signKey: encoded)
}

@MainActor @Test(arguments: [false, true])
func phoneRosterUsesItsOwnPairingsNotUnavailableOrderEntries(hasIndependentAccessToB: Bool) throws {
    let suite = "ComputerAccessIsolation.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "same-account", suite: suite)
    let a = isolationComputer("A", keyByte: 1)
    let b = isolationComputer("B via desktop SSH", keyByte: 2)
    storage.computers = hasIndependentAccessToB ? [a, b] : [a]
    // A previously saved position must not grant access to an unavailable computer.
    storage.computerOrder = ComputerOrder(ids: [b.id, a.id])
    var opened: [ComputerID] = []
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"same-bot-id","name":"Local bot","rev":1}"#.utf8))
    let accounts = AccountStore(storage: storage, clientKind: "ios", cloud: nil) { computer in
        opened.append(computer.id)
        let store = BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.connecting) }
        store.bots[bot.id] = bot
        return store
    }
    defer { accounts.retire() }
    accounts.setActive(false)

    let expected = hasIndependentAccessToB ? [b.id, a.id] : [a.id]
    #expect(accounts.computers.map(\.id) == expected)
    #expect(Set(opened) == Set(expected))
    #expect(Set(accounts.roster.map(\.ref.computerId)) == Set(expected))
    #expect(accounts.roster.count == expected.count)
    #expect((accounts.store(for: b.id) != nil) == hasIndependentAccessToB)
    #expect(Set(storage.computers.map(\.id)) == Set(expected))
}

@MainActor @Test func forgottenComputerDoesNotReturnFromAccountOrder() {
    let suite = "ComputerAccessIsolation.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "same-account", suite: suite)
    let a = isolationComputer("A", keyByte: 1)
    let b = isolationComputer("B", keyByte: 2)
    storage.computers = [a, b]
    storage.computerOrder = ComputerOrder(ids: [b.id, a.id])
    let accounts = AccountStore(storage: storage, clientKind: "ios", cloud: nil) { computer in
        BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.connecting) }
    }
    defer { accounts.retire() }
    accounts.setActive(false)
    accounts.forget(b.id)
    #expect(accounts.computers.map(\.id) == [a.id])
    #expect(accounts.store(for: b.id) == nil)
    #expect(storage.computers.map(\.id) == [a.id])
    #expect(storage.computerOrder?.ids.contains(b.id) == true)
}
