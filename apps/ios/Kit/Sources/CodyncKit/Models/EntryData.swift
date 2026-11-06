import Foundation

public struct EntryData: Codable, Hashable, Sendable {
    public var connectionRequest: ConnectionRequest?
    /// A memory line an older host put in the chat. Memory lives in the bot's memory settings, so it isn't shown.
    public var memory: MemoryLine?
    public var memoryFile: MemoryLine?
    /// notice: the agent summarized its context.
    public var compaction: Compaction?
    /// notice: the bot's chat starts over with this handoff.
    public var handoff: HandoffCard?
    public var routineId: String?
    public var runId: String?
    /// notice: a failed routine run the user can run again (`retryRoutineRun`).
    public var retry: Bool?
    /// notice: the bot moved to another agent of its chain, or back to its own.
    public var agentSwitch: AgentSwitch?
    /// notice (error): the bot has no fallbacks and its agent ran out of usage or was rate limited: offer to add one.
    public var addFallback: Bool?
    public var text: String?
    public var final: Bool?
    /// user: queued | sent | cancelled | failed · tool: pending | in_progress | completed | failed · permission: pending | answered | cancelled | expired
    public var status: String?
    public var clientNonce: String?
    public var title: String?
    public var toolKind: String?
    /// tool: whether it's the bot's own bookkeeping (composing its message, keeping its memory), left out of the
    /// working steps; `nil` from a host older than 2.25.0.
    public var bookkeeping: Bool?
    public var output: String?
    public var diffs: [FileDiff]?
    public var locations: [Location]?
    public var options: [PermissionOption]?
    public var selected: String?
    public var command: String?
    public var detail: String?
    public var cwd: String?
    public var entries: [PlanItem]?
    /// notice: info | error
    public var style: String?
    /// Structured conversation data added to the existing team-tool notice.
    public var botMessage: BotMessage?
    public var heading: String?
    public var delegationId: String?
    public var sourceBotId: String?
    public var targetBotId: String?
    /// The bot that wrote it (a group shows who spoke).
    public var author: String?
    /// user: the message it replies to, as the computer quoted it.
    public var replyTo: ReplyQuote?
    /// The user's emoji reactions, oldest first.
    public var reactions: [String]?
    /// The bots' reactions, one per bot, oldest first.
    public var botReactions: [BotReaction]?
    /// Files sent with a user message.
    public var attachments: [Attachment]?
    /// Immutable files explicitly sent by the bot.
    public var files: [SharedFile]?
    /// notice: a finished voice call's length.
    public var callSeconds: Int?
    /// The voice call it was said in (user), answered during (agent), or that it ends (notice); the chat shows those
    /// messages only in the call's conversation.
    public var call: String?
    /// user: `voice` when a call's voice model wrote it for the user (`send_to_bot`).
    public var from: String?
    /// notice: what was said on a realtime call, oldest first.
    public var transcript: [CallLine]?
    /// agent: an artifact card, the bot's artifact as published then (the newest version shows).
    public var artifact: ArtifactRef?

    public init(text: String? = nil, status: String? = nil, clientNonce: String? = nil) {
        self.text = text
        self.status = status
        self.clientNonce = clientNonce
    }
}

/// One line of a voice call: what the user said (`from: user`) or the voice answered (`voice`), with when (ms).
public struct CallLine: Codable, Hashable, Sendable {
    public var from: String
    public var text: String
    public var at: Int64?
    /// The app's id for the line: one sent again (its first answer lost) is kept once.
    public var id: String?

    public init(from: String, text: String, at: Int64?, id: String? = nil) {
        self.from = from
        self.text = text
        self.at = at
        self.id = id
    }
}

/// The message a reply quotes: its entry, who wrote it (`user` or a bot id) and its text (at most 500
/// characters, or its file names), and the artifact when it is an artifact card. A partial quote still decodes, so the
/// message itself always shows.
public struct ReplyQuote: Codable, Hashable, Sendable {
    public var id: String
    public var author: String
    public var text: String
    public var artifact: String?

    public init(id: String, author: String, text: String, artifact: String? = nil) {
        self.id = id
        self.author = author
        self.text = text
        self.artifact = artifact
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(String.self, forKey: .id) ?? ""
        author = try c.decodeIfPresent(String.self, forKey: .author) ?? ""
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        artifact = try c.decodeIfPresent(String.self, forKey: .artifact)
    }
}

/// A memory line from an older host, known only to keep it out of the chat.
public struct MemoryLine: Codable, Hashable, Sendable {}

/// One bot's emoji on a message.
public struct BotReaction: Codable, Hashable, Sendable {
    public var bot: String
    public var emoji: String
}

extension EntryData {
    /// The bots' reactions by emoji, in the order each first appeared.
    public var botReactionGroups: [(emoji: String, bots: [String])] {
        (botReactions ?? []).reduce(into: []) { groups, r in
            if let i = groups.firstIndex(where: { $0.emoji == r.emoji }) {
                groups[i].bots.append(r.bot)
            } else {
                groups.append((r.emoji, [r.bot]))
            }
        }
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

    public var isImage: Bool { Self.isImage(name) }

    public static func isImage(_ name: String) -> Bool {
        ["png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "bmp"].contains((name as NSString).pathExtension.lowercased())
    }
}

