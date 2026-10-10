import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A scripted transport: tests set its link state and push events; it records what the store asked.
actor FakeRemote: RemoteTransport {
    private var state: LinkState
    private var stateSinks: [AsyncStream<LinkState>.Continuation] = []
    private var eventSinks: [AsyncThrowingStream<Data, Error>.Continuation] = []
    private var mailboxSinks: [AsyncStream<MailboxEvent>.Continuation] = []
    private(set) var sent: [String] = []
    private(set) var readReceipts: [Data] = []
    private var failReadReceipts = false
    private(set) var readAttempts = 0
    private(set) var shutdownCount = 0
    private var isShutdown = false

    func setReadFailure(_ fail: Bool) { failReadReceipts = fail }
    private(set) var calls: [String] = []
    private var failures: [String: Int] = [:]
    /// The next `count` calls of `method` fail as if the link dropped under them.
    func fail(_ method: String, times count: Int) { failures[method] = count }
    private(set) var enqueued: [String] = []
    var cancelResult = MailboxCancel.cancelled

    private var hello = #"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0}"#
    /// What `hello` answers from now on (a host updated in place keeps its link).
    func setHello(_ json: String) { hello = json }

    init(_ state: LinkState) { self.state = state }

    var subscribed: Bool { !eventSinks.isEmpty }
    var eventSubscriptionCount: Int { eventSinks.count }

    func set(_ new: LinkState) {
        state = new
        for s in stateSinks { s.yield(new) }
    }

    func emit(_ json: String) {
        for s in eventSinks { s.yield(Data(json.utf8)) }
    }

    func emit(_ event: MailboxEvent) {
        for s in mailboxSinks { s.yield(event) }
    }

    private func respond(_ method: String, _ body: Data) throws -> Data {
        guard !isShutdown else { throw HostError.unreachable }
        calls.append(method)
        if let left = failures[method], left > 0 {
            failures[method] = left - 1
            throw HostError.unreachable
        }
        switch method {
        case "hello":
            return Data(hello.utf8)
        case "markRead":
            readAttempts += 1
            if failReadReceipts { throw HostError.unreachable }
            readReceipts.append(body)
            return Data("{}".utf8)
        case "send":
            let b = (try JSONSerialization.jsonObject(with: body) as? [String: String]) ?? [:]
            sent.append(b["text"] ?? "")
            let entry = #"{"id":"e\#(sent.count)","seq":\#(sent.count),"botId":"\#(b["botId"] ?? "")","rev":\#(sent.count),"kind":"user","turn":1,"data":{"text":"\#(b["text"] ?? "")","clientNonce":"\#(b["clientNonce"] ?? "")"},"createdAt":1,"updatedAt":1}"#
            return Data(#"{"entry":\#(entry)}"#.utf8)
        default:
            return Data("{}".utf8)
        }
    }

    nonisolated func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await respond(method, body)
    }

    private func addEvents(_ c: AsyncThrowingStream<Data, Error>.Continuation) { eventSinks.append(c) }
    private func addState(_ c: AsyncStream<LinkState>.Continuation) {
        isShutdown = false
        c.yield(state)
        stateSinks.append(c)
    }
    private func addMailbox(_ c: AsyncStream<MailboxEvent>.Continuation) { mailboxSinks.append(c) }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { c in Task { await self.addEvents(c) } }
    }

    nonisolated func states() -> AsyncStream<LinkState> {
        AsyncStream { c in Task { await self.addState(c) } }
    }

    nonisolated func computerUpdates() -> AsyncStream<Computer> { AsyncStream { _ in } }

    func enqueue(botId: String, text: String, clientNonce: String, threadId: String?) async throws { enqueued.append(clientNonce) }
    func cancelQueued(clientNonce: String) async -> MailboxCancel { cancelResult }
    func listQueued() async throws -> [QueuedItem] { enqueued.map { QueuedItem(nonce: $0, exp: nil, state: "queued") } }

    nonisolated func mailboxEvents() -> AsyncStream<MailboxEvent> {
        AsyncStream { c in Task { await self.addMailbox(c) } }
    }

    func shutdown() async {
        shutdownCount += 1
        isShutdown = true
        for s in eventSinks { s.finish() }
        for s in stateSinks { s.finish() }
        eventSinks = []
        stateSinks = []
    }
}

