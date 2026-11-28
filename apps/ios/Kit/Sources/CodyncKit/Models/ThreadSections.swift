import Foundation

/// How the Threads tab groups what doesn't need the user (the desktop's `threadsGroup`): under the folder (or routine)
/// a thread works for, under its bot, under its computer, or not at all.
public enum ThreadGrouping: String, CaseIterable, Sendable {
    case folder, bot, computer, none
}

/// All bots' work as the Threads tab lists it, the desktop's `threadSections`: what needs the user on top, then the
/// groups `grouping` makes (a folder a thread works in outside its bot's own, a worktree of it being the bot's; a
/// routine's runs; a group per bot in the roster's order, which is the bot list's; or one per computer, in the order
/// the computers come), then the rest. Newest first inside each, or as the sort options say.
public struct ThreadSections<Row> {
    public struct Item: Identifiable {
        public let of: Row
        public let task: WorkTask
        /// The row's bot and the task: task ids are only unique within a bot.
        public let id: String
    }

    public struct Group {
        public enum Kind: CaseIterable, Sendable { case folder, routine, bot, computer }

        public let key: String
        public let kind: Kind
        /// The folder's last name, the routine's, the bot's or the computer's.
        public let title: String
        public internal(set) var rows: [Item] = []
    }

    public let needsYou: [Item]
    public let groups: [Group]
    public let recent: [Item]

    /// `key` tells the rows apart, `computer` names a row's computer. `byStatus`: working first, then waiting, then
    /// the ended ones; `byActivity`: by when each last moved (ended, else started, else added) instead of when it was
    /// added.
    public init(_ roster: [Row], grouping: ThreadGrouping, byStatus: Bool = false, byActivity: Bool = false,
                key: (Row) -> String, computer: (Row) -> String, bot: (Row) -> Bot) {
        let time: (WorkTask) -> Int64 = byActivity ? { $0.endedAt ?? $0.startedAt ?? $0.createdAt } : { $0.createdAt }
        // Stable: rows alike keep the roster's order.
        func newest(_ items: [Item]) -> [Item] {
            items.enumerated().sorted { a, b in
                let ranks = byStatus ? (Self.rank(a.element.task.status), Self.rank(b.element.task.status)) : (0, 0)
                let times = (time(a.element.task), time(b.element.task))
                if ranks.0 != ranks.1 { return ranks.0 < ranks.1 }
                if times.0 != times.1 { return times.0 > times.1 }
                return a.offset < b.offset
            }.map(\.element)
        }
        let rows = roster.flatMap { of in bot(of).work.map { Item(of: of, task: $0, id: "\(key(of))/\($0.id)") } }
        needsYou = newest(rows.filter { $0.task.status == .needsYou })
        var found: [String: Group] = [:]
        var order: [String] = []
        var recent: [Item] = []
        for row in newest(rows.filter { $0.task.status != .needsYou }) {
            guard let group = Self.group(of: row, grouping, key: key, computer: computer, bot: bot) else {
                recent.append(row)
                continue
            }
            if found[group.key] == nil {
                found[group.key] = group
                order.append(group.key)
            }
            found[group.key]?.rows.append(row)
        }
        // A bot's group keeps the bot list's order, a computer's the computers'; the others come as their newest row.
        let keys = switch grouping {
        case .bot: roster.map { "bot:\(key($0))" }
        case .computer: roster.map { "computer:\(computer($0))" }
        case .folder, .none: order
        }
        var seen: Set<String> = []
        groups = keys.filter { seen.insert($0).inserted }.compactMap { found[$0] }
        self.recent = recent
    }

    /// The Threads tab's search, the desktop's `threadItems`: the tasks that have a thread whose title or bot's name
    /// (not the box without a bot's) has `query`, newest first, flat.
    public static func found(_ roster: [Row], matching query: String, key: (Row) -> String, bot: (Row) -> Bot) -> [Item] {
        let rows = roster.flatMap { of in
            bot(of).work.filter { $0.thread != nil }.map { Item(of: of, task: $0, id: "\(key(of))/\($0.id)") }
        }
        return rows.filter { item in
            let owner = bot(item.of)
            return item.task.title.localizedStandardContains(query) || (!owner.plain && owner.name.localizedStandardContains(query))
        }
        .enumerated().sorted { ($1.element.task.createdAt, $0.offset) < ($0.element.task.createdAt, $1.offset) }.map(\.element)
    }

    /// How many tasks are worked on now: the Threads tab's badge.
    public static func working(in roster: [Row], bot: (Row) -> Bot) -> Int {
        roster.reduce(0) { $0 + bot($1).work.count { $0.status == .working } }
    }

    private static func rank(_ status: WorkTask.Status) -> Int {
        switch status {
        case .working: 0
        case .waiting: 1
        default: 2
        }
    }

    private static func group(of row: Item, _ grouping: ThreadGrouping, key: (Row) -> String, computer: (Row) -> String,
                              bot: (Row) -> Bot) -> Group? {
        let task = row.task, owner = bot(row.of)
        switch grouping {
        case .bot: return Group(key: "bot:\(key(row.of))", kind: .bot, title: owner.name)
        case .computer: return Group(key: "computer:\(computer(row.of))", kind: .computer, title: computer(row.of))
        case .none: return nil
        case .folder:
            if task.source?.kind == .routine {
                return Group(key: "routine:\(key(row.of)):\(task.source?.id ?? task.title)", kind: .routine, title: task.title)
            }
            // Every thread in a folder Codync made for it, in one group.
            if task.ownFolder { return Group(key: "folder:own", kind: .folder, title: L10n.tr("Own folder")) }
            // A thread in its own worktree, or in its bot's folder, works where its bot does.
            guard task.worktree == nil, let path = task.wherePath, path != owner.cwd else { return nil }
            return Group(key: "folder:\(path)", kind: .folder, title: path.folderName)
        }
    }
}
