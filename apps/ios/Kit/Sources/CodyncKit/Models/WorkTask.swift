import Foundation

/// One piece of work in a bot's box (the host's `chat/work/`), as its bot carries it (`bot.work`): the live and
/// waiting tasks in dispatch order, then the ones that ended in the last day. A thread (with its own session) works it;
/// `thread` names it, for its transcript (`threadEntries`). What the words say is decided here, the same on Android
/// and the desktop.
public struct WorkTask: Codable, Identifiable, Hashable, Sendable {
    public enum Status: String, Codable, Sendable {
        case waiting, working, needsYou, done, failed, stopped

        /// A thread has it now.
        public var isLive: Bool { self == .working || self == .needsYou }
        public var hasEnded: Bool { self == .done || self == .failed || self == .stopped }

        /// The status in words: a phrase to look up.
        public var word: String {
            switch self {
            case .waiting: "Waiting"
            case .working: "Working"
            case .needsYou: "Needs you"
            case .done: "Done"
            case .failed: "Failed"
            case .stopped: "Stopped"
            }
        }
    }

    /// Why a waiting task waits. One that doesn't read (a kind a newer host adds) is left out.
    public struct Wait: Codable, Hashable, Sendable {
        public enum Kind: String, Codable, Sendable {
            /// It starts when a thread is free.
            case next
            /// `ahead` tasks start before it.
            case behind
            /// It follows a task a thread still works on.
            case after
            /// The agent is near its usage limit.
            case usage
        }

        public var kind: Kind
        public var ahead: Int

        public init(_ kind: Kind, ahead: Int = 0) {
            self.kind = kind
            self.ahead = ahead
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            kind = try c.decode(Kind.self, forKey: .kind)
            ahead = (try? c.decodeIfPresent(Int.self, forKey: .ahead)) ?? 0
        }
    }

    /// Where a task came from when the bot didn't add it itself: a run of the routine `id`, or a request from the
    /// bot `id` (nil when the asking bot isn't one of this computer's). One of a kind this app doesn't know reads
    /// as the bot's own.
    public struct Source: Codable, Hashable, Sendable {
        public enum Kind: String, Codable, Sendable { case routine, request }

        public var kind: Kind
        public var id: String?

        public init(_ kind: Kind, id: String? = nil) {
            self.kind = kind
            self.id = id
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            kind = try c.decode(Kind.self, forKey: .kind)
            id = try? c.decodeIfPresent(String.self, forKey: .id)
        }
    }

    /// A git worktree of the task's own: its branch, and its folder on the computer.
    public struct Worktree: Codable, Hashable, Sendable {
        public var branch: String
        public var path: String

        public init(branch: String, path: String = "") {
            self.branch = branch
            self.path = path
        }

