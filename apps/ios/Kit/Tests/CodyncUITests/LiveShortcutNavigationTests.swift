import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test(arguments: [false, true])
func leavingLiveShortcutBeforeBotArrivesDoesNotStartCallOnReturn(openAnotherBot: Bool) async throws {
    let suite = "LiveShortcutNavigation.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: nil, suite: suite)
    let key = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
    storage.computers = [Computer(id: RelayCrypto.computerId(signKey: key), name: "Mac", signKey: key.base64URL)]
    let remote = FakeRemote(.ready(.direct))
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in remote }
    }
    defer { accounts.retire() }
    let computer = try #require(accounts.computers.first)
    let store = try #require(accounts.store(for: computer.id))
    let original = BotReference(accountId: nil, computerId: computer.id, botId: "b1")
    let other = BotReference(accountId: nil, computerId: computer.id, botId: "b2")
    accounts.selection = original
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2}"#)
    let opening = Task { try await store.requestVoiceCall(to: "b1") }
    #expect(await until { store.preparingVoiceCall != nil })
    accounts.selection = openAnotherBot ? other : nil
    // Returning before the connection completes must not revalidate the abandoned request.
    accounts.selection = original
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    await #expect(throws: CancellationError.self) { try await opening.value }
    #expect(!store.takeVoiceCallRequest(for: "b1"))

    // A fresh shortcut for the same bot still starts normally.
    try await store.requestVoiceCall(to: "b1")
    #expect(store.takeVoiceCallRequest(for: "b1"))
    #expect(!store.takeVoiceCallRequest(for: "b1"))

    // A ready request awaiting the view is invalidated by the same navigation path.
    try await store.requestVoiceCall(to: "b1")
    accounts.selection = openAnotherBot ? other : nil
    accounts.selection = original
    #expect(!store.takeVoiceCallRequest(for: "b1"))
}
