import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func olderThreadsPageInAndStartOverWhenTheKeepChoiceChanges() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(#"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0,"threadKeepDays":0}"#)
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.listsOlderThreads })
    store.loadFirstOlderThreads()
    #expect(await until { store.olderThreads.rows.map(\.task.id) == ["t1"] })
    #expect(!store.olderThreads.more)
    // The host had no more: the list's end asks nothing.
    store.loadOlderThreads()
    try await Task.sleep(for: .milliseconds(100))
    #expect(await fake.calls.filter { $0 == "olderThreads" }.count == 1)

    await fake.emit(#"{"type":"threadKeepDays","days":30}"#)
    #expect(await until { store.hello?.threadKeepDays == 30 })
    #expect(store.olderThreads.rows.isEmpty)
    // Dropping an older thread takes it off the list at once.
    store.loadFirstOlderThreads()
    #expect(await until { !store.olderThreads.rows.isEmpty })
    store.dropWork("b1", task: "t1")
    #expect(store.olderThreads.rows.isEmpty)
}

@MainActor @Test func anOlderHostListsNoOlderThreads() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    store.loadFirstOlderThreads()
    try await Task.sleep(for: .milliseconds(100))
    #expect(!store.listsOlderThreads)
    #expect(await !fake.calls.contains("olderThreads"))
}