        /// Only one without a branch is unreadable.
        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            branch = try c.decode(String.self, forKey: .branch)
            path = (try? c.decodeIfPresent(String.self, forKey: .path)) ?? ""
        }
    }

    public var id: String
    public var title: String
    public var status: Status
    /// What its thread is doing now; empty until it says.
    public var activity: String
    /// The thread that has or had it; nil before one took it.
    public var thread: String?
    public var wait: Wait?
    /// It gave way to a task that cut in, and starts again from its brief.
    public var cutIn: Bool
    public var createdAt: Int64
    public var startedAt: Int64?
    public var endedAt: Int64?
    /// What its thread reported, or why it could not finish; empty until it ended.
    public var result: String
    /// Nil for one the bot added.
    public var source: Source?
    /// The git worktree its thread works in, while it is still there.
    public var worktree: Worktree?
    /// Its thread takes the user's messages (`WorkThread.swift`). False from an older host.
    public var talk: Bool
    /// Its thread takes files with the user's messages too. False from a host before them, where the composer has no +.
    public var talkFiles: Bool
    /// The folder its thread works in on the computer; nil before a thread took it, and from an older host.
    public var folder: String?
    /// It takes one of the bot's seats, as the host says (one waiting on the user gives its seat up); nil from an
    /// older host, where every live task takes one (`takesSeat`).
    public var seat: Bool?

    public init(id: String, title: String, status: Status, activity: String = "", thread: String? = nil, wait: Wait? = nil,
                cutIn: Bool = false, createdAt: Int64 = 0, startedAt: Int64? = nil, endedAt: Int64? = nil, result: String = "",
                source: Source? = nil, worktree: Worktree? = nil, talk: Bool = false, talkFiles: Bool = false, folder: String? = nil, seat: Bool? = nil) {
        self.id = id
        self.title = title
        self.status = status
        self.activity = activity
        self.thread = thread
        self.wait = wait
        self.cutIn = cutIn
        self.createdAt = createdAt
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.result = result
        self.source = source
        self.worktree = worktree
        self.talk = talk
        self.talkFiles = talkFiles
        self.folder = folder
        self.seat = seat
    }

    /// Lenient, but for what says which task it is and where it stands: one without an id, or with a status this
    /// app doesn't know (a newer host's), is unreadable, and `WorkList` leaves it out.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = (try? c.decodeIfPresent(String.self, forKey: .title)) ?? ""
        status = try c.decode(Status.self, forKey: .status)
        activity = (try? c.decodeIfPresent(String.self, forKey: .activity)) ?? ""
        // Hosts before 2.51.0 (`task`): goes when minHost reaches 2.51.0.
        thread = (try? c.decodeIfPresent(String.self, forKey: .thread)) ?? OldHostWire.value(String.self, "task", from: decoder)
        wait = try? c.decodeIfPresent(Wait.self, forKey: .wait)
        cutIn = (try? c.decodeIfPresent(Bool.self, forKey: .cutIn)) ?? false
        createdAt = (try? c.decodeIfPresent(Int64.self, forKey: .createdAt)) ?? 0
        startedAt = try? c.decodeIfPresent(Int64.self, forKey: .startedAt)
        endedAt = try? c.decodeIfPresent(Int64.self, forKey: .endedAt)
        result = (try? c.decodeIfPresent(String.self, forKey: .result)) ?? ""
        source = try? c.decodeIfPresent(Source.self, forKey: .source)
        worktree = try? c.decodeIfPresent(Worktree.self, forKey: .worktree)
        talk = (try? c.decodeIfPresent(Bool.self, forKey: .talk)) ?? false
        talkFiles = (try? c.decodeIfPresent(Bool.self, forKey: .talkFiles)) ?? false
        folder = try? c.decodeIfPresent(String.self, forKey: .folder)
        seat = try? c.decodeIfPresent(Bool.self, forKey: .seat)
    }

    /// It takes one of the bot's seats (`maxThreads`): the host's word, else a live task.
    public var takesSeat: Bool { seat ?? status.isLive }

    /// Try again is offered on a task that failed or was stopped, unless another bot asked for it: that bot has
    /// its answer already. A routine's task runs the routine again.
    public var canRetry: Bool { (status == .failed || status == .stopped) && source?.kind != .request }

    /// A thread has it and it may give way to a task that cuts in: only work the bot added. A routine's run or
    /// another bot's request is stopped, never set aside.
    public var givesWay: Bool { status.isLive && source == nil }
}

/// A bot's `work` as the host sent it, each task that doesn't read left out.
struct WorkList: Decodable {
    var tasks: [WorkTask]

    private struct Slot: Decodable {
        var task: WorkTask?
        init(from decoder: Decoder) throws { task = try? WorkTask(from: decoder) }
    }

    init(from decoder: Decoder) throws {
        tasks = try decoder.singleValueContainer().decode([Slot].self).compactMap(\.task)
    }
}

public extension WorkTask {
    /// What a live task is doing, in a few words.
    func what(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        if status == .needsYou { return tr("Needs you") }
        return activity.isEmpty ? title : tr(activity)
    }

