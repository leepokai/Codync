import CryptoKit
import Foundation
import Network

/// The end-to-end encrypted channel to one computer (§6, §7): direct WebSocket on the LAN when it
/// answers within 1.5 s, otherwise the cloud relay. Inner RPC multiplexes calls and streams over
/// one channel; the relay adds presence and the offline mailbox. Reconnects by itself until `shutdown()`.
public actor ChannelTransport: RemoteTransport {
    static let identityChanged = "This computer's identity changed. Pair it again to keep using it."

    public internal(set) var computer: Computer
    /// The last connection attempt failed because this app or the computer is too old.
    public internal(set) var needsUpgrade = false
    /// Gave up for good (revoked, identity changed, needs an update); only a new transport can recover.
    public internal(set) var isStopped = false

    let identity: DeviceIdentity
    /// Pairing mode (§4.1): the only request allowed is `pair`; the host then closes with 4100.
    let pairingCode: String?
    let dial: Dialer
    let directBudget: Duration
    private let watchesNetwork: Bool

    /// The screen follows the active signaling route; direct-only connections never request TURN.
    public func screenRelayRequired(failedAttempts: Int = 0) async throws -> Bool {
        try await ready(within: 20)
        // A private relay has no TURN (`RelayCopy.host`), and a computer in no account gets no address.
        return ScreenRoute.asksForTurn(channelOnRelay: link?.route == .relay, route: computer.route ?? .automatic,
                                       hasTurn: computer.cloud != nil && RelayCopy.host(of: computer.cloud) == nil,
                                       failedAttempts: failedAttempts)
    }

    var prefersCloudflare: Bool { computer.route == .cloudflareFirst }

    var state: LinkState = .connecting
    var lastSeen: Date?
    var link: Link?
    var linkGeneration = 0
    var closed = false
    var restartRequested = false
    private var supervisor: Task<Void, Never>?
    private var monitor: NWPathMonitor?
    private var lastInterfaces: [String]?
    /// Relay upgrades refused with 403 since the last handshake. Right after a pairing or an
    /// approval the device can reach the relay before the host's new ACL does, so a few are retried.
    var refusals = 0

    var nextRequestId: UInt32 = 0
    var calls: [UInt32: CheckedContinuation<Data, Error>] = [:]
    var streams: [UInt32: AsyncThrowingStream<Data, Error>.Continuation] = [:]
    var streamIds: [UUID: UInt32] = [:]
    var puts: [String: CheckedContinuation<Void, Error>] = [:]
    var cancels: [String: CheckedContinuation<MailboxCancel, Never>] = [:]
    var lists: [UUID: CheckedContinuation<[QueuedItem], Error>] = [:]
    private var waiters: [UUID: CheckedContinuation<Void, Never>] = [:]

    private var stateObservers: [UUID: AsyncStream<LinkState>.Continuation] = [:]
    var computerObservers: [UUID: AsyncStream<Computer>.Continuation] = [:]
    var mailboxObservers: [UUID: AsyncStream<MailboxEvent>.Continuation] = [:]

    /// What the local network says about a computer's addresses, for one with no relay to learn them from.
    typealias Discovered = @Sendable (ComputerID) async -> [String]
    let discovered: Discovered

    /// One open socket and, once the handshake is done, its channel keys.
    struct Link {
        let socket: any ChannelSocket
        let route: HostRoute
        let generation: Int
        let outbox: AsyncStream<String>.Continuation
        var handshake: RelayCrypto.Handshake?
        var sealer: RelayCrypto.FrameSealer?
        var opener: RelayCrypto.FrameOpener?
        var lastReceived = ContinuousClock.now
    }

    /// How one connection attempt ended.
    enum Outcome: Sendable {
        /// Reconnect now (paired, rekey, network changed).
        case again
        case retry(String)
        /// Retry with backoff while showing this state (a lapsed lease may come back).
        case wait(LinkState)
        /// Don't retry: revoked, identity changed, needs an update, pairing over.
        case stop(LinkState)
    }

    public init(computer: Computer, identity: DeviceIdentity) {
        self.init(computer: computer, identity: identity, pairingCode: nil, discovered: { await LocalDiscovery.shared.urls(for: $0) })
    }

    init(computer: Computer, identity: DeviceIdentity, pairingCode: String?, dial: @escaping Dialer = URLSessionSocket.dial,
         directBudget: Duration = .milliseconds(1500), watchesNetwork: Bool = true,
         discovered: @escaping Discovered = { _ in [] }) {
        self.discovered = discovered
        self.computer = computer
        self.identity = identity
        self.pairingCode = pairingCode
        self.dial = dial
        self.directBudget = directBudget
        self.watchesNetwork = watchesNetwork
    }

    func start() {
        guard supervisor == nil, !closed else { return }
        supervisor = Task { await self.run() }
        guard watchesNetwork else { return }
        // A new network path (Wi-Fi ↔ cellular) can make direct possible again, or dead.
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] path in
            guard path.status == .satisfied else { return }
            let interfaces = path.availableInterfaces.map(\.name)
            Task { await self?.networkChanged(interfaces) }
        }
        monitor.start(queue: .global(qos: .utility))
        self.monitor = monitor
    }

    public func shutdown() {
        guard !closed else { return }
        closed = true
        supervisor?.cancel()
        monitor?.cancel()
        link?.socket.close(code: 1000)
        dropChannel(HostError.unreachable)
        dropMailbox()
        for (_, c) in waiters { c.resume() }
        waiters = [:]
        stateObservers.values.forEach { $0.finish() }
        computerObservers.values.forEach { $0.finish() }
        mailboxObservers.values.forEach { $0.finish() }
        stateObservers = [:]
        computerObservers = [:]
        mailboxObservers = [:]
    }

    /// Reconnects from scratch (the app came back to the foreground, say).
    public func reconnect() {
        guard !closed else { return }
        restartRequested = true
        link?.socket.close(code: 1000)
        wake()
    }

    private func networkChanged(_ interfaces: [String]) {
        // The monitor reports the current path right away, then every small path change
        // (DNS, expensive/constrained flags); only a different set of interfaces is a new network.
        defer { lastInterfaces = interfaces }
        guard let lastInterfaces, lastInterfaces != interfaces else { return }
        reconnect()
    }

    /// Waits until the first attempt is decided (ready, offline, failed, unauthorized).
    func settled(within limit: Duration) async -> LinkState {
        let deadline = ContinuousClock.now + limit
        while state == .connecting, !closed, ContinuousClock.now < deadline {
            await stateChange(until: deadline)
        }
        return state
    }

    // MARK: state

    func setState(_ new: LinkState) {
        guard new != state else { return }
        state = new
        for c in stateObservers.values { c.yield(new) }
        wake()
    }

    private func wake() {
        let w = waiters
        waiters = [:]
        for c in w.values { c.resume() }
    }

    /// Suspends until the state changes, `wake()` is called, or `deadline` passes.
    func stateChange(until deadline: ContinuousClock.Instant) async {
        let key = UUID()
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            waiters[key] = c
            Task {
                try? await Task.sleep(until: deadline)
                self.resumeWaiter(key)
            }
        }
    }

    private func resumeWaiter(_ key: UUID) {
        waiters.removeValue(forKey: key)?.resume()
    }

    private func observeStates(_ key: UUID, _ c: AsyncStream<LinkState>.Continuation) {
        guard !closed else { c.yield(state); c.finish(); return }
        c.yield(state)
        stateObservers[key] = c
    }

    private func observeComputer(_ key: UUID, _ c: AsyncStream<Computer>.Continuation) {
        guard !closed else { c.finish(); return }
        computerObservers[key] = c
    }

    private func observeMailbox(_ key: UUID, _ c: AsyncStream<MailboxEvent>.Continuation) {
        guard !closed else { c.finish(); return }
        mailboxObservers[key] = c
    }

    private func removeObserver(_ key: UUID) {
        stateObservers[key] = nil
        computerObservers[key] = nil
        mailboxObservers[key] = nil
    }

    public nonisolated func states() -> AsyncStream<LinkState> {
        AsyncStream { c in
            let key = UUID()
            Task { await self.observeStates(key, c) }
            c.onTermination = { _ in Task { await self.removeObserver(key) } }
        }
    }

    /// Every merged `Computer` after an inner `hello` (§7.5 step 5); persist it.
    public nonisolated func computerUpdates() -> AsyncStream<Computer> {
        AsyncStream { c in
            let key = UUID()
            Task { await self.observeComputer(key, c) }
            c.onTermination = { _ in Task { await self.removeObserver(key) } }
        }
    }

    public nonisolated func mailboxEvents() -> AsyncStream<MailboxEvent> {
        AsyncStream { c in
            let key = UUID()
            Task { await self.observeMailbox(key, c) }
            c.onTermination = { _ in Task { await self.removeObserver(key) } }
        }
    }
}

/// Reconnect delays: from 1 s doubling to 30 s, full jitter (§7.6).
struct Backoff {
    private var attempt = 0

    mutating func next() -> Duration {
        let cap = min(30.0, pow(2.0, Double(attempt)))
        attempt += 1
        return .milliseconds(Int(Double.random(in: 0...cap) * 1000))
    }
}
