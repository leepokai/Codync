import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

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
    let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.4.0") { fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.mismatch == .updateHost(version: "2.2.0", minimum: VersionMismatch.minHost) })
    // The host updates and restarts: the link drops and comes back.
    await fake.setHello(helloJSON(version: "2.4.0", minApp: "2.3.0"))
    await fake.set(.connecting)
    await fake.set(.ready(.relay))
    #expect(await until { store.mismatch == nil })
    #expect(await until { await fake.subscribed })
    #expect(store.hostVersion == HostVersion(version: "2.4.0", minApp: "2.3.0"))
}

@MainActor @Test func unreadableHelloStillSaysWhichSideToUpdate() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // A newer host whose hello this app can't decode, with or without a minApp that covers it.
    for minApp in ["2.6.0", "2.0.0"] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: "2.6.0", minApp: minApp, backends: #""changed shape""#))
        let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.4.0") { fake }
        store.setActive(true)
        #expect(await until { store.mismatch == .updateApp(minimum: "2.6.0") }, "minApp \(minApp)")
        #expect(store.hello == nil)
        store.retire()
    }
}

@MainActor @Test func unreadableEventsAskForAnAppUpdateOnlyFromANewerHost() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    for (hostVersion, appIsBehind) in [("2.6.0", true), ("2.4.0", false)] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: hostVersion, minApp: "2.3.0"))
        let store = BotStore(computer: randomComputer("Mac"), clientKind: "ios", storage: storage, appVersion: "2.4.0") { fake }
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
