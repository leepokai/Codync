import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func voiceCallSendsAndReceivesInBackgroundWithoutViewUpdates() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })

    let call = UUID()
    var spoken: [String] = []
    store.beginVoiceCall(call, hear: { spoken.append($0.text) }, end: {})
    store.setActive(false)
    // Allow shutdown to run if backgrounding incorrectly scheduled one.
    try await Task.sleep(for: .milliseconds(30))
    store.send("spoken while backgrounded", to: "b1")
    #expect(await until { store.chat("b1").first?.id == "e1" })
    #expect(await fake.sent == ["spoken while backgrounded"])
    #expect(await fake.shutdownCount == 0)

    // The computer tells the call what to say; another call's cue isn't this one's.
    await fake.emit(#"{"type":"call","call":"\#(UUID().uuidString)","kind":"reply","text":"Another call","seq":1}"#)
    await fake.emit(#"{"type":"call","call":"\#(call.uuidString)","kind":"reply","text":"The answer","seq":1}"#)
    #expect(await until { spoken == ["The answer"] })

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
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let first = UUID(), second = UUID()
    store.beginVoiceCall(first, hear: { _ in }, end: {})
    store.beginVoiceCall(second, hear: { _ in }, end: {})
    store.setActive(false)
    store.endVoiceCall(first)
    try await Task.sleep(for: .milliseconds(30))
    #expect(await fake.shutdownCount == 0)
    store.endVoiceCall(second)
    #expect(await until { await fake.shutdownCount == 1 })
}

@MainActor @Test func aCallsEndKeepsTheConnectionUntilItReachesTheComputer() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    let call = UUID()
    store.beginVoiceCall(call, hear: { _ in }, end: {})
    store.setActive(false)
    // Hung up from the Live Activity, with the app in the background.
    store.beginCallEnd()
    store.endVoiceCall(call)
    try await Task.sleep(for: .milliseconds(30))
    #expect(await fake.shutdownCount == 0, "the call's end still has to reach the computer")
    store.finishCallEnd()
    #expect(await until { await fake.shutdownCount == 1 })
}

@MainActor @Test func retiringStoreEndsVoiceCallAndPreventsLaterSends() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let call = UUID()
    var ended = false
    store.beginVoiceCall(call, hear: { _ in Issue.record("Retired call received speech") }, end: { ended = true })
    store.setActive(false)
    store.retire()
    #expect(ended)
    #expect(await until { await fake.shutdownCount == 1 })
    store.send("must not send", to: "b1")
    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    #expect(store.chat("b1").isEmpty)
    #expect(await fake.sent.isEmpty)
}

@MainActor @Test func offlineSendsWaitInTheMailbox() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
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
    let account = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in fakes[computer.id]! }
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
    let account = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in FakeRemote(.ready(.direct)) }
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
    let account = AccountStore(storage: storage, cloud: nil) { _ in
        BotStore(computer: saved, storage: storage) { _ in FakeRemote(.ready(.direct)) }
    }
    account.setColor(saved.id, "red")
    #expect(storage.computers.first?.color == "red")
    account.forget(saved.id)
    #expect(storage.computers.isEmpty && account.stores.isEmpty)
    account.retire()
}

@MainActor @Test func eachComputersPushSwitchIsSavedWithIt() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let saved = randomComputer("Saved")
    storage.computers = [saved]
    let account = AccountStore(storage: storage, cloud: nil) { _ in
        BotStore(computer: saved, storage: storage) { _ in FakeRemote(.ready(.direct)) }
    }
    defer { account.retire() }
    account.setNotifications(saved.id, false)
    #expect(storage.computers.first?.notifications == false)
    #expect(account.store(for: saved.id)?.computer.notifications == false)
    account.setNotifications(saved.id, true)
    #expect(storage.computers.first?.notifications == nil)
    #expect(account.store(for: saved.id)?.computer.notifications == nil)
}




