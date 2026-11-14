import Foundation

/// A host-owned screen session and its short-lived ICE configuration. Never persist credentials.
public struct ScreenConnection: Codable, Sendable {
    public var session: String
    public var iceServers: [ScreenIceServer]
    /// Unix milliseconds, matching the host/cloud API.
    public var expiresAt: Int64
    public var trickle: Bool

    private enum CodingKeys: String, CodingKey { case session, iceServers, expiresAt, trickle }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        session = try c.decode(String.self, forKey: .session)
        iceServers = try c.decode([ScreenIceServer].self, forKey: .iceServers)
        expiresAt = try c.decode(Int64.self, forKey: .expiresAt)
        trickle = try c.decodeIfPresent(Bool.self, forKey: .trickle) ?? false
    }
}

public struct ScreenIceServer: Codable, Sendable {
    public var urls: [String]
    public var username: String?
    public var credential: String?
}
