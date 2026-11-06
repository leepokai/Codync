import Foundation

/// The last days at a glance, counted by the host from the computer's transcripts (`weekSummary`). No message text.
public struct WeekSummary: Decodable, Hashable, Sendable {
    public struct TopBot: Decodable, Hashable, Sendable {
        public var id: String
        public var name: String
        public var entries: Int
    }

    public var messages = 0
    public var replies = 0
    public var tools = 0
    public var fileEdits = 0
    public var approvalsAsked = 0
    public var approvalsAnswered = 0
    public var approvalsWaiting = 0
    public var activeBots = 0
    public var topBots: [TopBot] = []

    public init() {}

    /// Lenient: a count the host leaves out is zero.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        messages = try c.decodeIfPresent(Int.self, forKey: .messages) ?? 0
        replies = try c.decodeIfPresent(Int.self, forKey: .replies) ?? 0
        tools = try c.decodeIfPresent(Int.self, forKey: .tools) ?? 0
        fileEdits = try c.decodeIfPresent(Int.self, forKey: .fileEdits) ?? 0
        approvalsAsked = try c.decodeIfPresent(Int.self, forKey: .approvalsAsked) ?? 0
        approvalsAnswered = try c.decodeIfPresent(Int.self, forKey: .approvalsAnswered) ?? 0
        approvalsWaiting = try c.decodeIfPresent(Int.self, forKey: .approvalsWaiting) ?? 0
        activeBots = try c.decodeIfPresent(Int.self, forKey: .activeBots) ?? 0
        topBots = try c.decodeIfPresent([TopBot].self, forKey: .topBots) ?? []
    }
}
