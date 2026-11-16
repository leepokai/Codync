import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func theSelectionActionStartsTheChatOpeningAndItsFirstMessagesFinishItOnce() throws {
    let suite = "ChatOpening.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: nil, suite: suite)
    let key = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
    storage.computers = [Computer(id: RelayCrypto.computerId(signKey: key), name: "Mac", signKey: key.base64URL)]
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    }
    defer { accounts.retire() }
    let computer = try #require(accounts.computers.first)
    let store = try #require(accounts.store(for: computer.id))
    let ref = BotReference(accountId: nil, computerId: computer.id, botId: "b1")

    accounts.selection = ref
    #expect(store.chatOpening.finish("b1") != nil)
    #expect(store.chatOpening.finish("b1") == nil)

    accounts.selection = ref
    #expect(store.chatOpening.finish("b1") == nil)
    accounts.selection = nil
    accounts.selection = ref
    accounts.selection = nil
    #expect(store.chatOpening.finish("b1") == nil)
}
