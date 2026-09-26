import Foundation

// Wire types for codync-host (see host/src/api.rs). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

public struct Bot: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    /// `agent`, or `group`: several bots and the user in one chat (it has no agent, folder or harness).
    public var kind: String
    /// A group's bots.
    public var members: [String]
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
    /// Installed connector / skill ids this bot uses (see `Market.swift`).
    public var connectors: [String]
    public var skills: [String]
    /// The bot can see and operate the computer's desktop (the built-in `computer` tools).
    public var computer: Bool
    public var createdAt: Int64

    // Runtime, filled in by the host.
    public var rev: Int64
    /// idle | working | needsInput | error
    public var status: String
    public var activity: String
    public var startedAt: Int64?
    /// Where the running turn talks: the chat (a bot's own, or a group) and thread root.
    public var workingChat: String?
    public var workingThread: String?
    public var unread: Int
    public var lastMessage: String?
    public var lastAt: Int64

    public var isGroup: Bool { kind == "group" }
    public var isWorking: Bool { status == "working" || status == "needsInput" }
    /// Working right now in this chat's main conversation (`thread == nil`) or in that thread.
    public func isWorking(in chat: String, thread: String?) -> Bool {
        isWorking && (workingChat ?? id) == chat && workingThread == thread
    }
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
    /// The chat it's in: a bot's, or a group's.
    public var botId: String
    /// The root message of the thread it's in; nil in the main chat.
    public var threadId: String?
    public var rev: Int64
    /// user | agent | thought | tool | plan | permission | notice
    public var kind: String
    public var turn: Int64
    public var data: EntryData
    public var createdAt: Int64
    public var updatedAt: Int64

    public init(id: String, seq: Int64, botId: String, threadId: String? = nil, rev: Int64, kind: String, turn: Int64, data: EntryData, createdAt: Int64, updatedAt: Int64) {
        self.id = id
        self.seq = seq
        self.botId = botId
        self.threadId = threadId
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
    /// The bot that wrote it (a group shows who spoke).
    public var author: String?
    /// On a main-chat message that has a thread: its replies.
    public var thread: ThreadSummary?

    public init(text: String? = nil, status: String? = nil, clientNonce: String? = nil) {
        self.text = text
        self.status = status
        self.clientNonce = clientNonce
    }
}

public struct ThreadSummary: Codable, Hashable, Sendable {
    public var count: Int
    public var lastAt: Int64
    /// Who replied, first reply first: bot ids, and `user`.
    public var authors: [String]
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
    public var resetDate: Date? { resetsAt.map(Date.init(milliseconds:)) ?? resetsText.flatMap { Self.parseReset($0) } }

    /// Claude Code's text: "Sep 26 at 12pm (Asia/Taipei)", "Sep 25 at 7:30pm (Asia/Taipei)", or just "7:30pm (…)".
    /// The year isn't given: the next such date from `now`.
    static func parseReset(_ text: String, now: Date = .now) -> Date? {
        guard let match = text.firstMatch(of: /^(?:(\w{3} \d{1,2}) at )?(\d{1,2})(?::(\d{2}))?(am|pm)(?: \(([^)]+)\))?$/) else { return nil }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = match.5.flatMap { TimeZone(identifier: String($0)) } ?? .current
        guard let hour12 = Int(match.2) else { return nil }
        var parts = cal.dateComponents([.year, .month, .day], from: now)
        if let day = match.1 {
            let f = DateFormatter()
            f.locale = Locale(identifier: "en_US_POSIX")
            f.dateFormat = "MMM d"
            guard let md = f.date(from: String(day)) else { return nil }
            let c = Calendar(identifier: .gregorian).dateComponents([.month, .day], from: md)
            parts.month = c.month
            parts.day = c.day
        }
        parts.hour = hour12 % 12 + (match.4 == "pm" ? 12 : 0)
        parts.minute = match.3.flatMap { Int($0) } ?? 0
        guard var date = cal.date(from: parts) else { return nil }
        // Past by more than a day: it means next year (or tomorrow, for a bare time).
        if date < now - 86_400 || (match.1 == nil && date < now) {
            date = cal.date(byAdding: match.1 == nil ? .day : .year, value: 1, to: date) ?? date
        }
        return date
    }

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
    /// ACP registry id, for the agent's icon (`agent-<id>` in the asset catalog).
    public var registry: String?
    public var description: String?
    public var installed: Bool?
    /// Known to Codync by name: it can install and sign in to it.
    public var curated: Bool?
    /// Missing when the host can't tell.
    public var signedIn: Bool?
    public var canInstall: Bool?
}

/// How an agent can be signed in, from asking the agent itself.
public struct AgentAuth: Decodable, Sendable {
    public var signedIn: Bool?
    /// The agent's own words when it wants signing in (can hold a pairing code).
    public var detail: String?
    public var methods: [AuthMethod]
    /// Keys saved on the computer for this agent (names only).
    public var savedEnv: [String]?
    /// Codync's own sign-in command is available.
    public var login: Bool?
}

