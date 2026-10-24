import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor
private func shortcutStore(_ storage: SharedStore.Context, remote: FakeRemote) -> BotStore {
    let computer = Computer(id: UUID().uuidString, name: "Mac", signKey: "unused", boxKey: "unused")
    return BotStore(computer: computer, storage: storage) { _ in remote }
}

@MainActor @Test func shortcutSendWaitsForAcceptanceAndPreservesComposerDraft() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    #expect(await until { store.bots["b1"] != nil })
    store.setComposerDraft("Keep my draft", for: "b1")
    let progress = try await store.sendAndWait("  Shortcut text  ", to: "b1")
    if case .delivered = progress {} else { Issue.record("Expected delivered") }
    #expect(await remote.sent == ["Shortcut text"])
    #expect(store.chat("b1").contains { $0.id == "e1" })
    #expect(store.composerDraft(for: "b1") == "Keep my draft")
    await #expect(throws: (any Error).self) { try await store.sendAndWait("  \n", to: "b1") }
    await #expect(throws: (any Error).self) { try await store.sendAndWait("wrong bot", to: "missing") }
    #expect(await remote.sent.count == 1)

    await remote.fail("send", times: 1)
    let cancelled = Task { try await store.sendAndWait("cancel before retry", to: "b1") }
    #expect(await until { await remote.calls.filter { $0 == "send" }.count == 2 })
    cancelled.cancel()
    await #expect(throws: CancellationError.self) { try await cancelled.value }
    try await Task.sleep(for: .milliseconds(600))
    #expect(await remote.sent == ["Shortcut text"])
    #expect(await remote.calls.filter { $0 == "send" }.count == 2)
    #expect(store.chat("b1").contains { $0.data.text == "cancel before retry" && $0.data.status == "failed" })

    try await store.requestVoiceCall(to: "b1")
    #expect(!store.takeVoiceCallRequest(for: "another"))
    #expect(store.takeVoiceCallRequest(for: "b1"))
    #expect(!store.takeVoiceCallRequest(for: "b1"))
    try await store.requestVoiceCall(to: "b1")
    store.retire()
    #expect(store.voiceCallRequest == nil)
    await #expect(throws: (any Error).self) { try await store.requestVoiceCall(to: "b1") }
}

@MainActor @Test func liveShortcutWaitsForBotCatchUpAfterHello() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2}"#)
    #expect(await until { store.connection == .online })
    let opening = Task { try await store.requestVoiceCall(to: "b1") }
    try await Task.sleep(for: .milliseconds(150))
    #expect(store.voiceCallRequest == nil)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    try await opening.value
    #expect(store.voiceCallRequest == "b1")
    // Global rev can include memory writes that never appear in the bot/entry stream.
    let sent = try await store.sendAndWait("after delayed bot", to: "b1")
    if case .delivered = sent {} else { Issue.record("Expected delivered") }
    await remote.emit(#"{"type":"bot","bot":{"id":"b2","name":"Other","rev":2}}"#)
    #expect(await until { store.caughtUp })
    await #expect(throws: (any Error).self) { try await store.requestVoiceCall(to: "missing") }
}

private struct RefusingSendTransport: HostTransport {
    let remote: FakeRemote
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        if method == "send" { throw HostError.http(403, "Send refused") }
        return try await remote.call(method, body: body, timeout: timeout)
    }
    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { remote.stream(request) }
    func states() -> AsyncStream<LinkState> { remote.states() }
}

@MainActor @Test func shortcutSendThrowsWhenHostRefusesMessage() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    let storage = SharedStore.Context(accountID: nil, suite: suite)
    let computer = Computer(id: UUID().uuidString, name: "Mac", signKey: "unused")
    let store = BotStore(computer: computer, storage: storage) { _ in RefusingSendTransport(remote: remote) }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    #expect(await until { store.caughtUp })
    await #expect(throws: (any Error).self) { try await store.sendAndWait("refused", to: "b1") }
    #expect(store.chat("b1").first?.data.status == "failed")
    #expect(await remote.sent.isEmpty)
}

@MainActor @Test func shortcutSendReportsQueuedOnlyAfterMailboxAccepts() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.hostOffline(lastSeen: nil))
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    defer { store.retire() }
    store.bots["b1"] = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Ada"}"#.utf8))
    store.setActive(true)
    #expect(await until { store.canQueue })
    let progress = try await store.sendAndWait("queued text", to: "b1")
    if case .queued = progress {} else { Issue.record("Expected queued") }
    #expect(await remote.enqueued.count == 1)
    #expect(await remote.sent.isEmpty)
    await #expect(throws: (any Error).self) { try await store.requestVoiceCall(to: "b1") }
    #expect(store.voiceCallRequest == nil)
}

@MainActor @Test func backgroundShortcutConnectsWithoutSelectingChatAndDisconnectsAfterAcceptance() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    defer { store.retire() }
    store.setActive(false)
    let sending = Task { try await store.sendAndWait("background text", to: "b1") }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    let progress = try await sending.value
    if case .delivered = progress {} else { Issue.record("Expected delivered") }
    #expect(store.selection == nil)
    #expect(store.voiceCallRequest == nil)
    #expect(store.shortcutSends == 0)
    #expect(await remote.sent == ["background text"])
    #expect(await until { await remote.shutdownCount == 1 })
}

@MainActor @Test func backgroundShortcutKeepsRetryAliveUntilCancelled() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    await remote.fail("send", times: 1)
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1}"#)
    await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Ada","rev":1}}"#)
    let sending = Task { try await store.sendAndWait("cancel background retry", to: "b1") }
    #expect(await until { await remote.calls.contains("send") })
    store.setActive(false)
    #expect(await remote.shutdownCount == 0)
    sending.cancel()
    await #expect(throws: CancellationError.self) { try await sending.value }
    #expect(store.shortcutSends == 0)
    #expect(await until { await remote.shutdownCount == 1 })
    #expect(await remote.sent.isEmpty)
}

@MainActor @Test func retiringStoreClosesTransportDuringBackgroundShortcut() async throws {
    let suite = "BotShortcutTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.connecting)
    let store = shortcutStore(SharedStore.Context(accountID: nil, suite: suite), remote: remote)
    store.setActive(false)
    let sending = Task { try await store.sendAndWait("retired", to: "b1") }
    #expect(await until { store.shortcutSends == 1 && store.transport != nil })
    store.retire()
    await #expect(throws: (any Error).self) { try await sending.value }
    #expect(store.shortcutSends == 0)
    #expect(await until { await remote.shutdownCount == 1 })
    #expect(await remote.sent.isEmpty)
}
