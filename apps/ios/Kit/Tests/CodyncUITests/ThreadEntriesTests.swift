import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A thread's transcript stays in its own store: no chat ever holds it, and it goes when no task names the thread.
@MainActor @Test func threadEntriesStayOutOfEveryChat() async throws {
    let suite = "ThreadEntriesTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "test"), storage: storage) { _ in fake }
    defer { store.retire() }
    let events = Task { await store.runEvents(HostClient(transport: fake)) }
    defer { events.cancel() }
    func settle(_ rev: Int64) async throws {
        for _ in 0..<200 where store.rev != rev { try await Task.sleep(for: .milliseconds(10)) }
        #expect(store.rev == rev)
    }
    func threadEntry(rev: Int64, text: String) -> String {
        #"{"type":"threadEntry","rev":\#(rev),"thread":"thread-1","entry":{"id":"e1","seq":1,"botId":"thread-1","rev":\#(rev),"kind":"agent","turn":1,"data":{"text":"\#(text)"},"createdAt":1,"updatedAt":1}}"#
    }
    for _ in 0..<200 {
        if await fake.subscribed { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":2,"work":[{"id":"i1","title":"Fix login","status":"working","activity":"","thread":"thread-1","startedAt":1}]}}"#)
    await fake.emit(threadEntry(rev: 4, text: "new"))
    await fake.emit(threadEntry(rev: 3, text: "late"))
    try await settle(4)
    #expect(store.bots["b1"]?.work.map(\.thread) == ["thread-1"])
    #expect(store.threadEntries["thread-1"]?.map(\.data.text) == ["new"])
    #expect(store.entries.isEmpty)
    #expect(store.chat("b1").isEmpty && store.chat("thread-1").isEmpty)

    // An ended task still names its thread, for its transcript.
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":5,"work":[{"id":"i1","title":"Fix login","status":"done","thread":"thread-1","startedAt":1,"endedAt":9}]}}"#)
    try await settle(5)
    #expect(store.threadEntries["thread-1"]?.count == 1)

    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":6,"work":[]}}"#)
    try await settle(6)
    #expect(store.threadEntries.isEmpty)
}
