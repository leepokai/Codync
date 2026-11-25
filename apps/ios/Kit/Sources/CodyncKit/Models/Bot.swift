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
    public var customIcon: CustomIcon?
    public var backend: String
    public var command: String?
    /// The login profile it runs on; nil for the login the computer already has.
    public var loginProfile: String?
    public var cwd: String
    /// The bot's own folder, which the host made and keeps versions of, rather than a project the user chose.
    public var managedWorkspace: Bool = false
    /// `ask` or `auto`: whether Codync asks about what the agent asks, or approves it.
    public var permission: String
    /// The agent mode it works in (Claude's Manual / Plan / Auto…): the agent's id, or `auto` for its Auto mode
    /// under any id. Nil keeps the agent's default.
    public var mode: String?
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
    /// Advisor mode: the agent its threads run on; nil while they run on the bot's own.
    public var threadAgent: BotFallback?
    /// In Advisor mode, the threads' fallbacks. Nil from an older host: no Advisor mode there.
    public var threadFallbacks: [BotFallback]?
    public var pinned: Bool
    /// The computer's lead bot: the host makes the first, and the user may pick another (`setLead`) or delete it.
    /// Placed in the list like any bot, never hidden, and the one a call or shortcut that names no bot talks to.
    public var lead: Bool = false
    /// Where the user dragged it, lowest first within its `band`; ties sort by recent activity.
    public var position: Int = 0
    /// The user's section in the roster ("Work"); nil in none.
    public var section: String?
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
    /// When Codync summarizes its chat ahead of time. Nil where it doesn't (an agent it can't ask, a group, an
    /// older host): no setting to show.
    public var compaction: CompactionThreshold?
    /// The sizes `compaction` may take; nil until the agent has told its context window.
    public var compactionTokens: TokenRange?
    /// Moves whenever memory changes, so an open memory screen fetches it again.
    public var memoryRev: Int64?
    /// Created without a name: the host names it after its first few conversations.
    public var autoName: Bool = false
    /// The box of threads that run without a bot: the Threads tab's "No bot". False from an older host.
    public var plain: Bool = false
    public var createdAt: Int64

    // Runtime, filled in by the host.
    public var rev: Int64
    /// idle | working | needsInput | error
    public var status: String
    public var activity: String
    public var startedAt: Int64?
    /// Where the running turn talks: the bot's own chat, or a group.
    public var workingChat: String?
    /// The fallback it runs on; nil on its own agent.
    public var agentStep: AgentStep?
    /// Agents of its chain resting now.
    public var resting: [AgentRest] = []
    /// The `/` commands of the agent it runs on now. Empty from an older host, and for a group.
    public var agentCommands: [AgentCommand] = []
    /// What it keeps running on the computer. Empty from an older host, and for a group.
    public var devServers: [DevServer] = []
    /// Its box (`WorkTask.swift`): live and waiting tasks, then the ones ended in the last day. Empty from an
    /// older host: the feature doesn't show.
    public var work: [WorkTask] = []
    /// The most threads it runs at once (1 to 6). Nil from an older host: no setting to show.
    public var maxThreads: Int?
    /// Threads of its that work at once in one git repository each get a worktree. Nil from an older host: no
    /// setting to show.
    public var threadWorktrees: Bool?
    /// Its folder is in a git repository: where `threadWorktrees` matters.
    public var gitRepo: Bool = false
    /// The user's folders it sends work to, the first its default; `cwd` is then always its own folder. Nil from an
    /// older host, where `cwd` is the project.
    public var projects: [String]?
    /// Whether each of `projects` is in a git repository, in the same order.
    public var projectGitRepos: [Bool] = []
    /// When its threads get a git worktree. Nil from an older host (`threadWorktrees`) or a mode this app doesn't know.
    public var worktreeMode: WorktreeMode?
    /// Per project path: folders of its main checkout, relative to it, linked into each new worktree of it
    /// (`node_modules`). Nil from an older host: no setting to show.
    public var sharedFolders: [String: [String]]?

    /// `path` is one of its projects and in a git repository: looked up by path, so a reorder or an unsaved add in
    /// a form can't shift it.
    public func isGitProject(_ path: String) -> Bool {
        zip(projects ?? [], projectGitRepos).contains { $0 == path && $1 }
    }
    /// Whether the computer has git. Nil from an older host: it reads as installed.
    public var gitInstalled: Bool?
    /// How it stands outside its chat (`Bot.Outside`). Nil from an older host: the chat's own status.
    public var outside: Outside?
    public var unread: Int
    /// How many of `unread` are cards waiting on the user: the badge turns amber. 0 from an older host.
    public var waiting: Int = 0
    /// The messages in the badge the user hasn't read: what `markRead` clears (waiting cards stay counted).
    public var unseen: Int { unread - waiting }
    public var lastMessage: String?
    public var lastAt: Int64

    public var isGroup: Bool { kind == "group" }
    /// Counted and listed by the widgets and the Control Center control: a group room isn't a bot.
    public var showsInWidgets: Bool { !hidden && !isGroup }
    public var isWorking: Bool { status == "working" || status == "needsInput" }
    /// Working right now in this chat (not in a group it's in, or the reverse).
    public func isWorking(in chat: String) -> Bool {
        isWorking && (workingChat ?? id) == chat
    }
    public var needsInput: Bool { status == "needsInput" }
    /// Its band in the list: pinned bots, then the rest. Dragging stays inside a band.
    public var band: Int { pinned ? 0 : 1 }
    /// Where it works, as a list shows it: its default project's name, else its own folder, or (an older host) its
    /// project folder's name.
    public var folderName: String {
        projects?.first?.folderName ?? (managedWorkspace ? L10n.tr("Own folder") : (cwd as NSString).lastPathComponent)
    }
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
        customIcon = try c.decodeIfPresent(CustomIcon.self, forKey: .customIcon)
        backend = try c.decodeIfPresent(String.self, forKey: .backend) ?? "custom"
        command = try c.decodeIfPresent(String.self, forKey: .command)
        loginProfile = try? c.decodeIfPresent(String.self, forKey: .loginProfile)
        cwd = try c.decodeIfPresent(String.self, forKey: .cwd) ?? ""
        managedWorkspace = try c.decodeIfPresent(Bool.self, forKey: .managedWorkspace) ?? false
        permission = try c.decodeIfPresent(String.self, forKey: .permission) ?? "ask"
        mode = try c.decodeIfPresent(String.self, forKey: .mode)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        modelOptions = try c.decodeIfPresent([String: String].self, forKey: .modelOptions) ?? [:]
        fastMode = try c.decodeIfPresent(Bool.self, forKey: .fastMode) ?? false
        fastModeSupport = try c.decodeIfPresent(String.self, forKey: .fastModeSupport) ?? "unknown"
        fastModeIssue = try c.decodeIfPresent(String.self, forKey: .fastModeIssue)
        // A chain that doesn't decode stays unknown (nil), so saving the settings never sends an empty one.
        fallbacks = try? c.decodeIfPresent([BotFallback].self, forKey: .fallbacks)
        selfFallbacks = try? c.decodeIfPresent(Bool.self, forKey: .selfFallbacks)
        // Advisor mode that doesn't decode stays unknown (nil), like a chain, so saving the settings never turns it off.
        do {
            threadAgent = try c.decodeIfPresent(BotFallback.self, forKey: .threadAgent)
            threadFallbacks = try c.decodeIfPresent([BotFallback].self, forKey: .threadFallbacks)
        } catch {
            threadAgent = nil
            threadFallbacks = nil
        }
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        lead = try c.decodeIfPresent(Bool.self, forKey: .lead) ?? false
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
        section = try c.decodeIfPresent(String.self, forKey: .section)
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
        compaction = try c.decodeIfPresent(CompactionThreshold.self, forKey: .compaction)
        compactionTokens = try? c.decodeIfPresent(TokenRange.self, forKey: .compactionTokens)
        memoryRev = try c.decodeIfPresent(Int64.self, forKey: .memoryRev)
        autoName = try c.decodeIfPresent(Bool.self, forKey: .autoName) ?? false
        plain = (try? c.decodeIfPresent(Bool.self, forKey: .plain)) ?? false
        createdAt = try c.decodeIfPresent(Int64.self, forKey: .createdAt) ?? 0
        rev = try c.decodeIfPresent(Int64.self, forKey: .rev) ?? 0
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? "idle"
        activity = try c.decodeIfPresent(String.self, forKey: .activity) ?? ""
        startedAt = try c.decodeIfPresent(Int64.self, forKey: .startedAt)
        workingChat = try c.decodeIfPresent(String.self, forKey: .workingChat)
        agentStep = try? c.decodeIfPresent(AgentStep.self, forKey: .agentStep)
        resting = (try? c.decodeIfPresent([AgentRest].self, forKey: .resting)) ?? []
        agentCommands = (try? c.decodeIfPresent([AgentCommand].self, forKey: .agentCommands)) ?? []
        devServers = (try? c.decodeIfPresent([DevServer].self, forKey: .devServers)) ?? []
        work = (try? c.decodeIfPresent(WorkList.self, forKey: .work))?.tasks ?? []
        // Hosts before 2.51.0 (`maxAgents`, `taskWorktrees`): goes when minHost reaches 2.51.0.
        maxThreads = ((try? c.decodeIfPresent(Int.self, forKey: .maxThreads)) ?? OldHostWire.value(Int.self, "maxAgents", from: decoder)).map { min(max($0, Bot.maxThreadsRange.lowerBound), Bot.maxThreadsRange.upperBound) }
        threadWorktrees = (try? c.decodeIfPresent(Bool.self, forKey: .threadWorktrees)) ?? OldHostWire.value(Bool.self, "taskWorktrees", from: decoder)
        gitRepo = (try? c.decodeIfPresent(Bool.self, forKey: .gitRepo)) ?? false
        projects = try? c.decodeIfPresent([String].self, forKey: .projects)
        projectGitRepos = (try? c.decodeIfPresent([Bool].self, forKey: .projectGitRepos)) ?? []
        worktreeMode = try? c.decodeIfPresent(WorktreeMode.self, forKey: .worktreeMode)
        sharedFolders = try? c.decodeIfPresent([String: [String]].self, forKey: .sharedFolders)
        gitInstalled = try? c.decodeIfPresent(Bool.self, forKey: .gitInstalled)
        outside = try? c.decodeIfPresent(Outside.self, forKey: .outside)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        waiting = try c.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
        lastMessage = try c.decodeIfPresent(String.self, forKey: .lastMessage)
        lastAt = try c.decodeIfPresent(Int64.self, forKey: .lastAt) ?? createdAt
    }
}

/// When a bot's threads get a git worktree of their own, in a project that is a git repository.
public enum WorktreeMode: String, Codable, CaseIterable, Sendable {
    case off, sideBySide, always

    /// Its name, a phrase to look up.
    public var title: String {
        switch self {
        case .off: "Off"
        case .sideBySide: "Side by side"
        case .always: "Always"
        }
    }

    /// What it does, in one line: a phrase to look up.
    public var line: String {
        switch self {
        case .off: "Threads work in the project itself"
        case .sideBySide: "A thread gets its own worktree when another thread works in the same repository"
        case .always: "Every thread works in a worktree of its own; your checkout is never touched"
        }
    }
}

extension String {
    /// A folder path's name: its last component (`/` or `\` separated), or the path itself.
    public var folderName: String {
        split { $0 == "/" || $0 == "\\" }.last.map(String.init) ?? self
    }
}
