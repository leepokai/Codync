import Foundation

public struct EntryData: Codable, Hashable, Sendable {
    public var connectionRequest: ConnectionRequest?
    public var routineId: String?
    public var runId: String?
    public var text: String?
    public var final: Bool?
    /// user: queued | sent | cancelled | failed · tool: pending | in_progress | completed | failed · permission: pending | answered | cancelled | expired
    public var status: String?
    public var clientNonce: String?
    public var title: String?
    public var toolKind: String?
    public var output: String?
    public var diffs: [FileDiff]?
    public var locations: [Location]?
    public var options: [PermissionOption]?
    public var selected: String?
    public var command: String?
    public var detail: String?
    public var cwd: String?
    public var entries: [PlanItem]?
    /// notice: info | error | divider
    public var style: String?
    /// Structured conversation data added to the existing team-tool notice.
    public var botMessage: BotMessage?
    public var heading: String?
    public var delegationId: String?
    public var sourceBotId: String?
    public var targetBotId: String?
    /// The bot that wrote it (a group shows who spoke).
    public var author: String?
    /// On a main-chat message that has a thread: its replies.
    public var thread: ThreadSummary?
    /// The user's emoji reactions, oldest first.
    public var reactions: [String]?
    /// Files sent with a user message.
    public var attachments: [Attachment]?
    /// Immutable files explicitly sent by the bot.
    public var files: [SharedFile]?
    /// notice: a finished voice call's length.
    public var callSeconds: Int?

    public init(text: String? = nil, status: String? = nil, clientNonce: String? = nil) {
        self.text = text
        self.status = status
        self.clientNonce = clientNonce
    }
}

/// A file sent with a message; the agent reads it on the computer.
public struct Attachment: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var size: Int64

    public init(id: String, name: String, size: Int64) {
        self.id = id
        self.name = name
        self.size = size
    }

    public var isImage: Bool {
        ["png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "bmp"].contains((name as NSString).pathExtension.lowercased())
    }
}

