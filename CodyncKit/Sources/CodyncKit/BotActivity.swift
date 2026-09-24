#if os(iOS)
import ActivityKit
import Foundation

/// Live Activity for a bot working on something the user just delegated.
/// Content state keys must match what codync-host pushes (host/src/push.rs).
public struct BotActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable, Sendable {
        /// idle | working | needsInput | error
        public var status: String
        public var activity: String
        public var startedAt: Date?

        public init(status: String, activity: String, startedAt: Date?) {
            self.status = status
            self.activity = activity
            self.startedAt = startedAt
        }
    }

    public var botId: String
    public var name: String
    public var avatarShape: String
    public var avatarColor: String

    public init(bot: Bot) {
        botId = bot.id
        name = bot.name
        avatarShape = bot.avatarShape
        avatarColor = bot.avatarColor
    }
}
#endif
