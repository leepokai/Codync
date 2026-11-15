import Foundation

/// The chats the share sheet offers: the ones sent to lately first, then every computer's bots in roster order.
/// Hidden bots, and groups with no members yet, are left out.
public struct ShareTargets: Sendable {
    public struct Row: Identifiable, Sendable {
        public let recipient: ShareRecipient
        public let bot: Bot
        /// A group's members, for its avatar.
        public let members: [Bot]
        public let computerName: String
        public var id: ShareRecipient { recipient }
    }

    public struct Section: Identifiable, Sendable {
        /// The computer's name; nil when the account has only one computer.
        public let title: String?
        public let computerId: ComputerID
        public let rows: [Row]
        public var id: ComputerID { computerId }
    }

    public let recent: [Row]
    public let sections: [Section]
    /// More than one computer: rows say whose bot it is.
    public let manyComputers: Bool

    public var isEmpty: Bool { recent.isEmpty && sections.isEmpty }

    public init(computers: [Computer], order: ComputerOrder?, bots: [BotSnapshot], recent: [ShareRecipient]) {
        let ordered = (order ?? ComputerOrder()).orderedIDs(computers.map(\.id)).compactMap { id in computers.first { $0.id == id } }
        let rows = ordered.map { computer in
            let own = bots.filter { $0.computerId == computer.id }.map(\.bot)
            return own.filter { !$0.hidden && (!$0.isGroup || !$0.members.isEmpty) }.map { bot in
                Row(recipient: ShareRecipient(computerId: computer.id, botId: bot.id), bot: bot,
                    members: bot.members.compactMap { id in own.first { $0.id == id } }, computerName: computer.name)
            }
        }
        let all = rows.flatMap(\.self)
        let recentRows = recent.compactMap { recipient in all.first { $0.recipient == recipient } }
        let shown = Set(recentRows.map(\.recipient))
        manyComputers = ordered.count > 1
        self.recent = recentRows
        sections = zip(ordered, rows).compactMap { computer, rows in
            let rest = rows.filter { !shown.contains($0.recipient) }
            return rest.isEmpty ? nil : Section(title: ordered.count > 1 ? computer.name : nil, computerId: computer.id, rows: rest)
        }
    }
}