private func randomComputer(_ name: String) -> Computer {
    let sk = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
    return Computer(id: RelayCrypto.computerId(signKey: sk), name: name, signKey: sk.base64URLForTests,
                    boxKey: Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation.base64URLForTests,
                    cloud: URL(string: "https://cloud.example.dev"))
}

private extension Data {
    var base64URLForTests: String {
        base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }
}

private func context() -> (SharedStore.Context, String) {
    let suite = "CodyncUITests.\(UUID().uuidString)"
    return (SharedStore.Context(accountID: "user_\(UUID().uuidString)", suite: suite), suite)
}

/// Polls until `condition` holds (2 s at most).
@MainActor
private func until(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<200 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

private func botEvent(_ id: String, name: String, rev: Int) -> String {
    #"{"type":"bot","bot":{"id":"\#(id)","name":"\#(name)","rev":\#(rev),"lastAt":\#(rev)}}"#
}

@MainActor @Test func voiceCallSendsAndReceivesInBackgroundWithoutViewUpdates() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })

    let call = UUID()
    var spoken: [String] = []
    store.beginVoiceCall(call, botId: "b1", speak: { spoken.append($0) }, end: {})
    store.setActive(false)
    // Allow shutdown to run if backgrounding incorrectly scheduled one.
    try await Task.sleep(for: .milliseconds(30))
    store.send("spoken while backgrounded", to: "b1")
    #expect(await until { store.chat("b1").first?.id == "e1" })
    #expect(await fake.sent == ["spoken while backgrounded"])
    #expect(await fake.shutdownCount == 0)

    let reply = #"{"type":"entry","entry":{"id":"reply","seq":2,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"The answer","final":true},"createdAt":1,"updatedAt":1}}"#
    await fake.emit(reply)
    #expect(await until { spoken == ["The answer"] })
    await fake.emit(reply) // Event replays must not read the same reply twice.
    await fake.emit(#"{"type":"entry","entry":{"id":"thread","seq":3,"botId":"b1","threadId":"root","rev":3,"kind":"agent","turn":1,"data":{"text":"Thread answer","final":true},"createdAt":1,"updatedAt":1}}"#)
    await fake.emit(#"{"type":"entry","entry":{"id":"other","seq":4,"botId":"b2","rev":4,"kind":"agent","turn":1,"data":{"text":"Other bot","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b2").last?.id == "other" })
    #expect(spoken == ["The answer"])

    store.setActive(true)
    #expect(await fake.shutdownCount == 0)
    store.endVoiceCall(call)
    #expect(await fake.shutdownCount == 0) // Foreground keeps the ordinary connection.
    store.setActive(false)
    #expect(await until { await fake.shutdownCount == 1 })
}

@MainActor @Test func lastVoiceCallEndingDisconnectsBackgroundStore() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let first = UUID(), second = UUID()
    store.beginVoiceCall(first, botId: "b1", speak: { _ in }, end: {})
    store.beginVoiceCall(second, botId: "b1", speak: { _ in }, end: {})
    store.setActive(false)
    store.endVoiceCall(first)
    try await Task.sleep(for: .milliseconds(30))
    #expect(await fake.shutdownCount == 0)
    store.endVoiceCall(second)
    #expect(await until { await fake.shutdownCount == 1 })
}

@MainActor @Test func retiringStoreEndsVoiceCallAndPreventsLaterSends() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let call = UUID()
    var ended = false
    store.beginVoiceCall(call, botId: "b1", speak: { _ in Issue.record("Retired call received speech") }, end: { ended = true })
    store.setActive(false)
    store.retire()
    #expect(ended)
    #expect(await until { await fake.shutdownCount == 1 })
    store.send("must not send", to: "b1")
    store.beginVoiceCall(UUID(), botId: "b1", speak: { _ in }, end: {})
    #expect(store.chat("b1").isEmpty)
    #expect(await fake.sent.isEmpty)
}

