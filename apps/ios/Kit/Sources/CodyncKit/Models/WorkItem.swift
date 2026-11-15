import Foundation

/// One piece of work in a bot's box (the host's `chat/tasks/`), as its bot carries it (`bot.work`): the live and
/// waiting items in dispatch order, then the ones that ended in the last day. A task (an agent session) works it;
/// `task` names it, for its transcript (`taskEntries`). What the words say is decided here, the same on Android
/// and the desktop.
public struct WorkItem: Codable, Identifiable, Hashable, Sendable {
    public enum Status: String, Codable, Sendable {
        case waiting, working, needsYou, done, failed, stopped

        /// A task has it now.
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

    /// Why a waiting item waits. One that doesn't read (a kind a newer host adds) is left out.
    public struct Wait: Codable, Hashable, Sendable {
        public enum Kind: String, Codable, Sendable {
            /// It starts when a place is free.
            case next
            /// `ahead` items start before it.
            case behind
            /// It follows an item a task still works on.
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

    public var id: String
    public var title: String
    public var status: Status
    /// What its task is doing now; empty until it says.
    public var activity: String
    /// The task that has or had it; nil before one took it.
    public var task: String?
    public var wait: Wait?
    /// It gave way to an item that cut in, and starts again from its brief.
    public var cutIn: Bool
    public var createdAt: Int64
    public var startedAt: Int64?
    public var endedAt: Int64?
    /// What its task reported, or why it could not finish; empty until it ended.
    public var result: String

    public init(id: String, title: String, status: Status, activity: String = "", task: String? = nil, wait: Wait? = nil,
                cutIn: Bool = false, createdAt: Int64 = 0, startedAt: Int64? = nil, endedAt: Int64? = nil, result: String = "") {
        self.id = id
        self.title = title
        self.status = status
        self.activity = activity
        self.task = task
        self.wait = wait
        self.cutIn = cutIn
        self.createdAt = createdAt
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.result = result
    }

    /// Lenient, but for what says which item it is and where it stands: one without an id, or with a status this
    /// app doesn't know (a newer host's), is unreadable, and `WorkList` leaves it out.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = (try? c.decodeIfPresent(String.self, forKey: .title)) ?? ""
        status = try c.decode(Status.self, forKey: .status)
        activity = (try? c.decodeIfPresent(String.self, forKey: .activity)) ?? ""
        task = try? c.decodeIfPresent(String.self, forKey: .task)
        wait = try? c.decodeIfPresent(Wait.self, forKey: .wait)
        cutIn = (try? c.decodeIfPresent(Bool.self, forKey: .cutIn)) ?? false
        createdAt = (try? c.decodeIfPresent(Int64.self, forKey: .createdAt)) ?? 0
        startedAt = try? c.decodeIfPresent(Int64.self, forKey: .startedAt)
        endedAt = try? c.decodeIfPresent(Int64.self, forKey: .endedAt)
        result = (try? c.decodeIfPresent(String.self, forKey: .result)) ?? ""
    }
}

/// A bot's `work` as the host sent it, each item that doesn't read left out.
struct WorkList: Decodable {
    var items: [WorkItem]

    private struct Slot: Decodable {
        var item: WorkItem?
        init(from decoder: Decoder) throws { item = try? WorkItem(from: decoder) }
    }

    init(from decoder: Decoder) throws {
        items = try decoder.singleValueContainer().decode([Slot].self).compactMap(\.item)
    }
}

public extension WorkItem {
    /// What a live item is doing, in a few words.
    func what(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        if status == .needsYou { return tr("Needs you") }
        return activity.isEmpty ? title : tr(activity)
    }

    /// Why a waiting item waits.
    func waitLine(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        if cutIn { return tr("Was cut in on; starts again") }
        switch wait?.kind {
        case .behind where (wait?.ahead ?? 0) > 0: return L10n.fill(tr("{count} ahead"), ["count": String(wait?.ahead ?? 0)])
        case .next, .behind: return tr("Next up")
        case .after: return tr("Follows what it works on")
        case .usage: return tr("Waiting for usage")
        case nil: return tr("Waiting")
        }
    }

    /// How an ended item ended: what it reported, else the status in words.
    func endLine(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        status == .done || result.isEmpty ? tr(status.word) : result
    }

    /// An item's second line: why it waits, what its task does, or how it ended.
    func line(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        switch status {
        case .needsYou: tr("Needs you")
        case .working: tr(activity.isEmpty ? "Working" : activity)
        case .waiting: waitLine(tr)
        case .done, .failed, .stopped: endLine(tr)
        }
    }

    /// How long its task had it, once it ended.
    var timeTaken: Duration? {
        guard let startedAt, let endedAt else { return nil }
        return .milliseconds(max(0, endedAt - startedAt))
    }
}

/// What the user can do to an item from its row.
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

/// A bot's items as its details list them: Threads (what needs the user, then what is worked on), the event box
/// (what waits), Recent.
public struct WorkSections: Equatable, Sendable {
    public var needsYou: [WorkItem]
    public var working: [WorkItem]
    public var waiting: [WorkItem]
    public var recent: [WorkItem]

