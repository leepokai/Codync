import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func aPromptCountsOnceItArrivedAndAVoiceLineNever() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    var prompts = 0
    store.onPromptSent = { prompts += 1 }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })

    store.send("said aloud", to: "b1", call: "c1")
    #expect(await until { store.chat("b1").contains { $0.id == "e1" } })
    #expect(prompts == 0)

    store.send("typed", to: "b1")
    #expect(await until { store.chat("b1").contains { $0.id == "e2" } })
    #expect(prompts == 1)

    await fake.fail("send", times: 1_000)
    let failing = try #require(store.send("never arrives", to: "b1", replyTo: nil, nonce: UUID().uuidString))
    #expect(await until { await fake.calls.filter { $0 == "send" }.count > 2 })
    failing.cancel()
    #expect(await failing.value == .failed)
    #expect(prompts == 1)
}

@MainActor @Test func aPromptCountsOnceTheMailboxHoldsIt() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    var prompts = 0
    store.onPromptSent = { prompts += 1 }
    store.setActive(true)
    #expect(await until { store.canQueue })

    store.send("waits", to: "b1")
    #expect(await until { prompts == 1 })

    await fake.failEnqueue(HostError.unreachable)
    store.send("refused", to: "b2")
    #expect(await until { store.chat("b2").first?.data.status == "failed" })
    #expect(prompts == 1)
}
