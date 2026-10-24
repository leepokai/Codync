import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Channel")

// MARK: connection loop (§7.5, §7.6)

extension ChannelTransport {
    func run() async {
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
        let route = computer.route ?? .automatic
        // Cloudflare first: the relay, and only if it can't be reached, the direct addresses below.
        if route == .cloudflareFirst, let cloud = computer.cloud,
           let socket = try? await dial(.relay(relayRequest(cloud))) {
            return await serve(socket, route: .relay, keys: nil)
        }
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
        // Already tried first, or ruled out by the user (privacy): no second relay attempt.
        guard let cloud = computer.cloud, route == .automatic else { return .retry(HostError.unreachable.localizedDescription) }
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
    static func rejection(_ wire: Wire) -> Outcome? {
        let message = wire.message.flatMap { $0.isEmpty ? nil : $0 }
        switch wire.code {
        case "unauthorized", "revoked":
            return .stop(.unauthorized(message ?? "This device isn't allowed on that computer anymore."))
        case "leaseExpired":
            // The computer couldn't confirm the account's approval lately; it may again, so this
            // isn't a refusal: the channel keeps retrying.
            return .wait(.failed(message ?? "Your computer couldn't confirm this device's access. Retrying."))
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

    func upgrade() -> Outcome {
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
}
