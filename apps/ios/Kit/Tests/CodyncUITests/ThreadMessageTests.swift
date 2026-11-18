import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A store on a computer whose bot `b1` has the item `i1`, worked by `task-1`.
@MainActor private func threadStore() async -> (BotStore, FakeRemote, String) {
    let (storage, suite) = context()
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"work":[{"id":"i1","title":"Fix login","status":"working","task":"task-1","talk":true}]}}"#)
    #expect(await until { store.bots["b1"]?.work.first?.talk == true })
    return (store, fake, suite)
}

private let thread = WorkThread(botId: "b1", itemId: "i1", task: "task-1")

/// A message written to an item's thread names the bot and its item, shows in the task's transcript at once, and
/// never enters a chat.
@MainActor @Test func aMessageToAThreadGoesToItsItemAndShowsInItsTranscript() async throws {
    let (store, fake, suite) = await threadStore()
    defer { store.retire(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    store.send("  Also the docs ", to: thread, nonce: "n1")
    #expect(store.taskEntries["task-1"]?.map(\.id) == ["local-n1"])
    #expect(store.taskEntries["task-1"]?.first?.data.status == "sending")
    // The computer's copy takes its place.
    #expect(await until { store.taskEntries["task-1"]?.map(\.id) == ["e1"] })
    #expect(await fake.bodies["send"] == ["botId": "b1", "itemId": "i1", "text": "Also the docs", "clientNonce": "n1"])
    #expect(store.entries.isEmpty)
}

/// Send now and Remove on a message waiting in a thread name the item too; Remove takes it out at once.
@MainActor @Test func aWaitingMessageInAThreadIsSentNowOrTakenBackByItsItem() async throws {
    let (store, fake, suite) = await threadStore()
    defer { store.retire(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    await fake.emit(#"{"type":"taskEntry","rev":2,"task":"task-1","entry":{"id":"q1","seq":1,"botId":"task-1","rev":2,"kind":"user","turn":1,"data":{"text":"Also the docs","status":"queued"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.taskEntries["task-1"]?.count == 1 })
    let queued = try #require(store.taskEntries["task-1"]?.first)

    store.steer(queued, in: thread)
    #expect(await until { await fake.bodies["steer"] == ["botId": "b1", "itemId": "i1", "entryId": "q1"] })
    #expect(await until { store.answering["q1"] == nil })

    store.unqueue(queued, in: thread)
    #expect(store.taskEntries["task-1"]?.threadEntries.rest.isEmpty == true)
    #expect(await until { await fake.bodies["unqueue"] == ["botId": "b1", "itemId": "i1", "entryId": "q1"] })
    // A refusal puts it back.
    await fake.refuse("unqueue", "It was sent already")
    var other = queued
    other.id = "q2"
    store.unqueue(other, in: thread)
    #expect(await until { store.lastError == "It was sent already" })
    #expect(store.taskEntries["task-1"]?.threadEntries.rest.map(\.id) == ["q2"])
}

/// A thread the computer won't take a message for says why, and the message stays to send again or delete.
@MainActor @Test func aRefusedThreadMessageSaysWhyAndCanGoAgain() async throws {
    let (store, fake, suite) = await threadStore()
    defer { store.retire(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let refusal = #""Fix login" ended too long ago to write to: add new work instead"#
    await fake.refuse("send", refusal)

    store.send("Once more", to: thread, nonce: "n2")
    #expect(await until { store.taskEntries["task-1"]?.first?.data.status == "failed" })
    #expect(store.lastError == refusal)
    let failed = try #require(store.taskEntries["task-1"]?.first)

    // Resend goes with the same nonce.
    await fake.refuse("send", nil)
    store.retry(failed, in: thread)
    #expect(await until { store.taskEntries["task-1"]?.map(\.id) == ["e1"] })
    #expect(await fake.bodies["send"]?["clientNonce"] == "n2")

    await fake.refuse("send", refusal)
    store.send("And again", to: thread, nonce: "n3")
    #expect(await until { store.taskEntries["task-1"]?.last?.data.status == "failed" })
    let again = try #require(store.taskEntries["task-1"]?.last)
    store.discard(again, in: thread)
    #expect(store.taskEntries["task-1"]?.map(\.id) == ["e1"])
    #expect(store.entries.isEmpty)
}

/// The view of all bots' work asks a bot's conversation to open its details on an item's thread; only that
/// conversation takes it, once.
@MainActor @Test func aThreadRequestIsTakenOnceByItsOwnConversation() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in FakeRemote(.ready(.direct)) }
    defer { store.retire() }
    store.requestThread(of: "b1", item: WorkItem(id: "i1", title: "Fix login", status: .working, task: "task-1"))
    #expect(store.takeThreadRequest(for: "b2") == nil)
    #expect(store.takeThreadRequest(for: "b1")?.id == "i1")
    #expect(store.takeThreadRequest(for: "b1") == nil)
    #expect(DetailsRequest(thread: WorkItem(id: "i1", title: "Fix login", status: .working)).thread?.id == "i1")
}
