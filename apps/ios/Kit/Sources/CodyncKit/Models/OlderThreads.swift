import Foundation

/// An ended thread older than its bot's `work` carries (`olderThreads`): a `work` row and the bot whose box it is in.
public struct OlderThread: Decodable, Hashable, Sendable {
    public var botId: String
    public var task: WorkTask

    private enum CodingKeys: String, CodingKey { case botId }

    public init(botId: String, task: WorkTask) {
        self.botId = botId
        self.task = task
    }

    public init(from decoder: Decoder) throws {
        botId = try decoder.container(keyedBy: CodingKeys.self).decode(String.self, forKey: .botId)
        task = try WorkTask(from: decoder)
    }
}

/// One page of `olderThreads`, newest first, each row that doesn't read left out.
public struct OlderThreadsPage: Decodable, Sendable {
    public var threads: [OlderThread]
    public var more: Bool

    private enum CodingKeys: String, CodingKey { case threads, more }

    private struct Slot: Decodable {
        var row: OlderThread?
        init(from decoder: Decoder) throws { row = try? OlderThread(from: decoder) }
    }

    public init(threads: [OlderThread], more: Bool) {
        self.threads = threads
        self.more = more
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        threads = try c.decode([Slot].self, forKey: .threads).compactMap(\.row)
        more = try c.decode(Bool.self, forKey: .more)
    }
}

/// A computer's older threads as far as the Threads list has paged them.
public struct OlderThreadList: Equatable, Sendable {
    /// The pages the host sent; the next asks before the oldest of them.
    public private(set) var rows: [OlderThread] = []
    /// Ended threads that left their bot's `work` (past its last day) while connected, as they last read.
    public private(set) var kept: [OlderThread] = []
    /// Another page may follow.
    public private(set) var more = true
    /// Threads dropped here: their bot's next `work` without them doesn't keep them.
    private var dropped: Set<String> = []

    public init() {}

    /// Where the next page starts: the oldest `endedAt` held, nil for the first page.
    public var cursor: Int64? { rows.compactMap(\.task.endedAt).min() }

    /// A page that doesn't reach further back ends the paging, so a host that repeats itself never loops.
    public mutating func add(_ page: OlderThreadsPage) {
        let before = cursor
        rows += page.threads
        more = page.more && cursor != nil && cursor != before
    }

    /// `botId`'s `work` went from `was` to `now`: an ended thread it no longer carries stays listed, as it last read.
    public mutating func keep(_ botId: String, was: [WorkTask], now: [WorkTask]) {
        let carried = Set(now.map(\.id))
        for task in was where task.endedAt != nil && !carried.contains(task.id) && !dropped.contains("\(botId)/\(task.id)") {
            kept.removeAll { $0.botId == botId && $0.task.id == task.id }
            kept.append(OlderThread(botId: botId, task: task))
        }
    }

    /// A thread dropped (`dropWork`): out of the list, and not kept once it leaves its bot's `work`.
    public mutating func remove(_ taskId: String, of botId: String) {
        dropped.insert("\(botId)/\(taskId)")
        rows.removeAll { $0.botId == botId && $0.task.id == taskId }
        kept.removeAll { $0.botId == botId && $0.task.id == taskId }
    }

    /// `botId`'s older threads by bot, for `WorkTask.merging`: a kept one first, over its paged row.
    public var byBot: [String: [WorkTask]] { Dictionary(grouping: kept + rows, by: \.botId).mapValues { $0.map(\.task) } }
}

public extension WorkTask {
    /// A bot's `work`, then its older threads, each task once: the bot's own copy wins.
    static func merging(_ work: [WorkTask], older: [WorkTask]) -> [WorkTask] {
        var seen = Set<String>()
        return (work + older).filter { seen.insert($0.id).inserted }
    }
}