@MainActor @Test func offlineSendsWaitInTheMailbox() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    store.setActive(true)
    #expect(await until { store.connection == .computerOffline(lastSeen: nil) })
    #expect(store.canQueue && store.isOffline)

    store.send("hi", to: "b1")
    #expect(await until { await fake.enqueued.count == 1 })
    let waiting = try #require(store.chat("b1").first)
    #expect(waiting.data.status == "waiting")
    #expect(await fake.sent.isEmpty)

    // Cancelled before the computer saw it: gone.
    store.cancelQueued(waiting)
    #expect(await until { store.chat("b1").isEmpty })

    // Already handed over: it stays, marked delivered.
    store.send("second", to: "b1")
    #expect(await until { await fake.enqueued.count == 2 })
    await fake.setCancelResult(.delivering)
    let second = try #require(store.chat("b1").first)
    store.cancelQueued(second)
    #expect(await until { store.chat("b1").first?.data.status == "delivering" })

    // Expired in the mailbox: failed, can be resent.
    store.send("third", to: "b2")
    #expect(await until { await fake.enqueued.count == 3 })
    let third = try #require(store.chat("b2").first?.data.clientNonce)
    await fake.emit(.expired(nonce: third))
    #expect(await until { store.chat("b2").first?.data.status == "failed" })

    // Back online: sends go straight to the computer, and its copy replaces the local one.
    await fake.set(.ready(.relay))
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Rex", rev: 1))
    #expect(await until { store.connection == .online })
    store.send("live", to: "b3")
    #expect(await until { store.chat("b3").first?.id == "e1" })
    #expect(await fake.sent == ["live"])
    store.retire()
}

@MainActor @Test func accountKeepsComputersApart() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let c1 = randomComputer("Mac")
    let c2 = randomComputer("Linux box")
    storage.computers = [c1, c2]
    let fakes = [c1.id: FakeRemote(.ready(.direct)), c2.id: FakeRemote(.ready(.relay))]
    let account = AccountStore(storage: storage, clientKind: "ios", cloud: nil) { computer in
        BotStore(computer: computer, clientKind: "ios", storage: storage) { fakes[computer.id]! }
    }
    var updates: [BotReference] = []
    account.onBotUpdated = { ref, _ in updates.append(ref) }
    #expect(account.computers.map(\.id) == [c1.id, c2.id])
    for fake in fakes.values { #expect(await until { await fake.subscribed }) }

    // Same bot ID on both computers.
    await fakes[c1.id]!.emit(botEvent("b1", name: "Mac bot", rev: 1))
    await fakes[c2.id]!.emit(botEvent("b1", name: "Linux bot", rev: 2))
    #expect(await until { account.roster.count == 2 })
    let refs = account.roster.map(\.ref)
    #expect(Set(refs).count == 2)
    #expect(Set(refs.map(\.computerId)) == [c1.id, c2.id])
    #expect(account.roster.first { $0.ref.computerId == c2.id }?.bot.name == "Linux bot")
    #expect(Set(updates) == Set(refs))

    // Routing by reference reaches only that computer.
    let ref = try #require(refs.first { $0.computerId == c2.id })
    account.store(for: ref.computerId)?.send("hello", to: ref.botId)
    #expect(await until { await fakes[c2.id]!.sent == ["hello"] })
    #expect(await fakes[c1.id]!.sent.isEmpty)
    #expect(storage.lastComputerId == c2.id)

    // After switching accounts, late events write nothing.
    let old = account.store(for: c1.id)
    account.retire()
    await fakes[c1.id]!.emit(#"{"type":"usage","usage":{"providers":[{"id":"claude","name":"Claude","windows":[],"source":"x","updatedAt":1}]}}"#)
    try? await Task.sleep(for: .milliseconds(100))
    #expect(storage.usage[c1.id] == nil)
    #expect(old?.usage.providers.isEmpty == true)
    #expect(updates.count == 2)
}

@MainActor @Test func computersReorderAndStaySaved() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let a = randomComputer("A"), b = randomComputer("B"), c = randomComputer("C")
    storage.computers = [a, b, c]
    let account = AccountStore(storage: storage, clientKind: "ios", cloud: nil) { computer in
        BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.ready(.direct)) }
    }
    account.move(a.id, to: c.id)
    #expect(account.computers.map(\.id) == [b.id, c.id, a.id])
    account.move(a.id, to: b.id)
    #expect(account.computers.map(\.id) == [a.id, b.id, c.id])
    account.move(c.id, to: b.id)
    #expect(storage.computers.map(\.id) == [a.id, c.id, b.id])
    account.retire()
}

