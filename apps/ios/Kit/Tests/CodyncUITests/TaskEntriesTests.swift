import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A task's transcript stays in its own store: no chat ever holds it, and it goes when the task ends.
@MainActor @Test func taskEntriesStayOutOfEveryChat() async throws {
    let suite = "TaskEntriesTests.\(UUID().uuidString)"
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
    func taskEntry(rev: Int64, text: String) -> String {
        #"{"type":"taskEntry","rev":\#(rev),"task":"task-1","entry":{"id":"e1","seq":1,"botId":"task-1","rev":\#(rev),"kind":"agent","turn":1,"data":{"text":"\#(text)"},"createdAt":1,"updatedAt":1}}"#
    }
    for _ in 0..<200 {
        if await fake.subscribed { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":2,"tasks":[{"id":"task-1","title":"Fix login","status":"working","activity":"","startedAt":1}]}}"#)
    await fake.emit(taskEntry(rev: 4, text: "new"))
    await fake.emit(taskEntry(rev: 3, text: "late"))
    try await settle(4)
    #expect(store.bots["b1"]?.tasks.map(\.id) == ["task-1"])
    #expect(store.taskEntries["task-1"]?.map(\.data.text) == ["new"])
    #expect(store.entries.isEmpty)
    #expect(store.chat("b1").isEmpty && store.chat("task-1").isEmpty)

    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":5,"tasks":[]}}"#)
    try await settle(5)
    #expect(store.taskEntries.isEmpty)
}
