import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func savedBotKeepsItsCustomIcon() async throws {
    let suite = "BotIconTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    let computer = Computer(id: UUID().uuidString, name: "Mac", signKey: "unused", boxKey: "unused")
    let store = BotStore(computer: computer, storage: SharedStore.Context(accountID: nil, suite: suite)) { _ in remote }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    #expect(await until { store.bots["b1"] != nil })
    let icon = CustomIcon(dataUrl: "data:image/png;base64,AA==", tinted: false)
    store.applyIcons(["bot:b1": icon])
    #expect(store.bots["b1"]?.customIcon == icon)

    let saved = try await store.save(BotDraft(try #require(store.bots["b1"])))
    #expect(saved.customIcon == icon)
    #expect(store.bots["b1"]?.customIcon == icon)
}