@MainActor @Test func savedComputersKeepTheirColorAndCanBeForgotten() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let saved = randomComputer("Saved")
    storage.computers = [saved]
    let account = AccountStore(storage: storage, clientKind: "ios", cloud: nil) { _ in
        BotStore(computer: saved, clientKind: "ios", storage: storage) { FakeRemote(.ready(.direct)) }
    }
    account.setColor(saved.id, "red")
    #expect(storage.computers.first?.color == "red")
    account.forget(saved.id)
    #expect(storage.computers.isEmpty && account.stores.isEmpty)
    account.retire()
}

extension FakeRemote {
    func setCancelResult(_ result: MailboxCancel) { cancelResult = result }
}


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

@MainActor @Test func offlineTapReconnectsBeforeGivingUp() async throws {
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
    #expect(await until { store.connection == .offline("Can't reach") })
    store.stop("b1")
    #expect(await until { await up.subscribed })
    await up.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { await up.calls.contains("stop") })
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

// MARK: - Voice call operator

/// The realtime voice operator's tools run against the store, the same paths the chat uses.
@MainActor @Test func callOperatorToolsDriveTheBot() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })
    let op = CallOperator(model: store, botId: "b1")

    func json(_ s: String) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: Data(s.utf8))) as? [String: Any] ?? [:]
    }

    #expect(json(op.call("send_to_bot", arguments: ["text": "  run the tests "]))["sent"] as? Bool == true)
    #expect(await until { await fake.sent == ["run the tests"] })
    #expect(json(op.call("send_to_bot", arguments: [:]))["error"] != nil)
    #expect(json(op.call("answer_approval", arguments: ["option": "yes"]))["error"] as? String == "Nothing is waiting for approval.")

    await fake.emit(#"{"type":"entry","entry":{"id":"p1","seq":5,"botId":"b1","rev":5,"kind":"permission","turn":1,"data":{"title":"Run npm test","status":"pending","options":[{"optionId":"a","name":"Allow once","kind":"allow_once"},{"optionId":"r","name":"Reject","kind":"reject_once"}]},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "p1" } })
    let status = json(op.call("bot_status", arguments: [:]))
    #expect((status["approval"] as? [String: Any])?["options"] as? [String] == ["Allow once", "Reject"])
    #expect(json(op.call("answer_approval", arguments: ["option": "maybe"]))["error"] as? String == "No such option.")
    #expect(json(op.call("answer_approval", arguments: ["option": "allow"]))["answered"] as? String == "Allow once")
    #expect(store.answering["p1"] == "a")

    let recent = json(op.call("recent_messages", arguments: ["count": 1]))["messages"] as? [[String: String]]
    #expect(recent?.count == 1)
    #expect(recent?.first?["from"] == "approval request")
    #expect(op.reply("**Done**, see `x`").hasPrefix("[Ada replied] "))
}

/// Approvals reach a call as notices, replies as replies (a realtime operator words them differently).
@MainActor @Test func voiceCallGetsApprovalsAsNotices() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })
    var replies: [String] = []
    var notices: [String] = []
    store.beginVoiceCall(UUID(), botId: "b1", speak: { replies.append($0) }, announce: { notices.append($0) }, end: {})
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":2,"lastAt":2,"status":"needsInput"}}"#)
    #expect(await until { notices == ["Ada needs your approval in the chat."] })
    #expect(replies.isEmpty)
}

// MARK: - Version compatibility (docs/reference/compatibility.md)

private func helloJSON(version: String, minApp: String? = nil, backends: String = "[]") -> String {
    let min = minApp.map { #","minApp":"\#($0)""# } ?? ""
    return #"{"hostId":"h1","name":"Mac","version":"\#(version)"\#(min),"os":"macos","backends":\#(backends),"rev":0}"#
}

@MainActor @Test func hostNeedingANewerAppStopsSyncAndAsksForTheUpdate() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.6.0", minApp: "2.5.0"))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.4.0") { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.mismatch == .updateApp(minimum: "2.5.0") })
    #expect(await until { store.connection == .online })
    try await Task.sleep(for: .milliseconds(200))
    // Nothing this app can't read is synced.
    #expect(await !fake.subscribed)
}

@MainActor @Test func updatedHostIsNoticedOnReconnect() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.2.0"))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.12.0") { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.mismatch == .updateHost(version: "2.2.0", minimum: VersionMismatch.minHost) })
    // The host updates and restarts: the link drops and comes back.
    await fake.setHello(helloJSON(version: "2.12.0", minApp: "2.3.0"))
    await fake.set(.connecting)
    await fake.set(.ready(.relay))
    #expect(await until { store.mismatch == nil })
    #expect(await until { await fake.subscribed })
    #expect(store.hostVersion == HostVersion(version: "2.12.0", minApp: "2.3.0"))
}

