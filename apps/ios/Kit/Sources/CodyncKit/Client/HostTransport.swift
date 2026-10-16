import Foundation

/// How the encrypted channel reaches a host: direct on the LAN or through the cloud relay.
public enum HostRoute: Sendable, Equatable {
    case direct, relay
}

public enum HostStreamRequest: Sendable {
    case events(since: Int64, client: String)
    case term(String)
    case screenCandidates(String)
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
    func enqueue(botId: String, text: String, clientNonce: String, threadId: String?) async throws
    func cancelQueued(clientNonce: String) async -> MailboxCancel
    func listQueued() async throws -> [QueuedItem]
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

    /// The link failed, not the request: the same call can work once the computer is reachable again.
    public var isTransient: Bool {
        switch self {
        case .unreachable, .computerOffline: true
        default: false
        }
    }
}