public struct AuthMethod: Decodable, Sendable, Identifiable, Hashable {
    public enum Kind: String, Decodable, Sendable {
        /// Runs in a setup terminal.
        case terminal
        /// The agent signs itself in (usually a browser on the computer).
        case agent
        /// Keys typed here, kept on the computer.
        case envVar
    }

    public struct Var: Decodable, Sendable, Hashable {
        public var name: String
        public var label: String
        public var secret: Bool
        public var optional: Bool
    }

    public var id: String
    public var name: String
    public var description: String?
    public var kind: Kind?
    public var vars: [Var]?
    public var link: String?
}

/// What a setup terminal on the computer runs.
public enum SetupStep: String, Codable, Sendable {
    case install, login
}

public struct Hello: Codable, Sendable {
    public var hostId: String
    public var name: String
    public var version: String
    public var os: String
    /// laptop · macmini · macstudio · imac · macpro · desktop · linux (missing from older hosts).
    public var device: String?
    public var home: String?
    public var backends: [Backend]
    public var rev: Int64
    /// Missing from hosts that predate remote screen.
    public var screen: ScreenState?
    /// The host's current direct addresses.
    public var urls: [String]?
    /// Channel identity (§9.4); missing only on loopback to an older host.
    public var computerId: ComputerID?
    public var signKey: String?
    public var boxKey: String?
    public var `protocol`: Int?
    /// The cloud relay the host uses; nil = cloud off.
    public var cloud: String?
}

/// The computer's remote screen: whether phones can view/control it, and who's in control.
public struct ScreenState: Codable, Equatable, Sendable {
    public var enabled = false
    /// The screen helper is running.
    public var connected = false
    public var platform = ""
    /// Screen recording is permitted.
    public var capture = false
    /// Input injection is permitted.
    public var input = false
    public var displays: [ScreenDisplay] = []
    /// A phone took over: bots may only look.
    public var userControl = false
    /// The bot using the computer right now.
    public var agentBot: String?
    public var viewers = 0

    public init() {}

    public var available: Bool { enabled && connected && capture }
    public var mainDisplay: ScreenDisplay? { displays.first(where: \.main) ?? displays.first }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? false
        connected = try c.decodeIfPresent(Bool.self, forKey: .connected) ?? false
        platform = try c.decodeIfPresent(String.self, forKey: .platform) ?? ""
        capture = try c.decodeIfPresent(Bool.self, forKey: .capture) ?? false
        input = try c.decodeIfPresent(Bool.self, forKey: .input) ?? false
        displays = try c.decodeIfPresent([ScreenDisplay].self, forKey: .displays) ?? []
        userControl = try c.decodeIfPresent(Bool.self, forKey: .userControl) ?? false
        agentBot = try c.decodeIfPresent(String.self, forKey: .agentBot)
        viewers = try c.decodeIfPresent(Int.self, forKey: .viewers) ?? 0
    }
}

public struct ScreenDisplay: Codable, Hashable, Identifiable, Sendable {
    public var id: UInt32
    public var name: String
    /// Size in points: the coordinate space of screen input.
    public var width: Double
    public var height: Double
    public var main: Bool
}

public struct SyncResponse: Codable, Sendable {
    public var hostId: String
    public var rev: Int64
    public var bots: [Bot]
    public var entries: [Entry]
    public var usage: Usage
}

/// A fresh pairing code to show as a QR on this computer (loopback `pairing`).
public struct PairingInfo: Codable, Sendable {
    public var pairingUrl: String
    public var urls: [String]
    public var svg: String?
    public var expiresAt: Int64?
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

/// Create / update payload for a group chat.
public struct GroupDraft: Codable, Hashable, Sendable {
    public var id: String?
    public var kind = "group"
    public var name: String
    public var members: [String]
    public var pinned: Bool?

    public init(name: String, members: [String]) {
        self.name = name
        self.members = members
    }

    public init(_ group: Bot) {
        id = group.id
        name = group.name
        members = group.members
        pinned = group.pinned
    }
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
    public var connectors: [String]?
    public var skills: [String]?
    public var computer: Bool?

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
        connectors = bot.connectors
        skills = bot.skills
        computer = bot.computer
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
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "agent"
        members = try c.decodeIfPresent([String].self, forKey: .members) ?? []
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
        connectors = try c.decodeIfPresent([String].self, forKey: .connectors) ?? []
        skills = try c.decodeIfPresent([String].self, forKey: .skills) ?? []
        computer = try c.decodeIfPresent(Bool.self, forKey: .computer) ?? false
        createdAt = try c.decodeIfPresent(Int64.self, forKey: .createdAt) ?? 0
        rev = try c.decodeIfPresent(Int64.self, forKey: .rev) ?? 0
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? "idle"
        activity = try c.decodeIfPresent(String.self, forKey: .activity) ?? ""
        startedAt = try c.decodeIfPresent(Int64.self, forKey: .startedAt)
        workingChat = try c.decodeIfPresent(String.self, forKey: .workingChat)
        workingThread = try c.decodeIfPresent(String.self, forKey: .workingThread)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        lastMessage = try c.decodeIfPresent(String.self, forKey: .lastMessage)
        lastAt = try c.decodeIfPresent(Int64.self, forKey: .lastAt) ?? createdAt
    }
}