    /// Why a waiting task waits.
    func waitLine(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        if cutIn { return tr("Was cut in on; starts again") }
        switch wait?.kind {
        case .behind where (wait?.ahead ?? 0) > 0: return L10n.fill(tr("{count} ahead"), ["count": String(wait?.ahead ?? 0)])
        case .next, .behind: return tr("Next up")
        case .after: return tr("Follows what it works on")
        case .usage: return tr("Near the usage limit: one at a time")
        case nil: return tr("Waiting")
        }
    }

    /// How an ended task ended: what it reported, else the status in words.
    func endLine(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        status == .done || result.isEmpty ? tr(status.word) : result
    }

    /// A task's second line: why it waits, what its thread does, or how it ended.
    func line(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        switch status {
        case .needsYou: tr("Needs you")
        case .working: tr(activity.isEmpty ? "Working" : activity)
        case .waiting: waitLine(tr)
        case .done, .failed, .stopped: endLine(tr)
        }
    }

    /// How long its thread had it, once it ended.
    var timeTaken: Duration? {
        guard let startedAt, let endedAt else { return nil }
        return .milliseconds(max(0, endedAt - startedAt))
    }
}

/// What the user can do to a task from its row.
public enum WorkAction: String, Sendable {
    case stop, first, cutIn, drop, retry

    /// A phrase to look up.
    public var title: String {
        switch self {
        case .stop: "Stop"
        case .first: "First"
        case .cutIn: "Cut in"
        case .drop: "Drop"
        case .retry: "Try again"
        }
    }

    public var symbol: String {
        switch self {
        case .stop: "stop.fill"
        case .first: "arrow.up.to.line"
        case .cutIn: "arrow.turn.right.up"
        case .drop: "xmark"
        case .retry: "arrow.clockwise"
        }
    }
}

/// A bot's tasks as its details list them: Threads (what needs the user, then what is worked on), the event box
/// (what waits), Recent.
public struct WorkSections: Equatable, Sendable {
    public var needsYou: [WorkTask]
    public var working: [WorkTask]
    public var waiting: [WorkTask]
    public var recent: [WorkTask]

    /// Each group keeps the order the host sent.
    public init(_ tasks: [WorkTask]) {
        needsYou = tasks.filter { $0.status == .needsYou }
        working = tasks.filter { $0.status == .working }
        waiting = tasks.filter { $0.status == .waiting }
        recent = tasks.filter(\.status.hasEnded)
    }

    /// The tasks a thread has now: each has one of the bot's threads.
    public var live: [WorkTask] { needsYou + working }
    /// Nothing live or waiting: no capsule.
    public var isIdle: Bool { live.isEmpty && waiting.isEmpty }
    /// The one working task with nothing else going on: the capsule says what it does, and since when.
    public var solo: WorkTask? { needsYou.isEmpty && waiting.isEmpty && working.count == 1 ? working[0] : nil }
    /// The one live task: the capsule's Stop is for it.
    public var stoppable: WorkTask? { live.count == 1 ? live[0] : nil }

    /// How many more threads the bot may run. A host that doesn't say how many it runs at once: three.
    public func free(maxThreads: Int?) -> Int {
        max(0, (maxThreads ?? Bot.defaultMaxThreads) - inUse)
    }

    /// The seats taken: a thread waiting on the user takes none.
    public var inUse: Int { live.filter(\.takesSeat).count }

    /// Every thread is busy: a task starts now only when a live one gives way.
    public func isFull(maxThreads: Int?) -> Bool {
        !live.isEmpty && free(maxThreads: maxThreads) == 0
    }

    /// The live tasks that may give way to one that cuts in: what the cut-in dialog lists.
    public var givingWay: [WorkTask] { live.filter(\.givesWay) }

    /// Whether the first waiting task may cut in: every thread is busy, a task may give way, and a freed thread
    /// would start it (not one that follows unfinished work, or waits for usage).
    public func canCutIn(maxThreads: Int?) -> Bool {
        guard let first = waiting.first else { return false }
        return isFull(maxThreads: maxThreads) && !givingWay.isEmpty && first.wait?.kind != .after && first.wait?.kind != .usage
    }

