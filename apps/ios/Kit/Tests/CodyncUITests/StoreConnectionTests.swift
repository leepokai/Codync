import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func visibleConversationAcknowledgesEntriesBeforeUnreadAndFinalUpdates() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.bots["b1"] != nil })
    let token = UUID()
    store.setReading(token, botId: "b1", thread: nil, active: true)
    #expect(await until { await fake.readReceipts.count == 1 })
    // Streaming and final content have the same ID. The final update must be read
    // even though the roster has not yet reported any unread messages.
    await fake.emit(#"{"type":"entry","entry":{"id":"answer","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"draft","final":false},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.allEntries("b1").last?.rev == 2 })
    await fake.emit(#"{"type":"entry","entry":{"id":"answer","seq":1,"botId":"b1","rev":3,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":2}}"#)
    #expect(await until { await fake.readReceipts.count == 2 })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":4,"unread":1}}"#)
    #expect(await until { await fake.readReceipts.count == 3 })
    store.setReading(token, botId: "b1", thread: nil, active: false)
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":5,"unread":2}}"#)
    #expect(await until { store.bots["b1"]?.rev == 5 })
    try await Task.sleep(for: .milliseconds(50))
    #expect(await fake.readReceipts.count == 3)
    // Returning to the foreground acknowledges without needing navigation.
    store.setReading(token, botId: "b1", thread: nil, active: true)
    #expect(await until { await fake.readReceipts.count == 4 })
}

@MainActor @Test func readingOneThreadDoesNotAcknowledgeOtherThreads() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "mac", storage: storage) { fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let token = UUID()
    store.setReading(token, botId: "b1", thread: "root-a", active: true)
    #expect(await until { await fake.readReceipts.count == 1 })
    await fake.emit(#"{"type":"entry","entry":{"id":"other","seq":1,"botId":"b1","threadId":"root-b","rev":2,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.allEntries("b1").last?.id == "other" })
    try await Task.sleep(for: .milliseconds(50))
    #expect(await fake.readReceipts.count == 1)
    await fake.emit(#"{"type":"entry","entry":{"id":"own","seq":2,"botId":"b1","threadId":"root-a","rev":3,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { await fake.readReceipts.count == 2 })
    for receipt in await fake.readReceipts {
        let body = try #require(JSONSerialization.jsonObject(with: receipt) as? [String: Any])
        #expect(body["threadId"] as? String == "root-a")
        #expect(body["all"] as? Bool == false)
    }
}

@MainActor @Test func initialOfflineReportWaitsForReconnect() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    try await Task.sleep(for: .milliseconds(200))
    #expect(store.connection == .connecting)
    #expect(store.lastError == nil)
    await fake.set(.ready(.relay))
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    try await Task.sleep(for: .seconds(1))
    #expect(store.connection == .online)
}

@MainActor @Test func initialUnreachableEventuallyShowsOffline() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.failed("Can't reach"))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    try await Task.sleep(for: .milliseconds(200))
    #expect(store.connection == .connecting)
    #expect(await until { store.connection == .offline("Can't reach") })
    #expect(store.lastError == nil)
}

@MainActor @Test func failedAutomaticReadDoesNotInterruptChatAndRecoversWithTheLink() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.setReadFailure(true)
    store.setReading(UUID(), botId: "b1", thread: nil, active: true)
    #expect(await until { await fake.readAttempts == 1 })
    try await Task.sleep(for: .milliseconds(50))
    #expect(store.lastError == nil)
    await fake.set(.hostOffline(lastSeen: nil))
    #expect(await until { store.hostRoute == nil })
    #expect(store.connection == .online)
    #expect(store.canQueue)
    await fake.setReadFailure(false)
    await fake.set(.ready(.relay))
    #expect(await until { await fake.eventSubscriptionCount == 2 })
    await fake.emit(botEvent("b2", name: "Other bot", rev: 2))
    #expect(await until { await fake.readReceipts.count == 1 })
    #expect(store.connection == .online)
    #expect(store.lastError == nil)
}

@MainActor @Test func actionDuringReconnectWaitsInsteadOfFailing() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    // A drop still inside its grace: the header says connected, the link is gone.
    await fake.set(.connecting)
    #expect(await until { store.hostRoute == nil })
    store.stop("b1")
    #expect(await until { store.shownConnection == .connecting })
    #expect(await !fake.calls.contains("stop"))
    await fake.set(.ready(.relay))
    #expect(await until { await fake.eventSubscriptionCount == 2 })
    await fake.emit(botEvent("b1", name: "Bot", rev: 2))
    #expect(await until { await fake.calls.contains("stop") })
    #expect(store.shownConnection == .online)
    #expect(store.lastError == nil)
}

