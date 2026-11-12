import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// The real iPhone store and channel, through a relay, against a real host that restarts, driven by
/// `host/tests/iphone_reconnect_e2e.rs`, which runs the relay, seeds the host and restarts it. Runs only from there:
/// `TEST_RUNNER_CODYNC_RESTART_DIR=<dir>` is where the two sides hand each other each step.
@MainActor @Test(.enabled(if: restartDir != nil), .timeLimit(.minutes(5)))
func aRestartedHostCatchesTheIPhoneUpOnlyOnWhatIsNew() async throws {
    let dir = try #require(restartDir)
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let identity = DeviceIdentity(signing: .init(), push: .init())
    // A debug host links for Codync Dev; the test bundle reads links as the app it stands for.
    var link = try #require(URLComponents(string: try await handoff(dir, "pairing")))
    link.scheme = AppEnvironment.current.urlScheme
    let pairing = try #require(link.url.flatMap(Pairing.init(url:)))
    let computer = try await HostConnector.pair(pairing, identity: identity, deviceName: "Restart test", platform: "ios",
                                                certificate: nil, claim: nil).computer
    let open = { () -> (BotStore, LinkLog) in
        let log = LinkLog()
        let store = BotStore(computer: computer, storage: storage) { current in
            Recorder(channel: try await HostConnector.connect(current, identity: identity), log: log)
        }
        store.setActive(true)
        return (store, log)
    }

    // The first sync: every chat, each holding its most recent messages.
    var (store, log) = open()
    // The driver's 30 chats, beside the host's own assistant.
    let seeded = { store.bots.values.filter { $0.name.hasPrefix("Bot ") } }
    #expect(await wait { store.caughtUp && seeded().count == 30 && seeded().allSatisfy { !store.chat($0.id).isEmpty } },
            "\(store.connection), \(seeded().count) chats, rev \(store.rev), caught up: \(store.caughtUp)")
    let synced = store.rev
    try write(dir, "synced", "\(synced)")

    // The host restarts under the open store, which reconnects from its rev with nothing to fetch. `caughtUp` still
    // holds from the first sync, so the new stream's own catch-up says when it's done.
    _ = try await handoff(dir, "restarted")
    #expect(await wait { await log.subscriptions.dropFirst().first?.complete == true })
    let reconnect = await log.subscriptions
    #expect(reconnect.map { $0.since } == [0, synced])
    #expect(reconnect.last?.entries == [])
    await expectOnlyRelay(log)

    // Closed, then opened again after two messages: the cache's rev picks up just those.
    store.retire()
    try write(dir, "closed", "")
    let sent = try await handoff(dir, "sent").split(separator: ",").map(String.init)
    (store, log) = open()
    defer { store.retire() }
    #expect(await wait { await log.subscriptions.first?.complete == true })
    let reopened = try #require(await log.subscriptions.first)
    #expect(reopened.since == synced)
    #expect(reopened.entries.sorted() == sent.sorted())
    // The log notes each event before the store applies it.
    #expect(await wait { sent.allSatisfy { id in store.bots.keys.contains { store.chat($0).contains { $0.id == id } } } })
    await expectOnlyRelay(log)
}

/// The store connected, and only through the relay: the host's own addresses don't answer the simulator.
private func expectOnlyRelay(_ log: LinkLog) async {
    let routes = await log.routes
    #expect(!routes.isEmpty && routes.allSatisfy { $0 == .relay }, "\(routes)")
}

private let restartDir = ProcessInfo.processInfo.environment["CODYNC_RESTART_DIR"].map(URL.init(fileURLWithPath:))

/// What one events stream fetched before going live: its `since` and the catch-up's entry ids.
struct Subscription: Sendable {
    var since: Int64
    var catchUp: Int?
    var entries: [String] = []
    var seen = 0
    var complete: Bool { catchUp.map { seen >= $0 } ?? false }
}

/// What one store's link did: the route of each connection, and what each events stream caught up on.
actor LinkLog {
    private(set) var routes: [HostRoute] = []
    private(set) var subscriptions: [Subscription] = []

    func connected(_ route: HostRoute) {
        routes.append(route)
    }

    func open(since: Int64) -> Int {
        subscriptions.append(Subscription(since: since))
        return subscriptions.count - 1
    }

    /// The first event is the stream's `hello`, which says how many catch-up events follow.
    func note(_ data: Data, in index: Int) {
        guard let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        guard let catchUp = subscriptions[index].catchUp else {
            subscriptions[index].catchUp = event["catchUp"] as? Int ?? 0
            return
        }
        guard subscriptions[index].seen < catchUp else { return }
        subscriptions[index].seen += 1
        if event["type"] as? String == "entry", let id = (event["entry"] as? [String: Any])?["id"] as? String {
            subscriptions[index].entries.append(id)
        }
    }
}

/// The real channel, noting each connection's route and what each events stream catches up on.
struct Recorder: RemoteTransport {
    let channel: ChannelTransport
    let log: LinkLog

    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        guard case let .events(since, _) = request else { return channel.stream(request) }
        let (events, log) = (channel.stream(request), log)
        return AsyncThrowingStream { continuation in
            let task = Task {
                let index = await log.open(since: since)
                do {
                    for try await data in events {
                        await log.note(data, in: index)
                        continuation.yield(data)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await channel.call(method, body: body, timeout: timeout)
    }
    /// Noted before the store sees it, so a store that is ready has its route in the log.
    func states() -> AsyncStream<LinkState> {
        let (states, log) = (channel.states(), log)
        return AsyncStream { continuation in
            let task = Task {
                for await state in states {
                    if case let .ready(route) = state { await log.connected(route) }
                    continuation.yield(state)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
    func waitForRoute() async throws { try await channel.waitForRoute() }
    func computerUpdates() -> AsyncStream<Computer> { channel.computerUpdates() }
    func enqueue(botId: String, text: String, clientNonce: String, replyTo: String?) async throws {
        try await channel.enqueue(botId: botId, text: text, clientNonce: clientNonce, replyTo: replyTo)
    }
    func cancelQueued(clientNonce: String) async -> MailboxCancel { await channel.cancelQueued(clientNonce: clientNonce) }
    func listQueued() async throws -> [QueuedItem] { try await channel.listQueued() }
    func mailboxEvents() -> AsyncStream<MailboxEvent> { channel.mailboxEvents() }
    func shutdown() async { await channel.shutdown() }
}

/// Waits for the driver's `name` file and returns what it holds.
private func handoff(_ dir: URL, _ name: String) async throws -> String {
    let file = dir.appending(path: name)
    for _ in 0..<1200 {
        if let text = try? String(contentsOf: file, encoding: .utf8) { return text }
        try await Task.sleep(for: .milliseconds(100))
    }
    throw HandoffMissing(name: name)
}

struct HandoffMissing: Error {
    var name: String
}

private func write(_ dir: URL, _ name: String, _ text: String) throws {
    try Data(text.utf8).write(to: dir.appending(path: name), options: .atomic)
}

/// Polls until `condition` holds, for as long as a host restart and a channel's backoff take.
@MainActor private func wait(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<1200 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(100))
    }
    return false
}
