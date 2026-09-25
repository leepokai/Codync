import CryptoKit
import Foundation
import Testing
@testable import CodyncKit

private let v = Vectors.shared

/// One direction of a fake WebSocket.
actor Pipe {
    private var items: [String] = []
    private var waiters: [CheckedContinuation<String, Error>] = []
    private var closedCode: Int?

    func push(_ s: String) {
        guard closedCode == nil else { return }
        if waiters.isEmpty { items.append(s) } else { waiters.removeFirst().resume(returning: s) }
    }

    func pop() async throws -> String {
        if !items.isEmpty { return items.removeFirst() }
        if let closedCode { throw SocketClosed(code: closedCode) }
        return try await withCheckedThrowingContinuation { waiters.append($0) }
    }

    func close(_ code: Int) {
        guard closedCode == nil else { return }
        closedCode = code
        for w in waiters { w.resume(throwing: SocketClosed(code: code)) }
        waiters = []
    }
}

final class FakeSocket: ChannelSocket {
    let endpoint: Endpoint
    let toDevice = Pipe()
    let toHost = Pipe()

    init(_ endpoint: Endpoint) { self.endpoint = endpoint }

    func send(_ text: String) async throws { await toHost.push(text) }
    func receive() async throws -> String { try await toDevice.pop() }
    func ping() async throws {}
    func close(code: Int) {
        Task {
            await toDevice.close(code)
            await toHost.close(code)
        }
    }
}

private func object(_ text: String) -> [String: Any] {
    (try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]) ?? [:]
}

private func text(_ obj: [String: Any]) -> String {
    String(decoding: (try? JSONSerialization.data(withJSONObject: obj)) ?? Data(), as: UTF8.self)
}

/// The host (or, for DO messages, the relay) on the far end of a fake socket.
struct HostSide {
    let socket: FakeSocket
    var sealer: RelayCrypto.FrameSealer?
    var opener: RelayCrypto.FrameOpener?

    init(_ socket: FakeSocket) { self.socket = socket }

    /// Next message that isn't a relay ping.
    func raw() async throws -> [String: Any] {
        while true {
            let o = object(try await socket.toHost.pop())
            if o["t"] as? String != "ping" { return o }
        }
    }

    func push(_ obj: [String: Any]) async { await socket.toDevice.push(text(obj)) }

    /// Verifies the device's hello and answers with a welcome signed by `hostKey`.
    @discardableResult
    mutating func accept(hostKey: Curve25519.Signing.PrivateKey = v.hostSign) async throws -> [String: Any] {
        let hello = try await raw()
        #expect(hello["t"] as? String == "hello")
        #expect(hello["v"] as? Int == 1)
        let dk = Data(base64URL: hello["dk"] as? String ?? "") ?? Data()
        let ekD = Data(base64URL: hello["ek"] as? String ?? "") ?? Data()
        let n = Data(base64URL: hello["n"] as? String ?? "") ?? Data()
        let cid = RelayCrypto.computerIdRaw(signKey: v.b64("keys", "hostSignPub"))
        let sig = Data(base64URL: hello["sig"] as? String ?? "") ?? Data()
        #expect(try Curve25519.Signing.PublicKey(rawRepresentation: dk)
            .isValidSignature(sig, for: RelayCrypto.hs1Input(cid: cid, dk: dk, ekD: ekD, n: n)))
        let ekH = Curve25519.KeyAgreement.PrivateKey()
        let th = RelayCrypto.transcriptHash(cid: cid, dk: dk, ekD: ekD, n: n, ekH: ekH.publicKey.rawRepresentation)
        await push(["t": "welcome", "v": 1, "ek": ekH.publicKey.rawRepresentation.base64URL, "sig": try hostKey.signature(for: th).base64URL])
        let keys = RelayCrypto.channelKeys(sharedSecret: try RelayCrypto.sharedSecret(ekH, ekD), transcriptHash: th)
        sealer = RelayCrypto.FrameSealer(key: keys.h2d)
        opener = RelayCrypto.FrameOpener(key: keys.d2h)
        return hello
    }

    /// Next inner message from the device.
    mutating func receive() async throws -> [String: Any] {
        while true {
            let f = try await raw()
            guard f["t"] as? String == "f", var o = opener else { continue }
            let c = (f["c"] as? NSNumber)?.uint64Value ?? .max
            let message = try o.open(c: c, d: Data(base64URL: f["d"] as? String ?? "") ?? Data())
            opener = o
            if let message { return (try JSONSerialization.jsonObject(with: message) as? [String: Any]) ?? [:] }
        }
    }

    mutating func send(_ inner: [String: Any]) async throws {
        guard var s = sealer else { return }
        for frame in try s.seal(try JSONSerialization.data(withJSONObject: inner)) {
            await push(["t": "f", "c": frame.c, "d": frame.d.base64URL])
        }
        sealer = s
    }
}