    /// The Threads heading's `2 / 3`: the seats taken, of the most allowed.
    public func threadsCount(maxThreads: Int?) -> String {
        "\(inUse) / \(maxThreads ?? Bot.defaultMaxThreads)"
    }

    /// What a task's row offers, the one that shows on the row first.
    public func actions(for task: WorkTask, maxThreads: Int?) -> [WorkAction] {
        switch task.status {
        case .working, .needsYou: [.stop]
        case .waiting where task.id == waiting.first?.id: canCutIn(maxThreads: maxThreads) ? [.cutIn, .drop] : [.drop]
        case .waiting: [.first, .drop]
        case .failed, .stopped: task.canRetry ? [.retry, .drop] : [.drop]
        case .done: [.drop]
        }
    }

    /// `2 working · 3 waiting`, a part with none left out.
    public func counts(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        [("{count} working", working.count), ("{count} waiting", waiting.count)]
            .filter { $0.1 > 0 }
            .map { L10n.fill(tr($0.0), ["count": String($0.1)]) }
            .joined(separator: " · ")
    }

    /// The capsule's line above the composer.
    public func capsuleLabel(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        let said = needsYou.isEmpty ? [solo?.what(tr) ?? counts(tr)] : [tr("Needs you"), counts(tr)]
        return ([tr("Work")] + said).filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// All bots' work in one view: the roster's rows whose bot has a live or waiting task, in roster order, and how
/// many tasks are worked on in all (the count on the view's button).
public struct AllWork<Row> {
    public var bots: [Row]
    public var working: Int

    public init(_ roster: [Row], work: (Row) -> [WorkTask]) {
        bots = roster.filter { !WorkSections(work($0)).isIdle }
        working = bots.reduce(0) { $0 + work($1).count { $0.status == .working } }
    }
}

public extension Bot {
    /// How many threads a bot runs at once unless the user changed it.
    static let defaultMaxThreads = 3
    static let maxThreadsRange = 1...6

    /// What a roster row says under the bot's name, the most pressing thing first.
    enum RosterLine: Equatable, Sendable {
        /// The chat or one of its tasks waits on the user.
        case needsYou
        /// The chat's own turn.
        case activity
        /// Its work: `WorkSections.counts`.
        case work
        case lastMessage
    }

    /// What the row says when `rosterLine` is `.needsYou` (a phrase to look up): the chat's own request, else that
    /// a task waits on the user.
    var needsYouLine: String {
        guard needsInput else { return "Needs you" }
        return activity.isEmpty ? "Needs approval" : activity
    }

    /// How a bot stands outside its chat (the Live Activity, the widgets): it needs the user while the chat or one
    /// of its tasks waits on them, and works while the chat works or its box holds anything that isn't over.
    struct Outside: Codable, Hashable, Sendable {
        /// idle | working | needsInput | error
        public var status: String
        /// The chat turn's start, else the earliest open task's.
        public var startedAt: Int64?

        public init(status: String, startedAt: Int64? = nil) {
            self.status = status
            self.startedAt = startedAt
        }

        /// Only one without a status is unreadable.
        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            status = try c.decode(String.self, forKey: .status)
            startedAt = try? c.decodeIfPresent(Int64.self, forKey: .startedAt)
        }
    }

    /// `outside`, or the chat's own from a host that doesn't send it.
    var outsideStatus: String { outside?.status ?? status }
    var outsideStartedAt: Int64? { outside == nil ? startedAt : outside?.startedAt }
    var needsYouOutside: Bool { outsideStatus == "needsInput" }
    /// Something goes on, in its chat or its box: what keeps its Live Activity up.
    var isBusyOutside: Bool { outsideStatus == "working" || needsYouOutside }

    /// The one line beside the bot outside its chat, in the app's language: `Needs you` when a task waits on the
    /// user, else what its chat does, else how many tasks work and wait. Empty when nothing goes on.
    func outsideLine(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        let work = WorkSections(work)
        switch rosterLine {
        case .needsYou where !work.needsYou.isEmpty: return tr("Needs you")
        case .needsYou, .activity: return tr(activity)
        case .work: return work.counts(tr)
        case .lastMessage: return ""
        }
    }

    var rosterLine: RosterLine {
        let work = WorkSections(work)
        if needsInput || !work.needsYou.isEmpty { return .needsYou }
        if isWorking { return .activity }
        return work.isIdle ? .lastMessage : .work
    }
}

/// The thread a permission card is for (`data.thread`). Anything else in its place reads as no thread.
public struct ThreadRef: Codable, Hashable, Sendable {
    public var id: String
    public var title: String

