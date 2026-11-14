import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func aChatOnScreenIsReadAutomaticallyAndOpeningItIsTheUserReadingIt() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":2,"keptUnread":true}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.keptUnread == true })
    // Coming back into focus, or the link coming back, is not opening the chat: the host leaves it alone.
    store.setReading(UUID(), botId: "b1", active: true)
    #expect(await until { await fake.automaticReads == [true] })
    #expect(store.bots["b1"]?.keptUnread == true)
    store.openConversation("b1")
    #expect(store.bots["b1"]?.keptUnread == false)
    #expect(store.bots["b1"]?.unread == 0)
    #expect(await until { await fake.automaticReads == [true, false] })
}

@MainActor @Test func openingAChatReadsItWhateverIsKnownHere() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    // A mark set on another device may still be on its way here.
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    store.openConversation("b1")
    #expect(await until { await fake.automaticReads == [false] })
}

@MainActor @Test func openingAChatNotCachedYetStillReadsItOnTheHost() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b0", name: "Other", rev: 1))
    #expect(await until { store.connection == .online })
    #expect(store.bots["b1"] == nil)
    store.openConversation("b1")
    #expect(await until { await fake.automaticReads == [false] })
}

@MainActor @Test func aChatThatFailsToOpenStaysQuietAndIsReadAsOpenedOnceOnScreenWithALink() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":3,"keptUnread":true}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.keptUnread == true })
    await fake.setReadRefusal(true)
    store.openConversation("b1")
    #expect(await until { store.unsentOpens.contains("b1") })
    #expect(store.bots["b1"]?.unread == 3, "what was unread comes back")
    #expect(store.bots["b1"]?.keptUnread == true)
    #expect(store.lastError == nil, "opening a chat never opens a dialog")
    await fake.setReadRefusal(false)
    store.setReading(UUID(), botId: "b1", active: true)
    #expect(await until { await fake.automaticReads == [false] })
    #expect(store.unsentOpens.isEmpty)
}

@MainActor @Test func markingAChatUnreadDropsAnOpenThatNeverReachedTheHost() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.setReadRefusal(true)
    store.openConversation("b1")
    #expect(await until { store.unsentOpens.contains("b1") })
    store.markUnread("b1")
    #expect(store.unsentOpens.isEmpty)
}

@MainActor @Test func anOpenThatFailsWhileANewerMarkWaitsBehindItDoesNotGoAgain() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.setReadRefusal(true)
    store.openConversation("b1")
    store.markUnread("b1")
    #expect(await until { await fake.calls.contains("markUnread") })
    #expect(store.unsentOpens.isEmpty)
}

@MainActor @Test func markAsUnreadShowsTheAnswerUnlessANewerBotEventBeatIt() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    store.markUnread("b1")
    #expect(await until { store.bots["b1"]?.keptUnread == true })
    #expect(store.bots["b1"]?.unread == 1)
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":9,"unread":0}}"#)
    #expect(await until { store.bots["b1"]?.rev == 9 })
    store.markUnread("b1")
    #expect(await until { await fake.calls.filter { $0 == "markUnread" }.count == 2 })
    try await Task.sleep(for: .milliseconds(50))
    #expect(store.bots["b1"]?.rev == 9, "the answer's rev 2 is older")
    #expect(store.bots["b1"]?.keptUnread == false)
}

@MainActor @Test func refusedMarkAsReadGivesTheHoldAndCountBack() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":3,"keptUnread":true}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.keptUnread == true })
    await fake.setReadRefusal(true)
    store.markAllRead("b1")
    #expect(store.bots["b1"]?.keptUnread == false)
    #expect(store.bots["b1"]?.unread == 0)
    #expect(await until { store.bots["b1"]?.keptUnread == true })
    #expect(store.bots["b1"]?.unread == 3)
    #expect(store.lastError != nil, "the roster's Mark as read shows a refusal")
}

@MainActor @Test func markAsUnreadWaitsForAMarkAsReadStillRetrying() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":2}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.unread == 2 })
    await fake.fail("markRead", times: 2)
    store.markAllRead("b1")
    store.markUnread("b1")
    #expect(await until { await fake.calls.contains("markUnread") })
    let calls = await fake.calls.filter { $0 == "markRead" || $0 == "markUnread" }
    #expect(calls == ["markRead", "markRead", "markRead", "markUnread"])
}

@MainActor @Test func openingAChatRightAfterMarkingItUnreadWaitsSoTheReadLandsLast() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.fail("markUnread", times: 2)
    store.markUnread("b1")
    store.openConversation("b1")
    #expect(await until { await fake.calls.contains("markRead") })
    let calls = await fake.calls.filter { $0 == "markRead" || $0 == "markUnread" }
    #expect(calls == ["markUnread", "markUnread", "markUnread", "markRead"])
}

@MainActor @Test func aMessageSentWhileMarkAsUnreadIsRetryingWaitsSoTheSendReadsTheChatLast() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.fail("markUnread", times: 2)
    store.markUnread("b1")
    store.send("hi", to: "b1")
    #expect(await until { await fake.calls.contains("send") })
    let calls = await fake.calls.filter { $0 == "send" || $0 == "markUnread" }
    #expect(calls == ["markUnread", "markUnread", "markUnread", "send"])
}

@MainActor @Test func twoReadsRefusedInARowBothGiveBackWhatWasThereBeforeTheFirst() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":3,"keptUnread":true}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.keptUnread == true })
    await fake.setReadRefusal(true)
    store.openConversation("b1")
    store.openConversation("b1")
    #expect(await until { await fake.readAttempts == 2 })
    #expect(await until { store.sendingReadMarks["b1"] == nil })
    #expect(store.bots["b1"]?.unread == 3)
    #expect(store.bots["b1"]?.keptUnread == true)
    #expect(store.beforeReads.isEmpty)
}

@MainActor @Test func openingTheChatAlreadyOnScreenFromOutsideTheRosterIsTheUserOpeningIt() async throws {
    let suite = "ReadMarks.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: nil, suite: suite)
    let key = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
    storage.computers = [Computer(id: RelayCrypto.computerId(signKey: key), name: "Mac", signKey: key.base64URL)]
    let fake = FakeRemote(.ready(.direct))
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in fake }
    }
    defer { accounts.retire() }
    let computer = try #require(accounts.computers.first)
    let store = try #require(accounts.store(for: computer.id))
    let chat = BotReference(accountId: nil, computerId: computer.id, botId: "b1")
    accounts.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"unread":1,"keptUnread":true}}"#)
    #expect(await until { store.connection == .online && store.bots["b1"]?.keptUnread == true })

    accounts.open(chat)
    #expect(accounts.selection == chat)
    try await Task.sleep(for: .milliseconds(50))
    #expect(await fake.readReceipts.isEmpty, "a new selection is read by the view it brings up")

    accounts.open(chat)
    #expect(await until { await fake.automaticReads == [false] })
    #expect(store.bots["b1"]?.keptUnread == false)
}
