import Foundation

/// How a client reaches a host API: loopback HTTP (the Mac itself, an SSH tunnel), or the
/// end-to-end encrypted channel, direct on the LAN or through the cloud relay.
public enum HostRoute: Sendable, Equatable {
    case loopback, direct, relay
}

public enum HostStreamRequest: Sendable {
    case events(since: Int64, client: String)
    case term(String)
}

public enum LinkState: Sendable, Equatable {
    case connecting
    case ready(HostRoute)
    /// The relay says the computer isn't connected (asleep, off): the mailbox still works.
    case hostOffline(lastSeen: Date?)
    /// Revoked, lease expired, or the computer's identity changed: retrying won't help.
    case unauthorized(String)
    /// Can't reach the computer at all (neither directly nor through the relay).
    case failed(String)
}

public protocol HostTransport: Sendable {
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data
    /// Each element is one event's JSON (what SSE puts after `data:`).
    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error>
    /// A new stream per call: the current state first, then every change.
    func states() -> AsyncStream<LinkState>
}

/// A transport to a remote computer: learns its addresses and keys, and holds messages
/// in the relay mailbox while it is offline. `ChannelTransport` is the real one.
public protocol RemoteTransport: HostTransport {
    func computerUpdates() -> AsyncStream<Computer>
    func enqueue(botId: String, text: String, clientNonce: String) async throws
    func cancelQueued(clientNonce: String) async -> MailboxCancel
    func listQueued() async -> [QueuedItem]
    func mailboxEvents() -> AsyncStream<MailboxEvent>
    /// Stops for good: closes sockets and ends every stream.
    func shutdown() async
}

public enum HostError: LocalizedError, Sendable, Equatable {
    case http(Int, String)
    case unreachable
    case computerOffline(lastSeen: Date?)
    case unauthorized(String)
    case upgradeRequired

    public var errorDescription: String? {
        switch self {
        case .http(401, _): "This device isn't allowed on that computer anymore."
        case let .http(_, message): message
        case .unreachable: "Can't reach your computer. Is it on and connected to the internet?"
        case .computerOffline: "Your computer is offline."
        case let .unauthorized(message): message
        case .upgradeRequired: "Update Codync to connect to this computer."
        }
    }
}

/// Today's HTTP API on loopback: the Mac talking to its own host, or through an SSH tunnel.
public struct LoopbackTransport: HostTransport {
    public let baseURL: URL
    let token: String

    public init(baseURL: URL, token: String) {
        self.baseURL = baseURL
        self.token = token
    }

    private var session: URLSession { .shared }

    public func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        var req = URLRequest(url: baseURL.appending(path: "api/\(method)"), timeoutInterval: timeout)
        req.httpMethod = "POST"
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = body.isEmpty ? Data("{}".utf8) : body
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: req)
        } catch is URLError {
            throw HostError.unreachable
        }
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: data))?["error"] ?? "Host error \(status)"
            throw HostError.http(status, message)
        }
        return data
    }

    public func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        switch request {
        case let .events(since, client):
            var comps = URLComponents(url: baseURL.appending(path: "events"), resolvingAgainstBaseURL: false)
            comps?.queryItems = [.init(name: "since", value: String(since)), .init(name: "client", value: client)]
            return sse(comps?.url ?? baseURL.appending(path: "events"))
        case let .term(id):
            return sse(baseURL.appending(path: "term/\(id)"))
        }
    }

    public func states() -> AsyncStream<LinkState> {
        AsyncStream { $0.yield(.ready(.loopback)) }
    }

    private func sse(_ url: URL) -> AsyncThrowingStream<Data, Error> {
        // Idle timeout, not a total one: the host pings every 15 s, so silence this long means it's gone.
        var req = URLRequest(url: url, timeoutInterval: 45)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        let session = self.session
        let request = req
        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let (bytes, response) = try await session.bytes(for: request)
                    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                    guard status == 200 else { throw HostError.http(status, "Host error \(status)") }
                    for try await line in bytes.lines where line.hasPrefix("data:") {
                        continuation.yield(Data(line.dropFirst(5).utf8))
                    }
                    continuation.finish()
                } catch is URLError {
                    continuation.finish(throwing: HostError.unreachable)
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
