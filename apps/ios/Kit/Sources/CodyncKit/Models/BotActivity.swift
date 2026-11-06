import ActivityKit
import Foundation

/// A voice call with the bot this phone is on, as its Live Activity shows it.
public enum CallState: String, Codable, Hashable, Sendable {
    case connecting, listening, speaking, paused
}

public extension BotActivityAttributes {
    /// The in-app switch for Live Activities (State tab › Live Activity).
    static let enabledKey = "liveActivitiesEnabled"

    static var isOn: Bool { UserDefaults.standard.object(forKey: enabledKey) as? Bool ?? true }

    /// Whether a Live Activity may start: the in-app switch and iOS's.
    static var mayStart: Bool { isOn && ActivityAuthorizationInfo().areActivitiesEnabled }

    /// The bot's Live Activity on screen, not one that ended.
    static func shown(botId: String, computerId: ComputerID) -> Activity<Self>? {
        Activity<Self>.activities.first {
            $0.attributes.botId == botId && $0.attributes.computerId == computerId
                && ($0.activityState == .active || $0.activityState == .stale)
        }
    }
}

/// Live Activity for a bot working on something the user just delegated.
/// Content state keys must match what codync-host pushes (host/src/remote/push.rs).
public struct BotActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable, Sendable {
        /// idle | working | needsInput | error
        public var status: String
        /// Local app updates may show the current step; remote pushes leave it empty (§6.7).
        public var activity: String
        public var startedAt: Date?
        /// A voice call with the bot this phone is on. Set by the app only.
        public var call: CallState?
        /// When that call started, for its length.
        public var callStartedAt: Date?
        /// That call's microphone is muted (its Live Activity's mute button).
        public var callMuted: Bool?

        public init(status: String, activity: String, startedAt: Date?, call: CallState? = nil, callStartedAt: Date? = nil,
                    callMuted: Bool? = nil) {
            self.status = status
            self.activity = activity
            self.startedAt = startedAt
            self.call = call
            self.callStartedAt = callStartedAt
            self.callMuted = callMuted
        }
    }

    public var link: URL?
    public var computerId: ComputerID
    public var botId: String
    public var name: String
    public var avatarShape: String
    public var avatarColor: String

    public init(bot: Bot, computerId: ComputerID, link: URL? = nil) {
        self.link = link
        self.computerId = computerId
        botId = bot.id
        name = bot.name
        avatarShape = bot.avatarShape
        avatarColor = bot.avatarColor
    }
}