@MainActor @Test func unreadableHelloStillSaysWhichSideToUpdate() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // A newer host whose hello this app can't decode, with or without a minApp that covers it.
    for minApp in ["2.14.0", "2.0.0"] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: "2.14.0", minApp: minApp, backends: #""changed shape""#))
        let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.12.0") { fake }
        store.setActive(true)
        #expect(await until { store.mismatch == .updateApp(minimum: "2.14.0") }, "minApp \(minApp)")
        #expect(store.hello == nil)
        store.retire()
    }
}

@MainActor @Test func unreadableEventsAskForAnAppUpdateOnlyFromANewerHost() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    for (hostVersion, appIsBehind) in [("2.14.0", true), ("2.12.0", false)] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: hostVersion, minApp: "2.3.0"))
        let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.12.0") { fake }
        store.setActive(true)
        let broken = #"{"type":"bot","bot":{"id":"b1","name":7}}"#
        #expect(await until { await fake.eventSubscriptionCount == 1 })
        await fake.emit(broken)
        // The first one rewinds and subscribes again; the second one is final.
        #expect(await until { await fake.eventSubscriptionCount == 2 })
        await fake.emit(broken)
        if appIsBehind {
            #expect(await until { store.mismatch == .updateApp(minimum: hostVersion) })
        } else {
            try await Task.sleep(for: .milliseconds(200))
            #expect(store.mismatch == nil)
        }
        store.retire()
    }
}

@MainActor @Test func composerDraftsPersistAcrossNavigationAndReopening() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Mac")
    let store = BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.connecting) }
    let text = "  中文\nunfinished 🐱\n"
    store.setComposerDraft(text, for: "first")
    store.setComposerDraft("Other draft", for: "second")
    store.setComposerDraft("Reply", for: "first", thread: "root")
    store.selection = "second"
    #expect(store.composerDraft(for: "first") == text)
    store.retire()
    store.setComposerDraft("late edit", for: "first")
    let reopened = BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.connecting) }
    defer { reopened.retire() }
    #expect(reopened.composerDraft(for: "first") == text)
    #expect(reopened.composerDraft(for: "first", thread: "root") == "Reply")
    reopened.setComposerDraft("", for: "first")
    #expect(reopened.composerDraft(for: "first").isEmpty)
    #expect(reopened.composerDraft(for: "second") == "Other draft")
    #expect(reopened.composerDraft(for: "first", thread: "root") == "Reply")
    let other = BotStore(computer: randomComputer("Other"), clientKind: "ios", storage: storage) { FakeRemote(.connecting) }
    defer { other.retire() }
    #expect(other.composerDraft(for: "first").isEmpty)
    storage.erase()
    #expect(storage.composerDrafts.isEmpty)
}

@MainActor @Test func sendingDraftNeverClearsAnotherConversationOrLaterText() async {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("first", name: "First", rev: 1))
    #expect(await until { store.connection == .online })
    store.setComposerDraft("First message", for: "first")
    store.setComposerDraft("Other draft", for: "second")
    store.setComposerDraft("Reply", for: "first", thread: "root")
    #expect(store.sendComposerDraft(to: "first"))
    #expect(store.composerDraft(for: "first").isEmpty)
    store.setComposerDraft("Next unfinished message", for: "first")
    #expect(await until { store.chat("first").last?.id == "e1" })
    #expect(store.composerDraft(for: "first") == "Next unfinished message")
    #expect(store.composerDraft(for: "second") == "Other draft")
    #expect(store.composerDraft(for: "first", thread: "root") == "Reply")
    let anotherAccount = SharedStore.Context(accountID: "another-user", suite: suite)
    #expect(anotherAccount.composerDrafts.isEmpty)
}

