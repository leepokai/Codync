import Foundation

/// An app given MCP access on a computer (`mcpClients`), as a phone shows it: phones list and revoke them, the
/// computer adds them and sets what they may do.
public struct McpApp: Decodable, Sendable, Identifiable, Equatable {
    public var id: String
    public var label: String
    /// `readOnly` | `ask` | `auto` | `full`; an unknown one reads as Read only.
    public var access: String
    public var createdAt: Int64
    public var expiresAt: Int64
    public var revokedAt: Int64?

    /// The name of its level, as the computer's settings say it.
    public var levelName: String {
        switch access {
        case "ask": "Ask for every request"
        case "auto": "Auto"
        case "full": "Full access"
        default: "Read only"
        }
    }

    /// Whole days until it stops working, 0 once it has.
    public func daysLeft(now: Date = .now) -> Int {
        let left = Double(expiresAt) - now.timeIntervalSince1970 * 1000
        return max(0, Int((left / 86_400_000).rounded(.up)))
    }

    /// The line under it: its level and until when it works.
    public func detail(now: Date = .now) -> String {
        let left = daysLeft(now: now)
        let until = left == 0 ? L10n.tr("Expired") : L10n.fill(L10n.tr("Expires in {count} days"), ["count": "\(left)"])
        return L10n.tr(levelName) + " · " + until
    }
}

public struct McpApps: Decodable, Sendable {
    public var clients: [McpApp]

    /// The apps not revoked, newest first (the computer's own list shows the same).
    public var active: [McpApp] {
        clients.filter { $0.revokedAt == nil }.sorted { $0.createdAt > $1.createdAt }
    }

    public init(from decoder: Decoder) throws {
        struct Lossy: Decodable { let value: McpApp?; init(from d: Decoder) throws { value = try? McpApp(from: d) } }
        let c = try decoder.container(keyedBy: CodingKeys.self)
        clients = (try c.decodeIfPresent([Lossy].self, forKey: .clients) ?? []).compactMap(\.value)
    }

    private enum CodingKeys: String, CodingKey { case clients }
}

extension HostClient {
    public func mcpApps() async throws -> McpApps {
        try await call("mcpClients")
    }

    /// Its token stops working right away.
    public func revokeMcpApp(id: String) async throws {
        struct Body: Encodable { let id: String }
        let _: Empty = try await call("mcpClientRevoke", Body(id: id))
    }
}
