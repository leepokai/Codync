import CryptoKit
import Foundation
import Network
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Channel")

/// The end-to-end encrypted channel to one computer (§6, §7): direct WebSocket on the LAN when it
/// answers within 1.5 s, otherwise the cloud relay. Inner RPC multiplexes calls and streams over
/// one channel; the relay adds presence and the offline mailbox. Reconnects by itself until `shutdown()`.
public actor ChannelTransport: RemoteTransport {
    static let identityChanged = "This computer's identity changed. Pair it again to keep using it."

    public private(set) var computer: Computer
    /// The last connection attempt failed because this app or the computer is too old.
    public private(set) var needsUpgrade = false
    /// Gave up for good (revoked, identity changed, needs an update); only a new transport can recover.
    public private(set) var isStopped = false

    private let identity: DeviceIdentity
    /// Pairing mode (§4.1): the only request allowed is `pair`; the host then closes with 4100.
    private let pairingCode: String?
    private let dial: Dialer
    private let directBudget: Duration
    private let watchesNetwork: Bool

    private var state: LinkState = .connecting
    private var lastSeen: Date?
    private var link: Link?
    private var linkGeneration = 0
    private var closed = false
    private var restartRequested = false
    private var supervisor: Task<Void, Never>?
    private var monitor: NWPathMonitor?
    private var sawFirstPath = false
    /// Relay upgrades refused with 403 since the last handshake. Right after a pairing or an
    /// approval the device can reach the relay before the host's new ACL does, so a few are retried.
    private var refusals = 0

    private var nextRequestId: UInt32 = 0
    private var calls: [UInt32: CheckedContinuation<Data, Error>] = [:]
    private var streams: [UInt32: AsyncThrowingStream<Data, Error>.Continuation] = [:]
    private var streamIds: [UUID: UInt32] = [:]
    private var puts: [String: CheckedContinuation<Void, Error>] = [:]
    private var cancels: [String: CheckedContinuation<MailboxCancel, Never>] = [:]
    private var lists: [UUID: CheckedContinuation<[QueuedItem], Never>] = [:]
    private var waiters: [UUID: CheckedContinuation<Void, Never>] = [:]

    private var stateObservers: [UUID: AsyncStream<LinkState>.Continuation] = [:]
    private var computerObservers: [UUID: AsyncStream<Computer>.Continuation] = [:]
    private var mailboxObservers: [UUID: AsyncStream<MailboxEvent>.Continuation] = [:]

    /// One open socket and, once the handshake is done, its channel keys.
    private struct Link {
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
    private enum Outcome: Sendable {
        /// Reconnect now (paired, rekey, network changed).
        case again
        case retry(String)
        /// Retry with backoff while showing this state (a lapsed lease may come back).
        case wait(LinkState)
        /// Don't retry: revoked, identity changed, needs an update, pairing over.
        case stop(LinkState)
    }

    public init(computer: Computer, identity: DeviceIdentity) {
        self.init(computer: computer, identity: identity, pairingCode: nil)
    }

    init(computer: Computer, identity: DeviceIdentity, pairingCode: String?, dial: @escaping Dialer = URLSessionSocket.dial,
         directBudget: Duration = .milliseconds(1500), watchesNetwork: Bool = true) {
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
            Task { await self?.networkChanged() }
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

    private func networkChanged() {
        // The monitor reports the current path right away; only later ones are changes.
        guard sawFirstPath else { sawFirstPath = true; return }
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

    private func setState(_ new: LinkState) {
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
    private func stateChange(until deadline: ContinuousClock.Instant) async {
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

    // MARK: connection loop (§7.5, §7.6)

    private func run() async {
        var backoff = Backoff()
        while !closed, !Task.isCancelled {
            if case .ready = state { setState(.connecting) }
            let started = ContinuousClock.now
            let outcome = await attempt()
            guard !closed, !Task.isCancelled else { return }
            if restartRequested {
                restartRequested = false
                backoff = Backoff()
                continue
            }
            switch outcome {
            case .again:
                backoff = Backoff()
            case let .stop(final):
                log.info("channel stopped: \(String(describing: final), privacy: .public)")
                isStopped = true
                setState(final)
                return
            case .retry, .wait:
                // Stable for a minute: the next failure starts the backoff over.
                if ContinuousClock.now - started > .seconds(60) { backoff = Backoff() }
                if case let .wait(shown) = outcome {
                    setState(shown)
                } else if case let .retry(reason) = outcome {
                    // The relay said the computer is off: keep saying so rather than "can't reach".
                    if case .hostOffline = state {} else { setState(.failed(reason)) }
                }
                let deadline = ContinuousClock.now + backoff.next()
                while !closed, !restartRequested, ContinuousClock.now < deadline {
                    await stateChange(until: deadline)
                }
                restartRequested = false
            }
        }
    }

    private func attempt() async -> Outcome {
        let directURLs = computer.urls.compactMap(Self.channelURL)
        if !directURLs.isEmpty {
            switch await raceDirect(directURLs) {
            case let .connected(socket, keys): return await serve(socket, route: .direct, keys: keys)
            case let .outcome(end): return end
            case let .rejected(end):
                // A plaintext `reject` isn't signed: whatever answers at a saved LAN address can
                // send one. With a relay, ask the pinned computer through it; without one, only a
                // pairing (short-lived, user-driven) ends on it, a saved computer keeps retrying.
                if computer.cloud != nil { break }
                if pairingCode != nil {
                    if case .stop(.failed(HostError.upgradeRequired.localizedDescription)) = end { return upgrade() }
                    return end
                }
                if case let .stop(shown) = end { return .wait(shown) }
                return end
            case .none: break
            }
        }
        guard let cloud = computer.cloud else { return .retry(HostError.unreachable.localizedDescription) }
        let socket: any ChannelSocket
        do {
            socket = try await dial(.relay(relayRequest(cloud)))
        } catch {
            return .retry(HostError.unreachable.localizedDescription)
        }
        return await serve(socket, route: .relay, keys: nil)
    }

    /// `http://h:p` → `ws://h:p/channel?v=1`.
    static func channelURL(_ base: String) -> URL? {
        guard var comps = URLComponents(string: base), let scheme = comps.scheme else { return nil }
        comps.scheme = scheme == "https" ? "wss" : "ws"
        comps.path = "/channel"
        comps.query = "v=1"
        return comps.url
    }

    /// `GET {cloud}/v1/relay/device/{computerId}?v=1[&pair=<offerId>]`, signed with the device key.
    private func relayRequest(_ cloud: URL) throws -> URLRequest {
        var comps = URLComponents(url: cloud.appending(path: "v1/relay/device/\(computer.id)"), resolvingAgainstBaseURL: false)
        var query = "v=1"
        if let pairingCode, let code = Data(base64URL: pairingCode) { query += "&pair=\(RelayCrypto.offerId(code: code))" }
        comps?.percentEncodedQuery = query
        guard let url = comps?.url else { throw HostError.unreachable }
        let header = try identity.signatureHeader(method: "GET", authority: RelayCrypto.authority(of: url),
                                                  pathAndQuery: RelayCrypto.pathAndQuery(of: url), body: Data())
        comps?.scheme = url.scheme == "http" ? "ws" : "wss"
        var req = URLRequest(url: comps?.url ?? url, timeoutInterval: 15)
        req.setValue(header, forHTTPHeaderField: "Codync-Sig")
        return req
    }

    private enum DirectResult: Sendable {
        case connected(any ChannelSocket, RelayCrypto.ChannelKeys)
        /// Authenticated (the pinned key signed it): the identity changed.
        case outcome(Outcome)
        /// An unauthenticated `reject` from some address.
        case rejected(Outcome)
        case none
    }

    /// Every direct candidate in parallel; the first finished handshake within the budget wins.
    private func raceDirect(_ urls: [URL]) async -> DirectResult {
        let computer = computer
        let identity = identity
        let pair = pairingCode != nil
        let dial = dial
        let budget = directBudget
        enum Finish: Sendable { case attempt(DirectResult), timeout }
        return await withTaskGroup(of: Finish.self) { group in
            for url in urls {
                group.addTask { .attempt(await Self.directHandshake(url, computer: computer, identity: identity, pair: pair, dial: dial)) }
            }
            group.addTask {
                try? await Task.sleep(for: budget)
                return .timeout
            }
            var winner = DirectResult.none
            var rejection: Outcome?
            var decided = false
            var failed = 0
            for await finish in group {
                guard !decided else {
                    // A slower address that connected anyway: not needed.
                    if case let .attempt(.connected(socket, _)) = finish { socket.close(code: 1000) }
                    continue
                }
                switch finish {
                case .timeout:
                    decided = true
                case let .attempt(result):
                    // A reject only fails that address; another may be the real computer.
                    if case let .rejected(o) = result, rejection == nil { rejection = o }
                    switch result {
                    case .none, .rejected:
                        failed += 1
                        decided = failed == urls.count
                    default:
                        winner = result
                        decided = true
                    }
                }
                if decided { group.cancelAll() }
            }
            if case .none = winner, let rejection { return .rejected(rejection) }
            return winner
        }
    }

    private static func directHandshake(_ url: URL, computer: Computer, identity: DeviceIdentity, pair: Bool, dial: Dialer) async -> DirectResult {
        let socket: any ChannelSocket
        do { socket = try await dial(.direct(url)) } catch { return .none }
        return await withTaskCancellationHandler {
            do {
                let hs = try RelayCrypto.Handshake(computer: computer, deviceKey: identity.deviceKey)
                try await socket.send(try Self.helloMessage(hs, identity: identity, pair: pair))
                while true {
                    let wire = try Self.decodeWire(try await socket.receive())
                    switch wire.t {
                    case "welcome":
                        guard let ek = wire.ek.flatMap(Data.init(base64URL:)), let sig = wire.sig.flatMap(Data.init(base64URL:)) else {
                            socket.close(code: 4002)
                            return .none
                        }
                        do {
                            return .connected(socket, try hs.finish(ekH: ek, sig: sig))
                        } catch {
                            socket.close(code: 4001)
                            return .outcome(.stop(.unauthorized(identityChanged)))
                        }
                    case "reject":
                        socket.close(code: 1000)
                        return Self.rejection(wire).map { .rejected($0) } ?? .none
                    default:
                        continue
                    }
                }
            } catch {
                socket.close(code: 1000)
                return .none
            }
        } onCancel: {
            socket.close(code: 1000)
        }
    }

    /// What a `reject` means, or nil when a plain retry may succeed (rate limits, clock skew).
    private static func rejection(_ wire: Wire) -> Outcome? {
        let message = wire.message.flatMap { $0.isEmpty ? nil : $0 }
        switch wire.code {
        case "unauthorized", "revoked":
            return .stop(.unauthorized(message ?? "This device isn't allowed on that computer anymore."))
        case "leaseExpired":
            // The computer couldn't confirm the account's approval lately; it may again.
            return .wait(.unauthorized(message ?? "Your computer couldn't confirm this device's access. Retrying."))
        case "unsupportedVersion":
            return .stop(.failed(HostError.upgradeRequired.localizedDescription))
        case "pairingClosed":
            return .stop(.failed(message ?? "This pairing code expired. Show a new one on the computer."))
        default:
            return nil
        }
    }

    /// Runs one socket until it ends.
    private func serve(_ socket: any ChannelSocket, route: HostRoute, keys: RelayCrypto.ChannelKeys?) async -> Outcome {
        let (outbox, sink) = AsyncStream<String>.makeStream()
        linkGeneration += 1
        let generation = linkGeneration
        link = Link(socket: socket, route: route, generation: generation, outbox: sink)
        // One writer, so frames leave in counter order and a message's chunks never interleave.
        let writer = Task {
            for await text in outbox {
                do { try await socket.send(text) } catch { socket.close(code: 1011); return }
            }
        }
        let heartbeat = Task { await self.heartbeat(generation, route: route) }
        if let keys { install(keys, route: route) }
        if closed || restartRequested { socket.close(code: 1000) }

        var end: Outcome
        do {
            while true {
                let text = try await socket.receive()
                if let outcome = handle(text, generation: generation) {
                    end = outcome
                    socket.close(code: 1000)
                    break
                }
            }
        } catch let closedError as SocketClosed {
            end = outcome(closeCode: closedError.code)
        } catch let refused as SocketRefused {
            end = outcome(httpStatus: refused.status)
        } catch {
            end = .retry(HostError.unreachable.localizedDescription)
        }
        writer.cancel()
        heartbeat.cancel()
        sink.finish()
        link = nil
        dropChannel(HostError.unreachable)
        dropMailbox()
        return end
    }

    private func outcome(closeCode code: Int) -> Outcome {
        switch code {
        case 4001, 4003:
            .stop(.unauthorized("This device isn't allowed on \(computer.name) anymore."))
        case 4410:
            .stop(.failed("This pairing code expired. Show a new one on the computer."))
        case 4400:
            upgrade()
        case 4100:
            // Paired: pairing mode is done; otherwise reconnect normally.
            pairingCode == nil ? .again : .stop(.failed("Paired."))
        case 4011:
            .again
        default:
            .retry(HostError.unreachable.localizedDescription)
        }
    }

    private func outcome(httpStatus status: Int) -> Outcome {
        switch status {
        case 403:
            refusals += 1
            return refusals <= 3
                ? .retry(HostError.unreachable.localizedDescription)
                : .stop(.unauthorized("This device isn't allowed on \(computer.name) anymore."))
        case 426: return upgrade()
        default: return .retry(HostError.unreachable.localizedDescription)
        }
    }

    private func upgrade() -> Outcome {
        needsUpgrade = true
        return .stop(.failed(HostError.upgradeRequired.localizedDescription))
    }

    private func heartbeat(_ generation: Int, route: HostRoute) async {
        // Direct: the host pings every 15 s; relay: we ping the DO every 30 s.
        let interval: Duration = route == .direct ? .seconds(15) : .seconds(30)
        let limit: Duration = route == .direct ? .seconds(45) : .seconds(75)
        while !Task.isCancelled {
            try? await Task.sleep(for: interval)
            guard !Task.isCancelled, let link, link.generation == generation else { return }
            if ContinuousClock.now - link.lastReceived > limit {
                log.info("channel silent, closing")
                link.socket.close(code: 1011)
                return
            }
            let socket = link.socket
            Task {
                try? await socket.ping()
                if route == .direct { self.touch(generation) }
            }
        }
    }

    private func touch(_ generation: Int) {
        if link?.generation == generation { link?.lastReceived = .now }
    }

    // MARK: incoming

    struct Wire: Decodable {
        var t: String
        var ek: String?
        var sig: String?
        var code: String?
        var message: String?
        var c: UInt64?
        var d: String?
        var online: Bool?
        var lastSeenAt: Int64?
        var nonce: String?
        var state: String?
        var items: [QueuedItem]?
    }

    static func decodeWire(_ text: String) throws -> Wire {
        try JSONDecoder().decode(Wire.self, from: Data(text.utf8))
    }

    static func helloMessage(_ hs: RelayCrypto.Handshake, identity: DeviceIdentity, pair: Bool) throws -> String {
        struct Hello: Encodable { var t = "hello"; var v = 1; var dk: String; var ek: String; var n: String; var sig: String; var pair: Bool? }
        let hello = Hello(dk: identity.publicKey, ek: hs.ekD.base64URL, n: hs.n.base64URL,
                          sig: try identity.sign(hs.signInput).base64URL, pair: pair ? true : nil)
        return String(decoding: try JSONEncoder().encode(hello), as: UTF8.self)
    }

    /// Handles one socket message; returns an outcome when the socket must end.
    private func handle(_ text: String, generation: Int) -> Outcome? {
        guard link?.generation == generation else { return nil }
        link?.lastReceived = .now
        guard let wire = try? Self.decodeWire(text) else { return nil }
        switch wire.t {
        case "presence":
            if wire.online == true {
                // The host (re)joined: any previous channel is gone; start a fresh handshake.
                dropChannel(HostError.unreachable)
                return startHandshake(generation)
            }
            lastSeen = wire.lastSeenAt.map(Date.init(milliseconds:))
            dropChannel(HostError.computerOffline(lastSeen: lastSeen))
            link?.handshake = nil
            link?.sealer = nil
            link?.opener = nil
            setState(.hostOffline(lastSeen: lastSeen))
        case "welcome":
            guard let hs = link?.handshake, let route = link?.route,
                  let ek = wire.ek.flatMap(Data.init(base64URL:)), let sig = wire.sig.flatMap(Data.init(base64URL:)) else {
                return .retry(HostError.unreachable.localizedDescription)
            }
            link?.handshake = nil
            do {
                install(try hs.finish(ekH: ek, sig: sig), route: route)
            } catch {
                return .stop(.unauthorized(Self.identityChanged))
            }
        case "reject":
            if wire.code == "unsupportedVersion" { return upgrade() }
            return Self.rejection(wire) ?? .retry(wire.message ?? HostError.unreachable.localizedDescription)
        case "f":
            guard var opener = link?.opener, let c = wire.c, let d = wire.d.flatMap(Data.init(base64URL:)) else {
                link?.socket.close(code: 4002)
                return .retry("Protocol error")
            }
            do {
                let message = try opener.open(c: c, d: d)
                link?.opener = opener
                if let message { handleInner(message) }
            } catch RelayCryptoError.tooLarge {
                link?.socket.close(code: 4013)
                return .retry("Message too large")
            } catch {
                link?.socket.close(code: 4002)
                return .retry("Protocol error")
            }
        case "mbox.ok":
            if let nonce = wire.nonce { puts.removeValue(forKey: nonce)?.resume() }
        case "mbox.err":
            if let nonce = wire.nonce {
                puts.removeValue(forKey: nonce)?.resume(throwing: wire.code == "hostOnline" ? MailboxError.hostOnline : MailboxError.rejected(wire.code ?? "invalid"))
            }
        case "mbox.cancelled":
            if let nonce = wire.nonce { cancels.removeValue(forKey: nonce)?.resume(returning: .cancelled) }
        case "mbox.gone":
            if let nonce = wire.nonce { cancels.removeValue(forKey: nonce)?.resume(returning: wire.state == "delivering" ? .delivering : .unknown) }
        case "mbox.items":
            if let key = lists.keys.first { lists.removeValue(forKey: key)?.resume(returning: wire.items ?? []) }
        case "mbox.delivered":
            if let nonce = wire.nonce { broadcast(.delivered(nonce: nonce)) }
        case "mbox.failed":
            if let nonce = wire.nonce { broadcast(.failed(nonce: nonce, code: wire.code ?? "invalid")) }
        case "mbox.expired":
            if let nonce = wire.nonce { broadcast(.expired(nonce: nonce)) }
        default:
            break
        }
        return nil
    }

    private func startHandshake(_ generation: Int) -> Outcome? {
        guard let link else { return nil }
        do {
            let hs = try RelayCrypto.Handshake(computer: computer, deviceKey: identity.deviceKey)
            self.link?.handshake = hs
            self.link?.sealer = nil
            self.link?.opener = nil
            if case .ready = state { setState(.connecting) }
            if case .hostOffline = state { setState(.connecting) }
            link.outbox.yield(try Self.helloMessage(hs, identity: identity, pair: pairingCode != nil))
        } catch {
            return .stop(.unauthorized(Self.identityChanged))
        }
        // No welcome within 10 s: give up on this socket.
        Task {
            try? await Task.sleep(for: .seconds(10))
            self.handshakeDeadline(generation)
        }
        return nil
    }

    private func handshakeDeadline(_ generation: Int) {
        guard let link, link.generation == generation, link.handshake != nil else { return }
        link.socket.close(code: 1000)
    }

    private func install(_ keys: RelayCrypto.ChannelKeys, route: HostRoute) {
        link?.sealer = RelayCrypto.FrameSealer(key: keys.d2h)
        link?.opener = RelayCrypto.FrameOpener(key: keys.h2d)
        refusals = 0
        setState(.ready(route))
        if pairingCode == nil {
            Task { await self.refreshComputer() }
        }
    }

    private func broadcast(_ event: MailboxEvent) {
        for c in mailboxObservers.values { c.yield(event) }
    }

    /// Inner RPC (§6.5): `ok`/`err` answer calls, `ev`/`end`/`err` feed streams.
    private func handleInner(_ message: Data) {
        guard let obj = try? JSONSerialization.jsonObject(with: message) as? [String: Any],
              let rawId = obj["id"] as? NSNumber else { return }
        let id = rawId.uint32Value
        if let ev = obj["ev"] {
            if let data = try? JSONSerialization.data(withJSONObject: ev, options: .fragmentsAllowed) { streams[id]?.yield(data) }
        } else if obj["end"] as? Bool == true {
            streams.removeValue(forKey: id)?.finish()
            streamIds = streamIds.filter { $0.value != id }
        } else if let err = obj["err"] as? [String: Any] {
            let status = (err["status"] as? NSNumber)?.intValue ?? 500
            let error = HostError.http(status, err["message"] as? String ?? "Host error \(status)")
            if let call = calls.removeValue(forKey: id) {
                call.resume(throwing: error)
            } else if let stream = streams.removeValue(forKey: id) {
                stream.finish(throwing: error)
                streamIds = streamIds.filter { $0.value != id }
            }
        } else if obj.keys.contains("ok") {
            let data = (try? JSONSerialization.data(withJSONObject: obj["ok"] ?? NSNull(), options: .fragmentsAllowed)) ?? Data("null".utf8)
            calls.removeValue(forKey: id)?.resume(returning: data)
        }
    }

    /// Ends everything riding on the channel (not the mailbox, which lives on the relay socket).
    private func dropChannel(_ error: Error) {
        let c = calls
        let s = streams
        calls = [:]
        streams = [:]
        streamIds = [:]
        for call in c.values { call.resume(throwing: error) }
        // Route change or re-handshake: the store resubscribes from its own rev.
        for stream in s.values { stream.finish(throwing: HostError.unreachable) }
    }

    private func dropMailbox() {
        let p = puts, x = cancels, l = lists
        puts = [:]
        cancels = [:]
        lists = [:]
        for put in p.values { put.resume(throwing: HostError.unreachable) }
        for cancel in x.values { cancel.resume(returning: .unknown) }
        for list in l.values { list.resume(returning: []) }
    }

    // MARK: inner hello (§7.5 step 5)

    private func refreshComputer() async {
        guard let data = try? await perform("hello", body: Data(), timeout: 20),
              let hello = try? JSONDecoder().decode(Hello.self, from: data) else { return }
        merge(hello)
    }

    func merge(_ hello: Hello) {
        // The welcome signature already proved the key; a hello naming another computer is ignored.
        guard hello.computerId == nil || hello.computerId == computer.id,
              hello.signKey == nil || hello.signKey == computer.signKey else {
            log.error("hello names another computer")
            return
        }
        var c = computer
        if !hello.name.isEmpty { c.name = hello.name }
        if let bk = hello.boxKey, Data(base64URL: bk)?.count == 32 { c.boxKey = bk }
        if let urls = hello.urls { c.urls = urls.filter(Pairing.isDirectURL) }
        c.cloud = hello.cloud.flatMap(URL.init(string:)).flatMap { Pairing.isCloudURL($0) ? $0 : nil }
        if let device = hello.device { c.device = device }
        guard c != computer else { return }
        computer = c
        for o in computerObservers.values { o.yield(c) }
    }

    // MARK: calls and streams

    public nonisolated func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await perform(method, body: body, timeout: timeout)
    }

    private func nextId() -> UInt32 {
        nextRequestId = nextRequestId == .max ? 1 : nextRequestId + 1
        return nextRequestId
    }

    /// Waits (bounded) while connecting; throws what the link state means otherwise.
    private func ready(within limit: TimeInterval) async throws {
        let deadline = ContinuousClock.now + .seconds(max(limit, 0))
        while true {
            if closed { throw HostError.unreachable }
            switch state {
            case .ready where link?.sealer != nil: return
            case .ready, .connecting:
                guard ContinuousClock.now < deadline else { throw HostError.unreachable }
                await stateChange(until: deadline)
            case let .hostOffline(lastSeen): throw HostError.computerOffline(lastSeen: lastSeen)
            case let .unauthorized(message): throw HostError.unauthorized(message)
            case .failed: throw needsUpgrade ? HostError.upgradeRequired : HostError.unreachable
            }
        }
    }

    private func perform(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await ready(within: timeout)
        let b: Any = body.isEmpty ? NSNull() : try JSONSerialization.jsonObject(with: body, options: .fragmentsAllowed)
        let id = nextId()
        let payload = try JSONSerialization.data(withJSONObject: ["id": id, "m": method, "b": b] as [String: Any])
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<Data, Error>) in
                calls[id] = c
                do {
                    try sendInner(payload)
                } catch {
                    calls.removeValue(forKey: id)?.resume(throwing: error)
                    return
                }
                Task {
                    try? await Task.sleep(for: .seconds(timeout))
                    self.abandon(id, error: HostError.unreachable)
                }
            }
        } onCancel: {
            Task { await self.abandon(id, error: CancellationError()) }
        }
    }

    /// Timed out or cancelled: the host drops the answer (an already started mutation still runs).
    private func abandon(_ id: UInt32, error: Error) {
        guard let c = calls.removeValue(forKey: id) else { return }
        c.resume(throwing: error)
        try? sendInner(JSONSerialization.data(withJSONObject: ["id": id, "cancel": true]))
    }

    private func sendInner(_ message: Data) throws {
        guard var sealer = link?.sealer, let link else { throw HostError.unreachable }
        let frames = try sealer.seal(message)
        self.link?.sealer = sealer
        for frame in frames {
            link.outbox.yield(#"{"t":"f","c":\#(frame.c),"d":"\#(frame.d.base64URL)"}"#)
        }
        // Counters must never wrap: replace the channel (§3.5).
        if sealer.exhausted {
            restartRequested = true
            link.socket.close(code: 4011)
        }
    }

    public nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            let key = UUID()
            let task = Task { await self.openStream(request, key: key, continuation) }
            continuation.onTermination = { _ in
                task.cancel()
                Task { await self.closeStream(key) }
            }
        }
    }

    private func openStream(_ request: HostStreamRequest, key: UUID, _ continuation: AsyncThrowingStream<Data, Error>.Continuation) async {
        do {
            try await ready(within: 10)
            // The consumer left while we waited: `closeStream` already ran and found nothing to stop.
            try Task.checkCancellation()
            let id = nextId()
            let sub: [String: Any] = switch request {
            case let .events(since, client): ["id": id, "sub": "events", "b": ["since": since, "client": client]]
            case let .term(term): ["id": id, "sub": "term", "b": ["term": term]]
            }
            streams[id] = continuation
            streamIds[key] = id
            try sendInner(JSONSerialization.data(withJSONObject: sub))
        } catch {
            continuation.finish(throwing: error)
        }
    }

    /// The consumer went away: stop the host's stream.
    private func closeStream(_ key: UUID) {
        guard let id = streamIds.removeValue(forKey: key), streams.removeValue(forKey: id) != nil else { return }
        try? sendInner(JSONSerialization.data(withJSONObject: ["id": id, "cancel": true]))
    }

    // MARK: mailbox (§6.4, §7.4)

    private func relaySocket() throws -> Link {
        guard let link, link.route == .relay else { throw HostError.unreachable }
        return link
    }

    public func enqueue(botId: String, text: String, clientNonce: String) async throws {
        guard pairingCode == nil, let bk = computer.boxKey.flatMap(Data.init(base64URL:)), bk.count == 32 else {
            throw HostError.computerOffline(lastSeen: lastSeen)
        }
        if case let .ready(route) = state, route == .direct { throw MailboxError.hostOnline }
        let link = try relaySocket()
        guard puts[clientNonce] == nil else { throw MailboxError.rejected("busy") }
        struct Send: Encodable { var botId: String; var text: String; var clientNonce: String }
        struct Inner: Encodable { var m = "send"; var b: Send; var ts: Int64 }
        let inner = try JSONEncoder().encode(Inner(b: Send(botId: botId, text: text, clientNonce: clientNonce),
                                                   ts: Int64(Date.now.timeIntervalSince1970 * 1000)))
        // A fresh ephemeral key on every seal, retries included (§6.4 MUST).
        let sealed = try RelayCrypto.sealMailbox(inner, hostBoxKey: bk, computerId: computer.id, deviceKey: identity.deviceKey,
                                                 clientNonce: clientNonce, sign: identity.sign)
        struct Put: Encodable { var t = "mbox.put"; var nonce: String; var d: String }
        let put = String(decoding: try JSONEncoder().encode(Put(nonce: clientNonce, d: sealed.blob.base64URL)), as: UTF8.self)
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
            puts[clientNonce] = c
            link.outbox.yield(put)
            Task {
                try? await Task.sleep(for: .seconds(15))
                self.expirePut(clientNonce)
            }
        }
    }

    private func expirePut(_ nonce: String) {
        puts.removeValue(forKey: nonce)?.resume(throwing: HostError.unreachable)
    }

    public func cancelQueued(clientNonce: String) async -> MailboxCancel {
        guard let link = try? relaySocket(), cancels[clientNonce] == nil else { return .unknown }
        let message = String(decoding: (try? JSONSerialization.data(withJSONObject: ["t": "mbox.cancel", "nonce": clientNonce])) ?? Data(), as: UTF8.self)
        return await withCheckedContinuation { c in
            cancels[clientNonce] = c
            link.outbox.yield(message)
            Task {
                try? await Task.sleep(for: .seconds(10))
                self.expireCancel(clientNonce)
            }
        }
    }

    private func expireCancel(_ nonce: String) {
        cancels.removeValue(forKey: nonce)?.resume(returning: .unknown)
    }

    public func listQueued() async -> [QueuedItem] {
        guard let link = try? relaySocket() else { return [] }
        let key = UUID()
        return await withCheckedContinuation { c in
            lists[key] = c
            link.outbox.yield(#"{"t":"mbox.list"}"#)
            Task {
                try? await Task.sleep(for: .seconds(10))
                self.expireList(key)
            }
        }
    }

    private func expireList(_ key: UUID) {
        lists.removeValue(forKey: key)?.resume(returning: [])
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
