import Foundation

/// A host-owned screen session and its short-lived ICE configuration. Never persist credentials.
public struct ScreenConnection: Codable, Sendable {
    public var session: String
    public var iceServers: [ScreenIceServer]
    /// Unix milliseconds, matching the host/cloud API.
    public var expiresAt: Int64
}

public struct ScreenIceServer: Codable, Sendable {
    public var urls: [String]
    public var username: String?
    public var credential: String?
}
