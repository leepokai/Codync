import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A scripted transport: tests set its link state and push events; it records what the store asked.
actor FakeRemote: RemoteTransport {
    enum Checkpoint: Equatable, Sendable {
        case eventsSubscribed
        case called(String)
    }

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
    private var checkpointSinks: [UUID: AsyncStream<Checkpoint>.Continuation] = [:]

    func checkpoints() -> AsyncStream<Checkpoint> {
        let id = UUID()
        let pair = AsyncStream<Checkpoint>.makeStream()
        checkpointSinks[id] = pair.continuation
        if subscribed { pair.continuation.yield(.eventsSubscribed) }
        for call in calls { pair.continuation.yield(.called(call)) }
        pair.continuation.onTermination = { [weak self] _ in
            Task { await self?.removeCheckpointSink(id) }
        }
        return pair.stream
    }

    private func removeCheckpointSink(_ id: UUID) { checkpointSinks[id] = nil }

    private func record(_ checkpoint: Checkpoint) {
        for sink in checkpointSinks.values { sink.yield(checkpoint) }
    }

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
        record(.called(method))
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

    private func addEvents(_ c: AsyncThrowingStream<Data, Error>.Continuation) {
        eventSinks.append(c)
        record(.eventsSubscribed)
    }
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

func randomComputer(_ name: String) -> Computer {
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

func context() -> (SharedStore.Context, String) {
    let suite = "CodyncUITests.\(UUID().uuidString)"
    return (SharedStore.Context(accountID: "user_\(UUID().uuidString)", suite: suite), suite)
}

/// Polls until `condition` holds (2 s at most).
@MainActor
func until(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<200 {
        let satisfied = await condition()
        if satisfied { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

func botEvent(_ id: String, name: String, rev: Int) -> String {
    #"{"type":"bot","bot":{"id":"\#(id)","name":"\#(name)","rev":\#(rev),"lastAt":\#(rev)}}"#
}

extension FakeRemote {
    func setCancelResult(_ result: MailboxCancel) { cancelResult = result }
}
