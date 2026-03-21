import Foundation

#if os(iOS)
import ActivityKit

public struct OverallAttributes: ActivityAttributes, Codable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let sessions: [SessionSummary]
        public let primarySessionId: String?
        public let totalCost: Double

        public init(sessions: [SessionSummary], primarySessionId: String?, totalCost: Double) {
            self.sessions = sessions
            self.primarySessionId = primarySessionId
            self.totalCost = totalCost
        }
    }

    public init() {}
}

#else

public struct OverallAttributes: Codable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let sessions: [SessionSummary]
        public let primarySessionId: String?
        public let totalCost: Double

        public init(sessions: [SessionSummary], primarySessionId: String?, totalCost: Double) {
            self.sessions = sessions
            self.primarySessionId = primarySessionId
            self.totalCost = totalCost
        }
    }

    public init() {}
}

#endif
