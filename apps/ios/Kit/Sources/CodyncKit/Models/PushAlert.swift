import Foundation

/// Private notification content. Only the host and the device can read these fields.
public struct PushAlert: Decodable, Sendable {
    public let title: String
    public let subtitle: String?
    public let body: String
    /// The member bot speaking, when the alert is for a group chat.
    public let from: String?
    /// The chat, its first members and the speaker: names and avatars only, sent with the alert
    /// so a bot the phone hasn't seen yet still shows its face.
    public let faces: [Bot]?
    /// Unread across the sending computer's roster, for the app icon's badge; `nil` from an older host.
    public let unread: Int?
}