    /// Each group keeps the order the host sent.
    public init(_ items: [WorkItem]) {
        needsYou = items.filter { $0.status == .needsYou }
        working = items.filter { $0.status == .working }
        waiting = items.filter { $0.status == .waiting }
        recent = items.filter(\.status.hasEnded)
    }

    /// The items a task has now: each takes one of the bot's places.
    public var live: [WorkItem] { needsYou + working }
    /// Nothing live or waiting: no capsule.
    public var isIdle: Bool { live.isEmpty && waiting.isEmpty }
    /// The one working item with nothing else going on: the capsule says what it does, and since when.
    public var solo: WorkItem? { needsYou.isEmpty && waiting.isEmpty && working.count == 1 ? working[0] : nil }
    /// The one live item: the capsule's Stop is for it.
    public var stoppable: WorkItem? { live.count == 1 ? live[0] : nil }

    /// How many of the bot's places no task has. A host that doesn't say how many it has: three.
    public func free(maxAgents: Int?) -> Int {
        max(0, (maxAgents ?? Bot.defaultMaxAgents) - live.count)
    }

    /// Every place is taken: an item starts now only when a live one gives way.
    public func isFull(maxAgents: Int?) -> Bool {
        !live.isEmpty && free(maxAgents: maxAgents) == 0
    }

    /// Whether the first waiting item may cut in: every place is taken, and a freed one would start it (not one
    /// that follows unfinished work, or waits for usage).
    private func canCutIn(_ item: WorkItem, maxAgents: Int?) -> Bool {
        isFull(maxAgents: maxAgents) && item.wait?.kind != .after && item.wait?.kind != .usage
    }

    /// The Threads heading's `2 / 3`: how many run, of the most allowed.
    public func threadsCount(maxAgents: Int?) -> String {
        "\(live.count) / \(maxAgents ?? Bot.defaultMaxAgents)"
    }

    /// What an item's row offers, the one that shows on the row first.
    public func actions(for item: WorkItem, maxAgents: Int?) -> [WorkAction] {
        switch item.status {
        case .working, .needsYou: [.stop]
        case .waiting where item.id == waiting.first?.id: canCutIn(item, maxAgents: maxAgents) ? [.cutIn, .drop] : [.drop]
        case .waiting: [.first, .drop]
        case .failed, .stopped: [.retry, .drop]
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
        return ([tr("In the background")] + said).filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

public extension Bot {
    /// How many agents work a bot's items at once unless the user changed it.
    static let defaultMaxAgents = 3
    static let maxAgentsRange = 1...6

    /// What a roster row says under the bot's name, the most pressing thing first.
    enum RosterLine: Equatable, Sendable {
        /// The chat or one of its items waits on the user.
        case needsYou
        /// The chat's own turn.
        case activity
        /// Its work in the background: `WorkSections.counts`.
        case work
        case lastMessage
    }

    /// What the row says when `rosterLine` is `.needsYou` (a phrase to look up): the chat's own request, else that
    /// an item waits on the user.
    var needsYouLine: String {
        guard needsInput else { return "Needs you" }
        return activity.isEmpty ? "Needs your approval" : activity
    }

    var rosterLine: RosterLine {
        let work = WorkSections(work)
        if needsInput || !work.needsYou.isEmpty { return .needsYou }
        if isWorking { return .activity }
        return work.isIdle ? .lastMessage : .work
    }
}

/// The task a permission card is for (`data.task`). Anything else in its place reads as no task.
public struct TaskRef: Codable, Hashable, Sendable {
    public var id: String
    public var title: String

    public init(from decoder: Decoder) throws {
        let c = try? decoder.container(keyedBy: CodingKeys.self)
        id = (try? c?.decodeIfPresent(String.self, forKey: .id)) ?? ""
        title = (try? c?.decodeIfPresent(String.self, forKey: .title)) ?? ""
    }
}

/// The item a permission card is for (`data.item`). Anything else in its place reads as no item.
public struct ItemRef: Codable, Hashable, Sendable {
    public var id: String

    public init(from decoder: Decoder) throws {
        id = (try? decoder.singleValueContainer().decode(String.self)) ?? ""
    }
}

public extension Entry {
    /// What a task was told (its item): the host wrote it, the user didn't type it.
    var isToldToTask: Bool { kind == "user" && data.from == "host" }
}

public extension Array where Element == Entry {
    /// A task's transcript with more of its entries, oldest first: each entry once, the higher `rev` kept.
    func mergingTaskEntries(_ incoming: [Entry]) -> [Entry] {
        var byId = Dictionary(map { ($0.id, $0) }, uniquingKeysWith: { a, b in b.rev >= a.rev ? b : a })
        for e in incoming where e.rev >= (byId[e.id]?.rev ?? .min) { byId[e.id] = e }
        return byId.values.sorted { ($0.seq, $0.id) < ($1.seq, $1.id) }
    }

    /// The card in a bot's chat that an item waits on: the newest pending one that names it.
    func card(for item: WorkItem) -> Entry? {
        last { e in
            e.kind == "permission" && e.data.status == "pending"
                && (e.data.item?.id == item.id || (item.task != nil && e.data.task?.id == item.task))
        }
    }
}
