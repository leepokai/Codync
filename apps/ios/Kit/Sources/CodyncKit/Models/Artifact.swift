import Foundation

/// A self-contained HTML document a bot published for the user (`host/src/chat/artifacts.rs`). Its HTML comes from
/// `readArtifact`; publishing again under the same id replaces it and moves `updatedAt`.
public struct Artifact: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var botId: String
    public var title: String
    public var size: Int64
    public var sha256: String
    public var createdAt: Int64
    public var updatedAt: Int64
    /// Its newest card in the bot's chat: what a message about the artifact replies to. None when the artifact was
    /// published from a routine run or another bot's request.
    public var entryId: String?

    public var date: Date { Date(milliseconds: updatedAt) }

    /// The first host with artifacts and the Library.
    public static let firstHost = "2.25.0"

    /// How a message about the artifact quotes its card until the computer's own quote arrives.
    public var replyQuote: ReplyQuote? {
        entryId.map { ReplyQuote(id: $0, author: botId, text: "Artifact: \(title)", artifact: id) }
    }
}

/// What an artifact card in the chat carries (`data.artifact`): the artifact and the version it announced.
public struct ArtifactRef: Codable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var updatedAt: Int64
}