@MainActor @Test func droppedCallIsRetriedOnlyWhenSafeToRepeat() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.fail("stop", times: 1)
    store.stop("b1")
    #expect(await until { await fake.calls.filter { $0 == "stop" }.count == 2 })
    #expect(store.lastError == nil)
    // A new session can't be told apart from a second one, so it isn't sent twice.
    await fake.fail("newSession", times: 1)
    store.newSession("b1")
    #expect(await until { store.lastError != nil })
    #expect(await fake.calls.filter { $0 == "newSession" }.count == 1)
}

@MainActor @Test(.timeLimit(.minutes(1))) func offlineTapReconnectsBeforeGivingUp() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // The first link can't reach the computer; a fresh attempt would.
    let down = FakeRemote(.failed("Can't reach"))
    let up = FakeRemote(.ready(.relay))
    var made = 0
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) {
        made += 1
        return made == 1 ? down : up
    }
    defer { store.retire() }
    store.setActive(true)
    let offline = await waitForObservedCondition { store.connection == .offline("Can't reach") }
    try #require(offline)
    store.stop("b1")
    let subscriptions = await up.checkpoints()
    let subscribed = await waitForTestEvent(subscriptions, matching: { $0 == .eventsSubscribed })
    try #require(subscribed)
    await up.emit(botEvent("b1", name: "Bot", rev: 1))
    let calls = await up.checkpoints()
    let stopped = await waitForTestEvent(calls, matching: { $0 == .called("stop") })
    try #require(stopped)
    let downCalls = await down.calls
    #expect(made == 2)
    #expect(downCalls.isEmpty)
    #expect(store.lastError == nil)
}

@MainActor @Test(.timeLimit(.minutes(1))) func offlineActionWaitsForFreshTransportBeforeSending() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let down = FakeRemote(.failed("Can't reach"))
    let up = FakeRemote(.ready(.relay))
    let requested = AsyncStream<Bool>.makeStream()
    let release = AsyncStream<Void>.makeStream()
    defer { release.continuation.finish(); requested.continuation.finish() }
    var made = 0
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) {
        made += 1
        if made == 1 { return down }
        requested.continuation.yield(true)
        for await _ in release.stream { break }
        try Task.checkCancellation()
        return up
    }
    defer { store.retire() }
    store.setActive(true)
    let offline = await waitForObservedCondition { store.connection == .offline("Can't reach") }
    try #require(offline)
    store.stop("b1")
    let preparing = await waitForTestEvent(requested.stream, matching: { $0 })
    try #require(preparing)
    let waiting = await waitForObservedCondition { store.waiting == 1 }
    try #require(waiting)
    let downCalls = await down.calls
    let before = await up.calls
    #expect(store.shownConnection == .connecting)
    #expect(downCalls.isEmpty)
    #expect(before.isEmpty)
    #expect(store.lastError == nil)

    release.continuation.yield(())
    let subscriptions = await up.checkpoints()
    let subscribed = await waitForTestEvent(subscriptions, matching: { $0 == .eventsSubscribed })
    try #require(subscribed)
    await up.emit(botEvent("b1", name: "Bot", rev: 1))
    let calls = await up.checkpoints()
    let stopped = await waitForTestEvent(calls, matching: { $0 == .called("stop") })
    try #require(stopped)
    let after = await up.calls
    #expect(after.filter { $0 == "stop" }.count == 1)
    #expect(made == 2)
    #expect(store.lastError == nil)
}

@MainActor @Test func permissionAnswerShowsOnTheCardAndIsSentOnce() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let card = Entry(id: "p1", seq: 1, botId: "b1", threadId: nil, rev: 1, kind: "permission", turn: 1,
                     data: EntryData(status: "pending"), createdAt: 1, updatedAt: 1)
    store.respond(card, option: "allow")
    store.respond(card, option: "deny")
    #expect(store.answering["p1"] == "allow")
    #expect(await until { await fake.calls.contains("respondPermission") })
    try await Task.sleep(for: .milliseconds(100))
    #expect(await fake.calls.filter { $0 == "respondPermission" }.count == 1)
}

