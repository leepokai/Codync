import Foundation

// Wire types for codync-host (see host/src/api.rs). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

public struct Bot: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var name: String
    public var description: String
    public var avatarColor: String
    public var avatarShape: String
    public var backend: String
    public var command: String?
    public var cwd: String
    /// `ask` or `auto`.
    public var permission: String
    public var model: String?
    public var pinned: Bool
    public var hidden: Bool
    public var notify: Bool?
    public var createdAt: Int64

    // Runtime, filled in by the host.
    public var rev: Int64
    /// idle | working | needsInput | error
    public var status: String
    public var activity: String
    public var startedAt: Int64?
    public var unread: Int
    public var lastMessage: String?
    public var lastAt: Int64

    public var isWorking: Bool { status == "working" || status == "needsInput" }
    public var needsInput: Bool { status == "needsInput" }
    public var folderName: String { (cwd as NSString).lastPathComponent }
}

/// Just enough of a `bot` event to detect deletions.
struct BotStub: Decodable {
    var id: String
    var deleted: Bool?
    var rev: Int64
}

public struct Entry: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var seq: Int64
    public var botId: String
    public var rev: Int64
    /// user | agent | thought | tool | plan | permission | notice
    public var kind: String
    public var turn: Int64
    public var data: EntryData
    public var createdAt: Int64
    public var updatedAt: Int64

    public init(id: String, seq: Int64, botId: String, rev: Int64, kind: String, turn: Int64, data: EntryData, createdAt: Int64, updatedAt: Int64) {
        self.id = id
        self.seq = seq
        self.botId = botId
        self.rev = rev
        self.kind = kind
        self.turn = turn
        self.data = data
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// Chat-visible (Grok Bot model): user messages, final replies, approval cards, notices.
    public var isChat: Bool {
        switch kind {
        case "user", "permission", "notice": true
        case "agent": data.final == true
        default: false
        }
    }

    public var date: Date { Date(milliseconds: createdAt) }
}

public struct EntryData: Codable, Hashable, Sendable {
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

    public init(text: String? = nil, status: String? = nil, clientNonce: String? = nil) {
        self.text = text
        self.status = status
        self.clientNonce = clientNonce
    }
}

public struct FileDiff: Codable, Hashable, Sendable {
    public var path: String
    public var added: Int
    public var removed: Int
    public var isNew: Bool
    public var startLine: Int
    public var patch: String
}

public struct Location: Codable, Hashable, Sendable {
    public var path: String
    public var line: Int?
}

public struct PermissionOption: Codable, Hashable, Sendable, Identifiable {
    public var optionId: String
    public var name: String
    /// allow_once | allow_always | reject_once | reject_always
    public var kind: String
    public var id: String { optionId }
}

public struct PlanItem: Codable, Hashable, Sendable {
    public var content: String
    public var status: String
    public var priority: String?
}

public struct Usage: Codable, Hashable, Sendable {
    public var providers: [UsageProvider]
    public init(providers: [UsageProvider] = []) { self.providers = providers }
}

public struct UsageProvider: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var windows: [UsageWindow]
    public var source: String
    public var updatedAt: Int64
}

public struct UsageWindow: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var label: String
    public var percent: Double
    public var resetsAt: Int64?
    /// Human text when only that is known ("Sep 26 at 12pm (Asia/Taipei)").
    public var resetsText: String?
    public var resetDate: Date? { resetsAt.map(Date.init(milliseconds:)) }

    /// "resets in 3h 20m" / "resets Sep 26 at 12pm", or nil.
    public var resetDescription: String? {
        if let d = resetDate { return "resets in \(RelativeTime.until(d))" }
        return resetsText.map { "resets \($0.replacingOccurrences(of: #" \(.*\)$"#, with: "", options: .regularExpression))" }
    }
}

public struct Backend: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var available: Bool
    public var command: String
    public var installHint: String
}

public struct Hello: Codable, Sendable {
    public var hostId: String
    public var name: String
    public var version: String
    public var os: String
    public var home: String?
    public var backends: [Backend]
    public var rev: Int64
}

public struct SyncResponse: Codable, Sendable {
    public var hostId: String
    public var rev: Int64
    public var bots: [Bot]
    public var entries: [Entry]
    public var usage: Usage
}

public struct DirListing: Codable, Sendable {
    public var path: String
    public var parent: String?
    public var isGit: Bool
    public var dirs: [DirItem]
}

public struct DirItem: Codable, Hashable, Sendable, Identifiable {
    public var name: String
    public var path: String
    public var isGit: Bool
    public var id: String { path }
}

/// Create / update payload for a bot.
public struct BotDraft: Codable, Hashable, Sendable {
    public var id: String?
    public var name: String
    public var description: String
    public var avatarColor: String
    public var avatarShape: String
    public var backend: String
    public var command: String?
    public var cwd: String
    public var permission: String
    public var model: String?
    public var pinned: Bool?
    public var hidden: Bool?
    public var notify: Bool?

    public init(name: String = "", description: String = "", avatarColor: String = AvatarPalette.colors.randomElement()!.id,
                avatarShape: String = AvatarPalette.shapes.randomElement()!, backend: String = "claude", cwd: String = "",
                permission: String = "ask") {
        self.name = name
        self.description = description
        self.avatarColor = avatarColor
        self.avatarShape = avatarShape
        self.backend = backend
        self.cwd = cwd
        self.permission = permission
    }

    public init(_ bot: Bot) {
        id = bot.id
        name = bot.name
        description = bot.description
        avatarColor = bot.avatarColor
        avatarShape = bot.avatarShape
        backend = bot.backend
        command = bot.command
        cwd = bot.cwd
        permission = bot.permission
        model = bot.model
        pinned = bot.pinned
        hidden = bot.hidden
        notify = bot.notify
    }
}

public extension Date {
    init(milliseconds: Int64) { self.init(timeIntervalSince1970: Double(milliseconds) / 1000) }
}

extension Bot {
    /// Lenient: a host that is a little newer or older than the app still yields a usable bot.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? "Bot"
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        avatarColor = try c.decodeIfPresent(String.self, forKey: .avatarColor) ?? "blue"
        avatarShape = try c.decodeIfPresent(String.self, forKey: .avatarShape) ?? "blob"
        backend = try c.decodeIfPresent(String.self, forKey: .backend) ?? "custom"
        command = try c.decodeIfPresent(String.self, forKey: .command)
        cwd = try c.decodeIfPresent(String.self, forKey: .cwd) ?? ""
        permission = try c.decodeIfPresent(String.self, forKey: .permission) ?? "ask"
        model = try c.decodeIfPresent(String.self, forKey: .model)
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        hidden = try c.decodeIfPresent(Bool.self, forKey: .hidden) ?? false
        notify = try c.decodeIfPresent(Bool.self, forKey: .notify)
        createdAt = try c.decodeIfPresent(Int64.self, forKey: .createdAt) ?? 0
        rev = try c.decodeIfPresent(Int64.self, forKey: .rev) ?? 0
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? "idle"
        activity = try c.decodeIfPresent(String.self, forKey: .activity) ?? ""
        startedAt = try c.decodeIfPresent(Int64.self, forKey: .startedAt)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        lastMessage = try c.decodeIfPresent(String.self, forKey: .lastMessage)
        lastAt = try c.decodeIfPresent(Int64.self, forKey: .lastAt) ?? createdAt
    }
}
