import Foundation

/// Long work a bot does away from its chat (the host's `chat/tasks.rs`): one still going as its bot carries it
/// (`bot.tasks`), or one read back with its transcript (`taskEntries`), which may have ended. The chat shows one
/// capsule above the composer; what the words say is decided here, the same on Android and the desktop (`tasks.ts`).
public struct BotTask: Codable, Identifiable, Hashable, Sendable {
    public enum Status: String, Codable, Sendable {
        case starting, working, needsInput, done, failed, stopped

        /// A status a newer host adds reads as working, rather than losing the bot's tasks.
        public init(from decoder: Decoder) throws {
            self = Status(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .working
        }

        public var isLive: Bool { self == .starting || self == .working || self == .needsInput }

        /// The status in words: a phrase to look up.
        public var word: String {
            switch self {
            case .starting: "Starting…"
            case .working: "Working"
            case .needsInput: "Needs you"
            case .done: "Done"
            case .failed: "Failed"
            case .stopped: "Stopped"
            }
        }
    }

    public var id: String
    public var title: String
    public var status: Status
    /// What it is doing now; empty until it says.
    public var activity: String
    public var startedAt: Int64
    /// Only on one read back after it ended.
    public var endedAt: Int64?

    public init(id: String, title: String, status: Status, activity: String = "", startedAt: Int64, endedAt: Int64? = nil) {
        self.id = id
        self.title = title
        self.status = status
        self.activity = activity
        self.startedAt = startedAt
        self.endedAt = endedAt
    }

    /// Lenient: only a task without an id is unreadable.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = (try? c.decodeIfPresent(String.self, forKey: .title)) ?? ""
        status = (try? c.decodeIfPresent(Status.self, forKey: .status)) ?? .working
        activity = (try? c.decodeIfPresent(String.self, forKey: .activity)) ?? ""
        startedAt = (try? c.decodeIfPresent(Int64.self, forKey: .startedAt)) ?? 0
        endedAt = try? c.decodeIfPresent(Int64.self, forKey: .endedAt)
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

public extension BotTask {
    /// What it is doing, in a few words.
    func what(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        switch status {
        case .needsInput: tr("Needs you")
        case .starting: tr("Starting…")
        default: activity.isEmpty ? title : tr(activity)
        }
    }
}

public extension Array where Element == BotTask {
    /// What the tasks are doing: the one task's, or how many there are (and whether any waits for the user).
    func what(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        if count == 1 { return self[0].what(tr) }
        let tasks = L10n.fill(tr("{count} tasks running"), ["count": String(count)])
        return contains { $0.status == .needsInput } ? "\(tasks) · \(tr("Needs you").lowercased())" : tasks
    }

    /// The capsule's line above the composer.
    func capsuleLabel(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        "\(tr("In the background")) · \(what(tr))"
    }

    /// A roster row's preview while the chat itself is idle and tasks go on.
    func rosterPreview(_ tr: (String) -> String = { L10n.tr($0) }) -> String {
        "\(tr("In the background")): \(what(tr))"
    }

    /// When the oldest of them started: the capsule's time.
    var since: Int64? { map(\.startedAt).min() }
}

public extension Entry {
    /// What a task was told (its brief, the bot's `tell_task`): the host wrote it, the user didn't type it.
    var isToldToTask: Bool { kind == "user" && data.from == "host" }
}

public extension Array where Element == Entry {
    /// A task's transcript with more of its entries, oldest first: each entry once, the higher `rev` kept.
    func mergingTaskEntries(_ incoming: [Entry]) -> [Entry] {
        var byId = Dictionary(map { ($0.id, $0) }, uniquingKeysWith: { a, b in b.rev >= a.rev ? b : a })
        for e in incoming where e.rev >= (byId[e.id]?.rev ?? .min) { byId[e.id] = e }
        return byId.values.sorted { ($0.seq, $0.id) < ($1.seq, $1.id) }
    }
}