    public init(from decoder: Decoder) throws {
        let c = try? decoder.container(keyedBy: CodingKeys.self)
        id = (try? c?.decodeIfPresent(String.self, forKey: .id)) ?? ""
        title = (try? c?.decodeIfPresent(String.self, forKey: .title)) ?? ""
    }
}

/// The task a permission card is for (`data.task`). Anything else in its place reads as no task.
public struct TaskRef: Codable, Hashable, Sendable {
    public var id: String

    public init(from decoder: Decoder) throws {
        id = (try? decoder.singleValueContainer().decode(String.self)) ?? ""
    }

    /// As it came, so the app's own copy reads back.
    public func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        try value.encode(id)
    }
}

public extension Entry {
    /// What a card asks: an approval's title, a choice's question, the agent a sign-in waits on.
    var ask: String? {
        if let request = data.connectionRequest {
            return L10n.fill(L10n.tr("Needs {agent} sign-in"), ["agent": request.title])
        }
        return data.title ?? data.question
    }

    /// A sign-in card that holds messages: signing in there is what sends them again.
    fileprivate var holdsMessages: Bool { data.connectionRequest?.entryIds?.isEmpty == false }

    /// A card waiting in the chat for the user: an approval, a choice, or a sign-in an agent needs.
    fileprivate var isPendingCard: Bool {
        if let request = data.connectionRequest { return request.kind == "agent" && request.status == "pending" }
        return (kind == "permission" || data.decision == true) && data.status == "pending"
    }
}

public extension Array where Element == Entry {
    /// A thread's transcript with more of its entries, oldest first: each entry once, the higher `rev` kept. The
    /// computer's copy of a message sent from here (the same `clientNonce`) replaces the one shown at once.
    func mergingThreadEntries(_ incoming: [Entry]) -> [Entry] {
        var byId = Dictionary(map { ($0.id, $0) }, uniquingKeysWith: { a, b in b.rev >= a.rev ? b : a })
        for e in incoming where e.rev >= (byId[e.id]?.rev ?? .min) { byId[e.id] = e }
        for e in byId.values where e.kind == "user" {
            guard let nonce = e.data.clientNonce, !nonce.isEmpty, e.id != Entry.localId(nonce) else { continue }
            byId[Entry.localId(nonce)] = nil
        }
        return byId.values.sorted { ($0.seq, $0.id) < ($1.seq, $1.id) }
    }

    /// The card in a bot's chat that a task waits on (an approval, a choice its thread asks, a sign-in its agent
    /// needs): the newest pending one that names it, or, when that is a sign-in and another (the agent the turn left
    /// signed out, while its fallback was too) holds the task's message, that one, whose sign-in sends it again.
    func card(for task: WorkTask) -> Entry? {
        let cards = filter { e in
            e.isPendingCard && (e.data.task?.id == task.id || (task.thread != nil && e.data.thread?.id == task.thread))
        }
        guard let newest = cards.last else { return nil }
        return newest.data.connectionRequest == nil ? newest : cards.last(where: \.holdsMessages) ?? newest
    }
}