@MainActor @Test func readingOneChatDoesNotAcknowledgeAnother() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let token = UUID()
    store.setReading(token, botId: "b1", active: true)
    #expect(await until { await fake.readReceipts.count == 1 })
    await fake.emit(#"{"type":"entry","entry":{"id":"other","seq":1,"botId":"b2","rev":2,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b2").last?.id == "other" })
    try await Task.sleep(for: .milliseconds(50))
    #expect(await fake.readReceipts.count == 1)
    await fake.emit(#"{"type":"entry","entry":{"id":"own","seq":2,"botId":"b1","rev":3,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { await fake.readReceipts.count == 2 })
    for receipt in await fake.readReceipts {
        let body = try #require(JSONSerialization.jsonObject(with: receipt) as? [String: Any])
        #expect(body.keys.sorted() == ["botId"])
        #expect(body["botId"] as? String == "b1")
    }
}








/// The computer's cues reach a call once each: one replayed after a reconnect isn't said again, even after an engine
/// switch.
@MainActor @Test func voiceCallHearsEachOfTheComputersCuesOnce() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })
    var heard: [CallCue] = []
    let call = UUID()
    store.beginVoiceCall(call, hear: { heard.append($0) }, end: {})
    let cues = [("notice", "Ada needs the user's approval: Edit a.txt", 1), ("progress", "Ada: Running tests", 2), ("reply", "Done.", 3)]
    func emit(_ kind: String, _ text: String, _ seq: Int) async {
        await fake.emit(#"{"type":"call","call":"\#(call.uuidString)","kind":"\#(kind)","text":"\#(text)","seq":\#(seq)}"#)
    }
    for (kind, text, seq) in cues { await emit(kind, text, seq) }
    #expect(await until { heard.count == 3 })
    #expect(heard.map(\.kind) == [.notice, .progress, .reply])
    // The engine switches; the reconnect's catch-up replays what the computer kept.
    store.endVoiceCall(call)
    store.beginVoiceCall(call, hear: { heard.append($0) }, end: {})
    await emit("reply", "Done.", 3)
    await emit("reply", "Missed while away.", 4)
    #expect(await until { heard.last?.text == "Missed while away." })
    #expect(heard.map(\.text) == cues.map(\.1) + ["Missed while away."])
    // A newer host's new cue kind is passed over, not a reason to rewind.
    await fake.emit(#"{"type":"call","call":"\#(call.uuidString)","kind":"whisper","text":"?","seq":5}"#)
    try await Task.sleep(for: .milliseconds(30))
    #expect(store.unreadable == false)
}

// MARK: - Version compatibility (docs/reference/compatibility.md)

func helloJSON(version: String, minApp: String? = nil, backends: String = "[]", canUpdate: Bool = false,
               latestVersion: String? = nil) -> String {
    let min = minApp.map { #","minApp":"\#($0)""# } ?? ""
    let latest = latestVersion.map { #","latestVersion":"\#($0)""# } ?? ""
    return #"{"hostId":"h1","name":"Mac","version":"\#(version)"\#(min)\#(latest),"os":"macos","backends":\#(backends),"rev":0,"canUpdate":\#(canUpdate)}"#
}

@MainActor @Test func hostNeedingANewerAppStopsSyncAndAsksForTheUpdate() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.6.0", minApp: "2.5.0"))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: "2.4.0") { _ in fake }
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
    let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: VersionMismatch.minHost) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.mismatch == .updateHost(version: "2.2.0", minimum: VersionMismatch.minHost) })
    // The host updates and restarts: the link drops and comes back.
    await fake.setHello(helloJSON(version: VersionMismatch.minHost, minApp: "2.3.0"))
    await fake.set(.connecting)
    await fake.set(.ready(.relay))
    #expect(await until { store.mismatch == nil })
    #expect(await until { await fake.subscribed })
    #expect(store.hostVersion == HostVersion(version: VersionMismatch.minHost, minApp: "2.3.0"))
}

@MainActor @Test func unreadableHelloStillSaysWhichSideToUpdate() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // A newer host whose hello this app can't decode, with or without a minApp that covers it.
    for minApp in ["9.1.0", "2.0.0"] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: "9.1.0", minApp: minApp, backends: #""changed shape""#))
        let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: "9.0.0") { _ in fake }
        store.setActive(true)
        #expect(await until { store.mismatch == .updateApp(minimum: "9.1.0") }, "minApp \(minApp)")
        #expect(store.hello == nil)
        store.retire()
    }
}

@MainActor @Test func unreadableEventsAskForAnAppUpdateOnlyFromANewerHost() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    for (hostVersion, appIsBehind) in [("9.1.0", true), ("9.0.0", false)] {
        let fake = FakeRemote(.ready(.relay))
        await fake.setHello(helloJSON(version: hostVersion, minApp: "2.3.0"))
        let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: "9.0.0") { _ in fake }
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

