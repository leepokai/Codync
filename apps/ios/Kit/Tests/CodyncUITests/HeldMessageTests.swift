import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A computer in no account has no relay: the phone holds the message, and sends it when the computer is back.
/// Each try at reaching it makes a new link; the computer's back on the latest (`back`).
@MainActor private final class NoRelay {
    let (storage, suite) = context()
    private(set) var links: [FakeRemote] = []
    private var state = LinkState.failed("Can't reach your computer.")
    private(set) lazy var store: BotStore = {
        var computer = randomComputer("Mac")
        computer.cloud = nil
        return BotStore(computer: computer, storage: storage) { [unowned self] _ in
            let link = FakeRemote(state)
            links.append(link)
            return link
        }
    }()

    func start() async {
        store.setActive(true)
        #expect(await until { store.isOffline })
    }

    func back() async {
        state = .ready(.direct)
        guard let link = links.last else { return }
        await link.set(state)
        #expect(await until { await link.subscribed })
        await link.emit(botEvent("b1", name: "Bot", rev: 1))
    }

    func sent() async -> [String] {
        var all: [String] = []
        for link in links { all += await link.sent }
        return all
    }

    func end() {
        store.retire()
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
    }
}

@MainActor @Test func withNoRelayAMessageWaitsOnThePhoneAndGoesWithTheSameNonce() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    #expect(store.holdsOnPhone && !store.canQueue)

    store.send("hi", to: "b1")
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    let nonce = try #require(store.chat("b1").first?.data.clientNonce)
    let enqueued = await phone.links.last?.enqueued
    #expect(enqueued?.isEmpty == true, "there's no relay to hold it")

    await phone.back()
    #expect(await until { await phone.sent() == ["hi"] })
    #expect(store.chat("b1").first?.data.clientNonce == nonce)
    #expect(!store.holdsOnPhone)
}

@MainActor @Test func withNoRelayCancellingAWaitingMessageStaysOnThePhone() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.send("hi", to: "b1")
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    store.cancelQueued(try #require(store.chat("b1").first))
    #expect(store.chat("b1").isEmpty)
    await phone.back()
    try await Task.sleep(for: .milliseconds(100))
    #expect(await phone.sent().isEmpty, "a message taken back never goes")
}

@MainActor @Test func aHeldMessageWhoseFilesWentWithTheAppWaitsForTheUser() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    var data = EntryData(text: "see this", status: "held", clientNonce: "n1")
    data.attachments = [Attachment(id: UUID().uuidString, name: "a.txt", size: 1)]
    store.upsert(Entry(id: "local-n1", seq: .max, botId: "b1", rev: 0, kind: "user", turn: 0, data: data, createdAt: 1, updatedAt: 0))
    await phone.back()
    #expect(await until { store.chat("b1").first?.data.status == "failed" })
    #expect(store.lastError == nil)
    store.retry(try #require(store.chat("b1").first))
    #expect(store.lastError != nil)
    #expect(store.chat("b1").first?.data.status == "failed")
    #expect(await phone.sent().isEmpty, "not without its files")
}

@MainActor @Test func aResendWhileTheAppIsOpenKeepsItsFiles() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.send("see this", to: "b1", files: [OutgoingFile(name: "a.txt", data: Data("x".utf8))])
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    store.lastError = nil
    store.retry(try #require(store.chat("b1").first))
    #expect(store.lastError == nil)
    #expect(store.chat("b1").first?.data.status == "sending")
    #expect(store.chat("b1").first?.data.attachments?.count == 1)
}

@MainActor @Test func aResendKeepsTheCallItWasSaidIn() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.send("hi", to: "b1", call: "call-1")
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    store.retry(try #require(store.chat("b1").first))
    #expect(store.chat("b1").first?.data.call == "call-1")
}

@MainActor @Test func aMessageHeldOnThePhoneStillGoesAfterTheComputerJoinsAnAccount() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.send("hi", to: "b1")
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    let nonce = try #require(store.chat("b1").first?.data.clientNonce)
    store.updateComputer { $0.cloud = URL(string: "https://api.codync.dev") }

    await phone.back()
    #expect(await until { await phone.sent() == ["hi"] })
    #expect(store.chat("b1").first?.data.clientNonce == nonce)
    #expect(store.chat("b1").first?.data.status != "failed")
}

@MainActor @Test func aHeldMessageIsTakenBackOnThePhoneAfterTheComputerJoinsAnAccount() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.send("hi", to: "b1")
    #expect(await until { store.chat("b1").first?.data.status == "held" })
    store.updateComputer { $0.cloud = URL(string: "https://api.codync.dev") }
    store.cancelQueued(try #require(store.chat("b1").first))
    #expect(store.chat("b1").isEmpty)
}

private func localEntry(_ nonce: String, status: String) -> Entry {
    Entry(id: "local-\(nonce)", seq: .max, botId: "b1", rev: 0, kind: "user", turn: 0,
          data: EntryData(text: "hi", status: status, clientNonce: nonce), createdAt: 1, updatedAt: 0)
}

@MainActor @Test func withNoRelayLeftWhatARelayHeldGoesWithTheSameNonce() async throws {
    let phone = NoRelay()
    defer { phone.end() }
    await phone.start()
    let store = phone.store
    store.upsert(localEntry("n1", status: "waiting"))
    await phone.back()
    #expect(await until { await phone.sent() == ["hi"] })
    #expect(store.chat("b1").first?.data.clientNonce == "n1")
}

@MainActor @Test func whatARelayHeldGoesOnceHelloSaysTheRelayIsGone() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    var computer = randomComputer("Mac")
    let store = BotStore(computer: computer, storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.connection == .computerOffline(lastSeen: nil) })
    store.send("hi", to: "b1")
    #expect(await until { await fake.enqueued.count == 1 })
    let nonce = try #require(store.chat("b1").first?.data.clientNonce)

    await fake.set(.ready(.direct))
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    computer.cloud = nil
    await fake.emit(computer)
    #expect(await until { await fake.sent == ["hi"] })
    #expect(store.chat("b1").first?.data.clientNonce == nonce)
}

@MainActor @Test func aRelaunchKeepsWhatWaitsOnThePhoneAndFailsWhatWasOnItsWay() {
    let (storage, suite) = context()
    defer { storage.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Mac")
    let before = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.ready(.direct)) }
    before.upsert(localEntry("held", status: "held"))
    before.upsert(localEntry("sending", status: "sending"))
    before.retire()

    let after = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.ready(.direct)) }
    defer { after.retire() }
    let statuses = Dictionary(after.chat("b1").map { ($0.id, $0.data.status) }, uniquingKeysWith: { a, _ in a })
    #expect(statuses == ["local-held": "held", "local-sending": "failed"])
}
