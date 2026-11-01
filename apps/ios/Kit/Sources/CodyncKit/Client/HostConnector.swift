import Foundation
import os

/// Opens channels to computers (§7.5) and pairs new ones from a QR (§4.1).
public enum HostConnector {
    /// Direct first (1.5 s), else the relay. The returned transport stays alive while the
    /// computer is offline (presence, mailbox) and reconnects on its own; `shutdown()` ends it.
    public static func connect(_ computer: Computer, identity: DeviceIdentity) async throws -> ChannelTransport {
        let transport = ChannelTransport(computer: computer, identity: identity)
        await transport.start()
        let state = await transport.settled(within: .seconds(15))
        guard await transport.isStopped else { return transport }
        await transport.shutdown()
        if await transport.needsUpgrade { throw HostError.upgradeRequired }
        if case let .unauthorized(message) = state { throw HostError.unauthorized(message) }
        throw HostError.unreachable
    }

    /// Proves the QR's one-time code inside a channel to the QR's host key, with the account's `certificate`;
    /// the host then authorizes this device and closes with `4100 paired`. A computer in no account asks its
    /// user first, then signs `claim` and returns it (as JSON) for the phone to complete with the cloud.
    public static func pair(_ pairing: Pairing, identity: DeviceIdentity, deviceName: String, platform: String,
                            certificate: DeviceCertificate, claim: CloudClaim?) async throws -> (computer: Computer, claim: Data?) {
        try await pair(pairing, identity: identity, deviceName: deviceName, platform: platform,
                       certificate: certificate, claim: claim, dial: URLSessionSocket.dial)
    }

    static func pair(_ pairing: Pairing, identity: DeviceIdentity, deviceName: String, platform: String,
                     certificate: DeviceCertificate, claim: CloudClaim?, dial: @escaping Dialer) async throws -> (computer: Computer, claim: Data?) {
        let transport = ChannelTransport(computer: pairing.computer, identity: identity, pairingCode: pairing.code, dial: dial)
        await transport.start()
        do {
            struct Body: Encodable {
                var code: String
                var name: String
                var platform: String
                var certificate: DeviceCertificate
                var claim: CloudClaim?
            }
            struct Res: Decodable { var computerId: ComputerID; var name: String? }
            let body = try JSONEncoder().encode(Body(code: pairing.code, name: deviceName, platform: platform, certificate: certificate, claim: claim))
            try await transport.ready(within: 30)
            // The computer may be waiting for its user to let this phone in (up to 2 minutes).
            let data = try await transport.call("pair", body: body, timeout: 150)
            await transport.shutdown()
            let res = try JSONDecoder().decode(Res.self, from: data)
            guard res.computerId == pairing.computer.id else { throw HostError.unauthorized(ChannelTransport.identityChanged) }
            var computer = pairing.computer
            if let name = res.name, !name.isEmpty { computer.name = name }
            let signed = (try JSONSerialization.jsonObject(with: data) as? [String: Any])?["claim"] as? [String: Any]
            return (computer, try signed.map { try JSONSerialization.data(withJSONObject: $0) })
        } catch {
            await transport.shutdown()
            throw await transport.needsUpgrade ? HostError.upgradeRequired : error
        }
    }
}

// MARK: - WebSocket

/// A WebSocket as the channel sees it: text messages, close codes, and a liveness ping.
protocol ChannelSocket: Sendable {
    func send(_ text: String) async throws
    /// Throws `SocketClosed` or `SocketRefused` when the socket ends.
    func receive() async throws -> String
    /// Direct: a WebSocket ping frame, returning on pong. Relay: `{"t":"ping"}` (the DO answers).
    func ping() async throws
    func close(code: Int)
}

struct SocketClosed: Error, Equatable { var code: Int }
/// The upgrade was answered with this HTTP status instead of 101.
struct SocketRefused: Error, Equatable { var status: Int }

enum Endpoint: Sendable {
    case direct(URL)
    case relay(URLRequest)
}

typealias Dialer = @Sendable (Endpoint) async throws -> any ChannelSocket

final class URLSessionSocket: ChannelSocket {
    private let task: URLSessionWebSocketTask
    private let relay: Bool

    static let dial: Dialer = { endpoint in
        switch endpoint {
        case let .direct(url): URLSessionSocket(URLRequest(url: url, timeoutInterval: 10), relay: false)
        case let .relay(request): URLSessionSocket(request, relay: true)
        }
    }

    init(_ request: URLRequest, relay: Bool) {
        task = URLSession.shared.webSocketTask(with: request)
        // Chunks are ≤ 256 KiB before base64 and framing.
        task.maximumMessageSize = 1 << 20
        self.relay = relay
        task.resume()
    }

    func send(_ text: String) async throws {
        do { try await task.send(.string(text)) } catch { throw ended() }
    }

    func receive() async throws -> String {
        let message: URLSessionWebSocketTask.Message
        do { message = try await task.receive() } catch { throw ended() }
        switch message {
        case let .string(s): return s
        case let .data(d): return String(decoding: d, as: UTF8.self)
        @unknown default: throw SocketClosed(code: 1003)
        }
    }

    func ping() async throws {
        if relay { return try await send(#"{"t":"ping"}"#) }
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
            // URLSession can answer a ping twice when the socket closes; a continuation resumes once.
            let waiting = OSAllocatedUnfairLock<CheckedContinuation<Void, Error>?>(initialState: c)
            task.sendPing { error in
                guard let c = waiting.withLock({ $0.take() }) else { return }
                if let error { c.resume(throwing: error) } else { c.resume() }
            }
        }
    }

    func close(code: Int) {
        task.cancel(with: URLSessionWebSocketTask.CloseCode(rawValue: code) ?? .normalClosure, reason: nil)
    }

    private func ended() -> Error {
        if task.closeCode != .invalid { return SocketClosed(code: task.closeCode.rawValue) }
        if let http = task.response as? HTTPURLResponse, http.statusCode != 101 { return SocketRefused(status: http.statusCode) }
        return SocketClosed(code: 1006)
    }
}
