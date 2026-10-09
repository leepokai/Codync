import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func fullRefreshDeletionClearsCachedConversationSelectionAndDrafts() async throws {
    let suite = "BotDeletionSyncTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = Computer(id: UUID().uuidString, name: "Mac", signKey: "test")
    let fake = FakeRemote(.ready(.direct))
    await fake.setHello(#"{"hostId":"h1","name":"Mac","version":"\#(VersionMismatch.minHost)","os":"macos","backends":[],"rev":10}"#)
    let store = BotStore(computer: computer, clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"gone","name":"Deleted while offline","rev":9}"#.utf8))
    let entry = try JSONDecoder().decode(Entry.self, from: Data(#"{"id":"e1","botId":"gone","seq":1,"rev":8,"kind":"user","turn":1,"data":{"text":"cached"},"createdAt":1,"updatedAt":1}"#.utf8))
    store.bots[bot.id] = bot
    store.upsert(entry)
    store.selection = bot.id
    store.setComposerDraft("draft", for: bot.id)
    store.setComposerDraft("thread draft", for: bot.id, thread: "root")
    store.rev = 9
    store.hostVersion = HostVersion(version: "2.11.1", minApp: nil)
    let task = Task { await store.runEvents(HostClient(transport: fake)) }
    defer { task.cancel() }
    for _ in 0..<200 {
        if await fake.subscribed { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(await fake.subscribed)
    #expect(store.rev == 0)
    #expect(store.bots[bot.id] != nil) // Full refresh keeps the cache until updates arrive.
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":10}"#)
    await fake.emit(#"{"type":"bot","bot":{"id":"gone","deleted":true,"rev":10}}"#)
    for _ in 0..<200 {
        if store.rev == 10 { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(store.rev == 10)
    #expect(store.bots[bot.id] == nil)
    #expect(store.chat(bot.id).isEmpty)
    #expect(store.selection == nil)
    #expect(store.composerDraft(for: bot.id).isEmpty)
    #expect(store.composerDraft(for: bot.id, thread: "root").isEmpty)
    store.saveCache()
    let restored = BotStore(computer: computer, clientKind: "ios", storage: storage) { fake }
    defer { restored.retire() }
    #expect(restored.bots[bot.id] == nil)
    #expect(restored.chat(bot.id).isEmpty)
}
