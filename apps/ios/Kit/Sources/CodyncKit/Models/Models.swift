import Foundation

// Wire types for codync-host (see host/src/api/). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

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

    /// Chat-visible (Grok Bot model): user messages, final replies, approval cards, notices. Of memory and context,
    /// only a fresh conversation gets a line: memory lives in the bot's memory settings, a summary keeps the same
    /// conversation going, and a kept handoff changed nothing.
    public var isChat: Bool {
        switch kind {
        case "user", "permission": true
        case "notice": data.memory == nil && data.memoryFile == nil && data.compaction == nil && data.handoff?.status != "kept"
        case "agent": data.final == true
        default: false
        }
    }

    /// A message a reply can quote: the user's or a final bot reply, once the computer has it.
    public var isReplyable: Bool {
        (kind == "user" || (kind == "agent" && data.final == true)) && !id.hasPrefix("local-")
    }

    /// How a reply quotes this message until the computer's own quote arrives: who wrote it and its
    /// text, or its file names.
    public var replyQuote: ReplyQuote {
        let names = (data.attachments?.map(\.name) ?? []) + (data.files?.map(\.name) ?? [])
        let text = data.text.flatMap { $0.isEmpty ? nil : $0 } ?? names.joined(separator: ", ")
        return ReplyQuote(id: id, author: kind == "user" ? "user" : data.author ?? botId, text: String(text.prefix(500)),
                          artifact: data.artifact?.id)
    }

    public var date: Date { Date(milliseconds: createdAt) }
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
    /// A reading that isn't a share of a cap ("$4.20", "1.2M tokens", "Unlimited"). Shown instead of the percent.
    public var text: String? = nil
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
    /// Only for an agent that runs on Node.js, on a computer without its own: Codync's copy
    /// is `missing` or `added`.
    public var node: String?

    /// It can't run until Node.js is added to the computer.
    public var needsNode: Bool { node == "missing" }
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
    case install, node, login
}

public struct Hello: Codable, Sendable {
    public var icons: [String: CustomIcon]?
    public var iconsRevision: Int64?
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
    /// The oldest app version this host serves (missing from hosts that predate the check).
    public var minApp: String?
    /// The cloud relay the host uses; nil = cloud off.
    public var cloud: String?
    /// `installHostUpdate` can update this computer from here (missing from older hosts: it can't).
    public var canUpdate: Bool?
    /// The latest release the computer found; nil while unknown (development builds, older hosts).
    public var latestVersion: String?
}

public struct RecentWorkspace: Codable, Sendable, Identifiable {
    public var path: String
    public var usedAt: Int64
    public var id: String { path }
}

public struct DirListing: Codable, Sendable {
    public var recent: [RecentWorkspace]?
    public var ancestors: [DirItem]?
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
    /// What the group is for; its bots are told.
    public var description = ""
    public var members: [String]
    public var pinned: Bool?

    public init(name: String, description: String = "", members: [String]) {
        self.name = name
        self.description = description
        self.members = members
    }

    public init(_ group: Bot) {
        id = group.id
        name = group.name
        description = group.description
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
    /// Sent as null when nil (`BotUpdate`): the agent's default.
    public var mode: String?
    public var model: String?
    public var modelOptions: [String: String] = [:]
    public var fastMode: Bool?
    /// Nil where the host has none (an older one), so it isn't sent.
    public var fallbacks: [BotFallback]?
    public var selfFallbacks: Bool?
    public var pinned: Bool?
    public var hidden: Bool?
    public var notify: Bool?
    public var connectors: [String]?
    public var skills: [String]?
    public var computer: Bool?
    public var manageTeam: Bool?
    /// Nil from a host without the switch, so it isn't sent.
    public var offeredToNetwork: Bool?
    public var reachableByAssistants: Bool?
    /// Only sent when the user changes it: unset, the host picks by the bot's folder.
    public var sharedMemory: Bool?
    /// Only sent when the user changes it.
    public var harnessMemory: Bool?

    public init(name: String = "", description: String = "", avatarColor: String = AvatarPalette.colors.randomElement()!.id,
                avatarShape: String = AvatarPalette.shapes.randomElement()!, backend: String = "claude", cwd: String = "",
                permission: String = "ask", mode: String? = AgentMode.auto) {
        self.name = name
        self.description = description
        self.avatarColor = avatarColor
        self.avatarShape = avatarShape
        self.backend = backend
        self.cwd = cwd
        self.permission = permission
        self.mode = mode
    }

    public init(_ bot: Bot) {
        id = bot.id
        name = bot.name
        description = bot.description
        avatarColor = bot.avatarColor
        avatarShape = bot.avatarShape
        backend = bot.backend
        command = bot.command
        cwd = bot.managedWorkspace ? "" : bot.cwd
        permission = bot.permission
        mode = bot.mode
        model = bot.model
        modelOptions = bot.modelOptions
        fastMode = bot.fastMode
        fallbacks = bot.fallbacks
        selfFallbacks = bot.selfFallbacks
        pinned = bot.pinned
        hidden = bot.hidden
        notify = bot.notify
        connectors = bot.connectors
        skills = bot.skills
        computer = bot.computer
        manageTeam = bot.manageTeam
        offeredToNetwork = bot.offeredToNetwork
        reachableByAssistants = bot.reachableByAssistants
    }

    /// This draft with each field it hasn't changed from `base` (the bot as the form loaded it) taken from `fresh`
    /// (the bot as the host has it now), so saving the form keeps a change made elsewhere: a bot that manages the
    /// team, another device.
    public func rebased(from base: BotDraft, onto fresh: BotDraft) -> BotDraft {
        var draft = self
        func follow<Value: Equatable>(_ field: WritableKeyPath<BotDraft, Value>) {
            if self[keyPath: field] == base[keyPath: field] { draft[keyPath: field] = fresh[keyPath: field] }
        }
        follow(\.name); follow(\.description); follow(\.avatarColor); follow(\.avatarShape); follow(\.backend)
        follow(\.command); follow(\.cwd); follow(\.permission); follow(\.mode); follow(\.model); follow(\.modelOptions)
        follow(\.fastMode); follow(\.fallbacks); follow(\.selfFallbacks); follow(\.pinned); follow(\.hidden)
        follow(\.notify); follow(\.connectors); follow(\.skills); follow(\.computer); follow(\.manageTeam)
        follow(\.sharedMemory); follow(\.harnessMemory); follow(\.offeredToNetwork); follow(\.reachableByAssistants)
        return draft
    }
}

public extension Date {
    init(milliseconds: Int64) { self.init(timeIntervalSince1970: Double(milliseconds) / 1000) }
}
