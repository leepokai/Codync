import Foundation

/// An immutable host snapshot published by a bot. Separate from user uploads.
public struct SharedFile: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let size: Int64
    public let sha256: String
    /// A sound the chat plays in place, as the computer stamps it; missing (an older computer) means a plain file card.
    public let playsInChat: Bool

    public init(id: String, name: String, size: Int64, sha256: String, playsInChat: Bool = false) {
        self.id = id
        self.name = name
        self.size = size
        self.sha256 = sha256
        self.playsInChat = playsInChat
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(id: try c.decode(String.self, forKey: .id), name: try c.decode(String.self, forKey: .name),
                  size: try c.decode(Int64.self, forKey: .size), sha256: try c.decode(String.self, forKey: .sha256),
                  playsInChat: try c.decodeIfPresent(Bool.self, forKey: .playsInChat) ?? false)
    }

    /// The largest shared image the phone fetches whole to show.
    private static let inlineBytes: Int64 = 20 * 1024 * 1024
    /// An image small enough to show in the chat.
    public var showsAsImage: Bool { Attachment.isImage(name) && size <= Self.inlineBytes }

    public func validate() throws {
        guard size >= 0, size <= 100 * 1024 * 1024,
              sha256.count == 64, sha256.allSatisfy({ "0123456789abcdef".contains($0) }),
              !name.isEmpty, name.utf8.count <= 200, name != ".", name != "..",
              !name.contains("/"), !name.contains("\\"),
              !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw FileDownloadError.invalidMetadata
        }
    }
}