/// A dialer handing each new fake socket to the test.
private func fakeDialer() -> (Dialer, AsyncStream<FakeSocket>) {
    let (sockets, sink) = AsyncStream<FakeSocket>.makeStream()
    return ({ endpoint in
        let s = FakeSocket(endpoint)
        sink.yield(s)
        return s
    }, sockets)
}

private func helloBody() -> [String: Any] { [
    "hostId": "h1", "name": "Studio", "version": "3.0.0", "os": "macos", "backends": [], "rev": 0,
    "computerId": v.str("keys", "computerId"), "signKey": v.str("keys", "hostSignPub"), "boxKey": v.str("keys", "hostBoxPub"),
    "urls": ["http://10.0.0.2:19222"], "cloud": "https://cloud.example.dev", "device": "macstudio", "protocol": 1,
] }

private func waitFor(_ states: AsyncStream<LinkState>, _ match: (LinkState) -> Bool) async -> LinkState? {
    for await s in states where match(s) { return s }
    return nil
}

@Test(.timeLimit(.minutes(1))) func directChannelMultiplexesCallsAndStreams() async throws {
    let (dial, sockets) = fakeDialer()
    var computer = v.computer
    computer.boxKey = nil
    computer.urls = ["http://127.0.0.1:1"]
    let t = ChannelTransport(computer: computer, identity: v.identity, pairingCode: nil, dial: dial, watchesNetwork: false)
    let a = t.states()
    let b = t.states()
    let updates = t.computerUpdates()
    await t.start()
    var socketIterator = sockets.makeAsyncIterator()
    let socket = try #require(await socketIterator.next())
    guard case let .direct(url) = socket.endpoint else { Issue.record("not direct"); return }
    #expect(url.absoluteString == "ws://127.0.0.1:1/channel?v=1")
    var host = HostSide(socket)
    let h = try await host.accept()
    #expect(h["pair"] == nil)

    // Every consumer sees the same state.
    #expect(await waitFor(a) { $0 == .ready(.direct) } == .ready(.direct))
    #expect(await waitFor(b) { $0 == .ready(.direct) } == .ready(.direct))

    // The transport asks for hello by itself and merges what it learns.
    let first = try await host.receive()
    #expect(first["m"] as? String == "hello")
    try await host.send(["id": first["id"] ?? 0, "ok": helloBody()])
    var updated = updates.makeAsyncIterator()
    let merged = try #require(await updated.next())
    #expect(merged.boxKey == v.str("keys", "hostBoxPub"))
    #expect(merged.urls == ["http://10.0.0.2:19222"])
    #expect(merged.cloud?.absoluteString == "https://cloud.example.dev")
    #expect(merged.device == "macstudio" && merged.name == "Studio")

    // Two calls in flight, answered out of order.
    async let usage = t.call("usage", body: Data(#"{"refresh":false}"#.utf8), timeout: 10)
    async let sync = t.call("sync", body: Data(#"{"since":7}"#.utf8), timeout: 10)
    let r1 = try await host.receive()
    let r2 = try await host.receive()
    #expect(r1["id"] as? Int != r2["id"] as? Int)
    for r in [r2, r1] { try await host.send(["id": r["id"] ?? 0, "ok": ["method": r["m"] ?? ""]]) }
    #expect(object(String(decoding: try await usage, as: UTF8.self))["method"] as? String == "usage")
    #expect(object(String(decoding: try await sync, as: UTF8.self))["method"] as? String == "sync")
    let syncBody = [r1, r2].first { $0["m"] as? String == "sync" }?["b"] as? [String: Any]
    #expect(syncBody?["since"] as? Int == 7)

    // Errors keep their HTTP status.
    let denied = Task { try await t.call("setScreenEnabled", body: Data(#"{"enabled":true}"#.utf8), timeout: 10) }
    let r3 = try await host.receive()
    try await host.send(["id": r3["id"] ?? 0, "err": ["status": 403, "message": "Only on the computer."]])
    await #expect(throws: HostError.http(403, "Only on the computer.")) { _ = try await denied.value }

    // A stream yields events until its consumer stops; the host is then told to cancel.
    let reader = Task {
        var got: [Data] = []
        for try await d in t.stream(.events(since: 5, client: "ios")) {
            got.append(d)
            if got.count == 2 { break }
        }
        return got
    }
    let sub = try await host.receive()
    #expect(sub["sub"] as? String == "events")
    #expect((sub["b"] as? [String: Any])?["since"] as? Int == 5)
    #expect((sub["b"] as? [String: Any])?["client"] as? String == "ios")
    try await host.send(["id": sub["id"] ?? 0, "ev": ["type": "hello", "hostId": "h1", "rev": 9]])
    try await host.send(["id": sub["id"] ?? 0, "ev": ["type": "resync"]])
    let got = try await reader.value
    #expect(got.count == 2 && object(String(decoding: got[1], as: UTF8.self))["type"] as? String == "resync")
    let cancel = try await host.receive()
    #expect(cancel["id"] as? Int == sub["id"] as? Int && cancel["cancel"] as? Bool == true)

    // Revoked: no retry.
    await socket.toDevice.close(4003)
    guard case .unauthorized = await waitFor(t.states(), { if case .unauthorized = $0 { true } else { false } }) else {
        Issue.record("expected unauthorized"); return
    }
    await #expect(throws: HostError.self) { _ = try await t.call("hello", body: Data(), timeout: 1) }
    await t.shutdown()
}

@Test(.timeLimit(.minutes(1))) func changedHostIdentityIsNotRetried() async throws {
    let (dial, sockets) = fakeDialer()
    var computer = v.computer
    computer.urls = ["http://127.0.0.1:1"]
    let t = ChannelTransport(computer: computer, identity: v.identity, pairingCode: nil, dial: dial, watchesNetwork: false)
    await t.start()
    var it = sockets.makeAsyncIterator()
    var host = HostSide(try #require(await it.next()))
    try await host.accept(hostKey: Curve25519.Signing.PrivateKey())
    #expect(await t.settled(within: .seconds(5)) == .unauthorized(ChannelTransport.identityChanged))
    await t.shutdown()
}

@Test(.timeLimit(.minutes(1))) func directRejectFallsBackToTheRelay() async throws {
    let (dial, sockets) = fakeDialer()
    var computer = v.computer
    computer.urls = ["http://192.168.1.20:19222"]
    computer.cloud = URL(string: "https://cloud.example.dev")
    let t = ChannelTransport(computer: computer, identity: v.identity, pairingCode: nil, dial: dial, watchesNetwork: false)
    await t.start()
    var it = sockets.makeAsyncIterator()
    // Some other host now answers at the saved LAN address; its reject isn't signed.
    let stranger = HostSide(try #require(await it.next()))
    _ = try await stranger.raw()
    await stranger.push(["t": "reject", "code": "unauthorized"])
    let socket = try #require(await it.next())
    guard case .relay = socket.endpoint else { Issue.record("expected the relay"); return }
    var relay = HostSide(socket)
    await relay.push(["t": "presence", "online": true, "lastSeenAt": NSNull()])
    try await relay.accept()
    #expect(await t.settled(within: .seconds(5)) == .ready(.relay))
    #expect(await !t.isStopped)
    await t.shutdown()
}

@Test(.timeLimit(.minutes(1))) func relayPresenceAndMailbox() async throws {
    let (dial, sockets) = fakeDialer()
    var computer = v.computer
    computer.urls = []
    computer.cloud = URL(string: "https://cloud.example.dev")
    let t = ChannelTransport(computer: computer, identity: v.identity, pairingCode: nil, dial: dial, watchesNetwork: false)
    let mailbox = t.mailboxEvents()
    await t.start()
    var it = sockets.makeAsyncIterator()
    let socket = try #require(await it.next())
    guard case let .relay(request) = socket.endpoint else { Issue.record("not relay"); return }
    #expect(request.url?.absoluteString == "wss://cloud.example.dev/v1/relay/device/\(computer.id)?v=1")
    let sig = request.value(forHTTPHeaderField: "Codync-Sig") ?? ""
    #expect(sig.hasPrefix("v=1,kid=\(v.identity.publicKey),ts="))
    var relay = HostSide(socket)

    await relay.push(["t": "presence", "online": false, "lastSeenAt": 1_790_000_000_000])
    let seen = Date(milliseconds: 1_790_000_000_000)
    #expect(await t.settled(within: .seconds(5)) == .hostOffline(lastSeen: seen))
    await #expect(throws: HostError.computerOffline(lastSeen: seen)) { _ = try await t.call("hello", body: Data(), timeout: 1) }

    // Queue a message: it goes out sealed for the host's box key.
    let nonce = "3F2504E0-4F89-11D3-9A0C-0305E82C3301"
    async let queued: Void = t.enqueue(botId: "b1", text: "hi", clientNonce: nonce)
    let put = try await relay.raw()
    #expect(put["t"] as? String == "mbox.put" && put["nonce"] as? String == nonce)
    let blob = try #require(Data(base64URL: put["d"] as? String ?? ""))
    let epk = blob.prefix(32), mboxSig = blob.dropFirst(32).prefix(64), ct = blob.dropFirst(96)
    let cid = RelayCrypto.computerIdRaw(signKey: v.b64("keys", "hostSignPub"))
    #expect(v.deviceSign.publicKey.isValidSignature(mboxSig, for: RelayCrypto.label("codync/mbox/v1") + cid + epk + ct))
    let key = RelayCrypto.mailboxKey(sharedSecret: try RelayCrypto.sharedSecret(v.hostBox, Data(epk)), cid: cid,
                                     dk: v.b64("keys", "deviceSignPub"), epk: Data(epk))
    let box = try ChaChaPoly.SealedBox(nonce: ChaChaPoly.Nonce(data: Data(count: 12)), ciphertext: ct.dropLast(16), tag: ct.suffix(16))
    let inner = object(String(decoding: try ChaChaPoly.open(box, using: key, authenticating: v.b64("keys", "deviceSignPub") + Data(nonce.utf8)), as: UTF8.self))
    #expect(inner["m"] as? String == "send")
    #expect((inner["b"] as? [String: Any])?["clientNonce"] as? String == nonce)
    #expect((inner["b"] as? [String: Any])?["text"] as? String == "hi")
    await relay.push(["t": "mbox.ok", "nonce": nonce, "state": "queued"])
    try await queued

    // The relay refusing says why.
    let refused = Task { try await t.enqueue(botId: "b1", text: "again", clientNonce: "n2") }
    _ = try await relay.raw()
    await relay.push(["t": "mbox.err", "nonce": "n2", "code": "full"])
    await #expect(throws: MailboxError.rejected("full")) { try await refused.value }

    async let listed = t.listQueued()
    #expect(try await relay.raw()["t"] as? String == "mbox.list")
    await relay.push(["t": "mbox.items", "items": [["nonce": nonce, "exp": 1, "state": "queued"]]])
    #expect(await listed.map(\.nonce) == [nonce])

    async let cancelled = t.cancelQueued(clientNonce: nonce)
    #expect(try await relay.raw()["t"] as? String == "mbox.cancel")
    await relay.push(["t": "mbox.cancelled", "nonce": nonce])
    #expect(await cancelled == .cancelled)

    async let gone = t.cancelQueued(clientNonce: "n3")
    _ = try await relay.raw()
    await relay.push(["t": "mbox.gone", "nonce": "n3", "state": "delivering"])
    #expect(await gone == .delivering)

    await relay.push(["t": "mbox.expired", "nonce": "n4"])
    var events = mailbox.makeAsyncIterator()
    #expect(await events.next() == .expired(nonce: "n4"))

    // The host comes online: a fresh handshake through the relay.
    await relay.push(["t": "presence", "online": true, "lastSeenAt": nil as Int? as Any])
    try await relay.accept()
    #expect(await waitFor(t.states()) { $0 == .ready(.relay) } == .ready(.relay))
    await #expect(throws: MailboxError.self) {
        async let online: Void = t.enqueue(botId: "b1", text: "x", clientNonce: "n5")
        _ = try await relay.raw()
        await relay.push(["t": "mbox.err", "nonce": "n5", "code": "hostOnline"])
        try await online
    }
    await t.shutdown()
}

@Test(.timeLimit(.minutes(1))) func pairingSendsOnlyPairThenStopsOn4100() async throws {
    let (dial, sockets) = fakeDialer()
    var computer = v.computer
    computer.urls = ["http://127.0.0.1:1"]
    let code = v.str("pairing", "code")
    let t = ChannelTransport(computer: computer, identity: v.identity, pairingCode: code, dial: dial, watchesNetwork: false)
    await t.start()
    var it = sockets.makeAsyncIterator()
    let socket = try #require(await it.next())
    var host = HostSide(socket)
    let h = try await host.accept()
    #expect(h["pair"] as? Bool == true)
    async let paired = t.call("pair", body: Data(#"{"code":"\#(code)","name":"iPhone","platform":"ios"}"#.utf8), timeout: 10)
    let first = try await host.receive()
    #expect(first["m"] as? String == "pair")
    #expect((first["b"] as? [String: Any])?["code"] as? String == code)
    try await host.send(["id": first["id"] ?? 0, "ok": ["computerId": computer.id, "name": "Studio"]])
    #expect(object(String(decoding: try await paired, as: UTF8.self))["name"] as? String == "Studio")
    await socket.toDevice.close(4100)
    #expect(await waitFor(t.states()) { if case .failed = $0 { true } else { false } } == .failed("Paired."))
    await t.shutdown()
}