@MainActor @Test func existingCachesKeepTheirMessagesAndLocalSends() throws {
    let (storage, suite) = context()
    defer { storage.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Fixture")
    let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "0"
    let url = URL.cachesDirectory.appending(path: "codync-mirror-\(Bundle.main.bundleIdentifier ?? "app")-\(storage.id)-\(computer.id).json")
    let cached = #"""
    {"stamp":"\#(build)/3.0.0","hostId":"h1","rev":77,"bots":[],"entries":[
      {"id":"notice","seq":1,"botId":"dex","rev":77,"kind":"notice","turn":1,"createdAt":1,"updatedAt":1,
       "data":{"text":"Message from Miles","status":"completed"}},
      {"id":"local-pending","seq":2,"botId":"dex","rev":0,"kind":"user","turn":1,"createdAt":2,"updatedAt":2,
       "data":{"text":"My unsent message","status":"failed"}}
    ]}
    """#
    try Data(cached.utf8).write(to: url)
    let store = BotStore(computer: computer, clientKind: "ios", storage: storage) { FakeRemote(.ready(.direct)) }
    defer { store.retire() }
    // Adding conversation metadata does not invalidate the existing base cache.
    #expect(store.allEntries("dex").map(\.id) == ["notice", "local-pending"])
    #expect(store.allEntries("dex").last?.data.text == "My unsent message")
    store.saveCache()
    let bytes = try Data(contentsOf: url)
    let cache = try #require(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
    #expect(cache["rev"] as? Int == 77)
}

private func entryEvent(_ id: String, seq: Int, rev: Int, turn: Int = 5, text: String = "hi") -> String {
    #"{"type":"entry","entry":{"id":"\#(id)","seq":\#(seq),"botId":"b1","rev":\#(rev),"kind":"agent","turn":\#(turn),"data":{"text":"\#(text)","final":true},"createdAt":1,"updatedAt":1}}"#
}

private func windowEntry(_ id: String, seq: Int64, rev: Int64 = 1, threadId: String? = nil) -> Entry {
    var data = EntryData(text: id)
    data.final = true
    return Entry(id: id, seq: seq, botId: "b1", threadId: threadId, rev: rev, kind: "agent", turn: 0, data: data, createdAt: 1, updatedAt: 1)
}

@MainActor @Test func windowRuleDropsOnlyUnknownMainChatEntriesBelowTheFloor() {
    let below = windowEntry("a", seq: 5)
    #expect(BotStore.isOutsideLoadedWindow(below, floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(below, floor: 100, held: true))
    #expect(!BotStore.isOutsideLoadedWindow(windowEntry("a", seq: 100), floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(windowEntry("a", seq: 150), floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(below, floor: nil, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(windowEntry("a", seq: 5, threadId: "t"), floor: 100, held: false))
}

/// A connection that resumes from a rev > 0 leaves a rewritten old entry to history paging.
@MainActor @Test func resumedConnectionDropsRewrittenEntriesBelowTheMirrorsFloor() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.upsert(windowEntry("e100", seq: 100))
    store.upsert(windowEntry("local-1", seq: 0)) // optimistic: not a floor
    store.rev = 10
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":20}"#)
    await fake.emit(botEvent("b1", name: "Bot", rev: 11))
    // Rewritten old entry: dropped, though its rev is consumed.
    await fake.emit(entryEvent("old", seq: 5, rev: 12, turn: 0))
    // New entry above the floor and an update to a held one apply.
    await fake.emit(entryEvent("e101", seq: 101, rev: 13))
    await fake.emit(entryEvent("e100", seq: 100, rev: 14, text: "edited"))
    #expect(await until { store.chat("b1").first { $0.id == "e100" }?.data.text == "edited" && store.chat("b1").contains { $0.id == "e101" } })
    #expect(!store.chat("b1").contains { $0.id == "old" })
    #expect(store.rev == 14)
    // History results still insert older entries.
    store.upsert(windowEntry("old", seq: 5))
    #expect(store.chat("b1").contains { $0.id == "old" })
}

/// From rev 0 the catch-up is rev-ordered, not seq-ordered, and a sent message's RPC response may
/// land first: nothing is below a floor.
@MainActor @Test func freshCatchUpAcceptsEntriesInRevOrder() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage) { fake }
    defer { store.retire() }
    store.upsert(windowEntry("sent", seq: 100)) // RPC response before the catch-up
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":20}"#)
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    await fake.emit(entryEvent("e50", seq: 50, rev: 2, turn: 0))
    await fake.emit(entryEvent("e5", seq: 5, rev: 3, turn: 0))
    #expect(await until { store.chat("b1").map(\.id) == ["e5", "e50", "sent"] })
}
