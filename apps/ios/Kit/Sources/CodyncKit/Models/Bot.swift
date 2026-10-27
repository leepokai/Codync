import Foundation

// A bot as codync-host sends it (`bot` events, `sync`); field names match its camelCase JSON.

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
    /// Host-allocated personal workspace rather than a user-selected project.
    public var managedWorkspace: Bool = false
    /// `ask` or `auto`.
    public var permission: String
    public var model: String?
    /// The model's own settings (effort, fast…): agent option id → value.
    public var modelOptions: [String: String] = [:]
    /// The agent's Fast mode (Claude Code, Codex): faster replies for more usage.
    public var fastMode: Bool = false
    /// What the agent says about Fast mode: `unknown` (no session yet), `offered`, `unsupported`,
    /// or `refused` (it turned it off; `fastModeIssue` says why).
    public var fastModeSupport: String = "unknown"
    public var fastModeIssue: String?
    /// Agents that carry on, in order, when the bot's own can't answer (`AgentFallback.swift`). Nil from an older host.
    public var fallbacks: [BotFallback]?
    /// The bot may set its own fallbacks. Nil from an older host.
    public var selfFallbacks: Bool?
    public var pinned: Bool
    /// The computer's lead bot, which the host makes: first in the list, never deleted or hidden, and the one a
    /// call or shortcut that names no bot talks to.
    public var lead: Bool = false
    /// Where the user dragged it, lowest first within its `band`; ties sort by recent activity.
    public var position: Int = 0
    public var hidden: Bool
    public var notify: Bool?
    /// Installed connector / skill ids this bot uses (see `Market.swift`).
    public var connectors: [String]
    public var skills: [String]
    /// The bot can see and operate the computer's desktop (the built-in `computer` tools).
    public var computer: Bool
    /// The bot can create bots (which can too) and change any bot's setup (the built-in `manage` tools). Nil from an older host.
    public var manageTeam: Bool?
    /// Other computers in the user's networks list this bot and may message it. Nil from an older host.
    public var offeredToNetwork: Bool?
    /// The user's cloud assistant lists this bot and may message it. Nil from an older host.
    public var reachableByAssistants: Bool?
    /// The bot that created this one; nil when the user did.
    public var createdBy: String?
    /// Uses the memory all the user's bots share ("About you"). Nil from an older host.
    public var sharedMemory: Bool?
    /// Keeps the harness's own memory (Claude Code's…) on beside Codync's. Nil where it has no switch.
    public var harnessMemory: Bool?
    /// The harness keeps one memory for all its sessions that Codync can't turn off (Hermes Agent, Gemini CLI):
    /// every bot on it shares it. Nil from an older host.
    public var harnessMemoryShared: Bool?
    /// Moves whenever memory changes, so an open memory screen fetches it again.
    public var memoryRev: Int64?
    /// Created without a name: the host names it after its first few conversations.
    public var autoName: Bool = false
    public var createdAt: Int64

    // Runtime, filled in by the host.
    public var rev: Int64
    /// idle | working | needsInput | error
    public var status: String
    public var activity: String
    public var startedAt: Int64?
    /// Where the running turn talks: the bot's own chat, or a group.
    public var workingChat: String?
    /// The agent's last reported use of this bot's main session; nil until reported.
    public var contextUsage: ContextUsage?
    /// The fallback it runs on; nil on its own agent.
    public var agentStep: AgentStep?
    /// Agents of its chain resting now.
    public var resting: [AgentRest] = []
    public var unread: Int
    public var lastMessage: String?
    public var lastAt: Int64

    public var isGroup: Bool { kind == "group" }
    public var isWorking: Bool { status == "working" || status == "needsInput" }
    /// Working right now in this chat (not in a group it's in, or the reverse).
    public func isWorking(in chat: String) -> Bool {
        isWorking && (workingChat ?? id) == chat
    }
    public var needsInput: Bool { status == "needsInput" }
    /// Its band in the list: the lead, then pinned bots, then the rest. Dragging stays inside a band.
    public var band: Int { lead ? 0 : pinned ? 1 : 2 }
    public var folderName: String { managedWorkspace ? "Personal workspace" : (cwd as NSString).lastPathComponent }
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
        managedWorkspace = try c.decodeIfPresent(Bool.self, forKey: .managedWorkspace) ?? false
        permission = try c.decodeIfPresent(String.self, forKey: .permission) ?? "auto"
        model = try c.decodeIfPresent(String.self, forKey: .model)
        modelOptions = try c.decodeIfPresent([String: String].self, forKey: .modelOptions) ?? [:]
        fastMode = try c.decodeIfPresent(Bool.self, forKey: .fastMode) ?? false
        fastModeSupport = try c.decodeIfPresent(String.self, forKey: .fastModeSupport) ?? "unknown"
        fastModeIssue = try c.decodeIfPresent(String.self, forKey: .fastModeIssue)
        // A chain that doesn't decode stays unknown (nil), so saving the settings never sends an empty one.
        fallbacks = try? c.decodeIfPresent([BotFallback].self, forKey: .fallbacks)
        selfFallbacks = try? c.decodeIfPresent(Bool.self, forKey: .selfFallbacks)
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        lead = try c.decodeIfPresent(Bool.self, forKey: .lead) ?? false
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
        hidden = try c.decodeIfPresent(Bool.self, forKey: .hidden) ?? false
        notify = try c.decodeIfPresent(Bool.self, forKey: .notify)
        connectors = try c.decodeIfPresent([String].self, forKey: .connectors) ?? []
        skills = try c.decodeIfPresent([String].self, forKey: .skills) ?? []
        computer = try c.decodeIfPresent(Bool.self, forKey: .computer) ?? false
        manageTeam = try c.decodeIfPresent(Bool.self, forKey: .manageTeam)
        offeredToNetwork = try c.decodeIfPresent(Bool.self, forKey: .offeredToNetwork)
        reachableByAssistants = try c.decodeIfPresent(Bool.self, forKey: .reachableByAssistants)
        createdBy = try c.decodeIfPresent(String.self, forKey: .createdBy)
        sharedMemory = try c.decodeIfPresent(Bool.self, forKey: .sharedMemory)
        harnessMemory = try c.decodeIfPresent(Bool.self, forKey: .harnessMemory)
        harnessMemoryShared = try c.decodeIfPresent(Bool.self, forKey: .harnessMemoryShared)
        memoryRev = try c.decodeIfPresent(Int64.self, forKey: .memoryRev)
        autoName = try c.decodeIfPresent(Bool.self, forKey: .autoName) ?? false
        createdAt = try c.decodeIfPresent(Int64.self, forKey: .createdAt) ?? 0
        rev = try c.decodeIfPresent(Int64.self, forKey: .rev) ?? 0
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? "idle"
        activity = try c.decodeIfPresent(String.self, forKey: .activity) ?? ""
        startedAt = try c.decodeIfPresent(Int64.self, forKey: .startedAt)
        workingChat = try c.decodeIfPresent(String.self, forKey: .workingChat)
        contextUsage = try? c.decodeIfPresent(ContextUsage.self, forKey: .contextUsage)
        agentStep = try? c.decodeIfPresent(AgentStep.self, forKey: .agentStep)
        resting = (try? c.decodeIfPresent([AgentRest].self, forKey: .resting)) ?? []
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        lastMessage = try c.decodeIfPresent(String.self, forKey: .lastMessage)
        lastAt = try c.decodeIfPresent(Int64.self, forKey: .lastAt) ?? createdAt
    }
}
