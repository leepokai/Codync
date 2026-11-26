import Foundation

/// iMate's one personal token, kept on the host. Values never come back.
public struct ImateStatus: Decodable, Sendable {
    public var signedIn: Bool
    public var loginKeys: [String]
    public var installHint: String?
}
