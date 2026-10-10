import Foundation

/// One bot-to-bot exchange as seen from the chat of one of the two bots.
public struct BotExchange: Identifiable, Equatable, Sendable {
    public enum Outcome: Equatable, Sendable {
        case pending, done, failed(String)

        public var isFailed: Bool {
            if case .failed = self { true } else { false }
        }
    }

    public let id: String
    /// The bot whose chat holds the notice.
    public let chatId: String
    public let seq: Int64
    public let rev: Int64
    public let createdAt: Int64
    public let source: String
    public let target: String
    public let text: String
    public let reply: String?
    public let outcome: Outcome

    /// A notice with structured message data is a bot exchange; existing notices stay plain.
    public init?(_ entry: Entry) {
        guard entry.kind == "notice", let message = entry.data.botMessage else { return nil }
        id = entry.id
        chatId = entry.botId
        seq = entry.seq
        rev = entry.rev
        createdAt = entry.createdAt
        source = message.sourceBotId
        target = message.targetBotId
        text = message.text
        reply = message.reply
        switch entry.data.status {
        case "completed": outcome = .done
        case "failed", "cancelled": outcome = .failed(message.detail ?? "")
        default: outcome = .pending
        }
    }

    public var isOutgoing: Bool { chatId == source }
    public var peerId: String { isOutgoing ? target : source }
    public var verb: String { isOutgoing ? "Messaged" : "Message from" }
    public var date: Date { Date(milliseconds: createdAt) }

    /// The exchanges with `peer` in a chat, oldest first: what the host returned plus the live
    /// chat, keyed by entry id (the higher `rev` wins).
    public static func conversation(fetched: [Entry], live: [Entry], peer: String) -> [BotExchange] {
        var byId: [String: Entry] = [:]
        for e in fetched + live where e.kind == "notice" {
            if let known = byId[e.id], known.rev > e.rev { continue }
            byId[e.id] = e
        }
        return byId.values.compactMap(BotExchange.init).filter { $0.peerId == peer }.sorted { $0.seq < $1.seq }
    }
}

/// A line of the read-only conversation sheet.
public enum BotConversationRow: Identifiable, Equatable, Sendable {
    case separator(id: String, at: Date)
    case message(id: String, author: String, text: String, showsAuthor: Bool)
    /// `awaiting` is the bot being waited for while the outcome is pending.
    case status(id: String, outcome: BotExchange.Outcome, awaiting: String)

    public var id: String {
        switch self {
        case .separator(let id, _), .message(let id, _, _, _), .status(let id, _, _): id
        }
    }

    /// A gap of more than an hour starts a new time block.
    static let gap: Int64 = 3_600_000

    /// Rows for exchanges oldest first. A separator or status row resets the previous author.
    public static func build(_ exchanges: [BotExchange]) -> [BotConversationRow] {
        var rows: [BotConversationRow] = []
        var previousAuthor: String?
        var previousAt: Int64?
        for x in exchanges {
            if previousAt.map({ x.createdAt - $0 > gap }) ?? true {
                rows.append(.separator(id: "sep-\(x.id)", at: x.date))
                previousAuthor = nil
            }
            previousAt = x.createdAt
            rows.append(.message(id: "\(x.id)-request", author: x.source, text: x.text, showsAuthor: previousAuthor != x.source))
            previousAuthor = x.source
            if let reply = x.reply, !reply.isEmpty {
                rows.append(.message(id: "\(x.id)-reply", author: x.target, text: reply, showsAuthor: previousAuthor != x.target))
                previousAuthor = x.target
            }
            if x.outcome != .done {
                rows.append(.status(id: "\(x.id)-status", outcome: x.outcome, awaiting: x.target))
                previousAuthor = nil
            }
        }
        return rows
    }
}