@MainActor @Test func draftsFromBeforeQuoteRepliesStayWithTheirChat() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Mac")
    storage.composerDrafts[computer.id] = [#"["first",null]"#: "Unsent", #"["first","root"]"#: "Aside", "second": "New"]
    let store = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    defer { store.retire() }
    #expect(store.composerDraft(for: "first") == "Unsent")
    #expect(store.composerDraft(for: "second") == "New")
    store.setComposerDraft("Unsent!", for: "first")
    #expect(storage.composerDrafts[computer.id] == ["first": "Unsent!", "second": "New"])
}

@MainActor @Test func composerDraftsPersistAcrossNavigationAndReopening() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Mac")
    let store = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    let text = "  中文\nunfinished 🐱\n"
    store.setComposerDraft(text, for: "first")
    store.setComposerDraft("Other draft", for: "second")
    store.selection = "second"
    #expect(store.composerDraft(for: "first") == text)
    store.retire()
    store.setComposerDraft("late edit", for: "first")
    let reopened = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    defer { reopened.retire() }
    #expect(reopened.composerDraft(for: "first") == text)
    reopened.setComposerDraft("", for: "first")
    #expect(reopened.composerDraft(for: "first").isEmpty)
    #expect(reopened.composerDraft(for: "second") == "Other draft")
    let other = BotStore(computer: randomComputer("Other"), storage: storage) { _ in FakeRemote(.connecting) }
    defer { other.retire() }
    #expect(other.composerDraft(for: "first").isEmpty)
    storage.erase()
    #expect(storage.composerDrafts.isEmpty)
}

@MainActor @Test func sendingDraftNeverClearsAnotherConversationOrLaterText() async {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("first", name: "First", rev: 1))
    #expect(await until { store.connection == .online })
    store.setComposerDraft("First message", for: "first")
    store.setComposerDraft("Other draft", for: "second")
    #expect(store.sendComposerDraft(to: "first"))
    #expect(store.composerDraft(for: "first").isEmpty)
    store.setComposerDraft("Next unfinished message", for: "first")
    #expect(await until { store.chat("first").last?.id == "e1" })
    #expect(store.composerDraft(for: "first") == "Next unfinished message")
    #expect(store.composerDraft(for: "second") == "Other draft")
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
    let store = BotStore(computer: computer, storage: storage) { _ in FakeRemote(.ready(.direct)) }
    defer { store.retire() }
    // Adding conversation metadata does not invalidate the existing base cache.
    #expect(store.chat("dex").map(\.id) == ["notice", "local-pending"])
    #expect(store.chat("dex").last?.data.text == "My unsent message")
    store.saveCache()
    let bytes = try Data(contentsOf: url)
    let cache = try #require(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
    #expect(cache["rev"] as? Int == 77)
}

private func entryEvent(_ id: String, seq: Int, rev: Int, turn: Int = 5, text: String = "hi") -> String {
    #"{"type":"entry","entry":{"id":"\#(id)","seq":\#(seq),"botId":"b1","rev":\#(rev),"kind":"agent","turn":\#(turn),"data":{"text":"\#(text)","final":true},"createdAt":1,"updatedAt":1}}"#
}

private func windowEntry(_ id: String, seq: Int64, rev: Int64 = 1) -> Entry {
    var data = EntryData(text: id)
    data.final = true
    return Entry(id: id, seq: seq, botId: "b1", rev: rev, kind: "agent", turn: 0, data: data, createdAt: 1, updatedAt: 1)
}

@MainActor @Test func windowRuleDropsOnlyUnknownEntriesBelowTheFloor() {
    let below = windowEntry("a", seq: 5)
    #expect(BotStore.isOutsideLoadedWindow(below, floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(below, floor: 100, held: true))
    #expect(!BotStore.isOutsideLoadedWindow(windowEntry("a", seq: 100), floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(windowEntry("a", seq: 150), floor: 100, held: false))
    #expect(!BotStore.isOutsideLoadedWindow(below, floor: nil, held: false))
}

/// A connection that resumes from a rev > 0 leaves a rewritten old entry to history paging.
@MainActor @Test func resumedConnectionDropsRewrittenEntriesBelowTheMirrorsFloor() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
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
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
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
