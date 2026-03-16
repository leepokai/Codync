import Foundation

#if os(iOS)
import ActivityKit

public struct CodePulseAttributes: ActivityAttributes, Codable, Sendable {
    public let sessionId: String
    public let projectName: String

    public init(sessionId: String, projectName: String) {
        self.sessionId = sessionId
        self.projectName = projectName
    }

    public struct ContentState: Codable, Hashable, Sendable {
        public let status: String
        public let model: String
        public let tasks: [TaskItem]
        public let completedCount: Int
        public let totalCount: Int
        public let currentTask: String?
        public let contextPct: Int
        public let costUSD: Double
        public let durationSec: Int

        public init(status: String, model: String, tasks: [TaskItem],
                    completedCount: Int, totalCount: Int, currentTask: String?,
                    contextPct: Int, costUSD: Double, durationSec: Int) {
            self.status = status
            self.model = model
            self.tasks = tasks
            self.completedCount = completedCount
            self.totalCount = totalCount
            self.currentTask = currentTask
            self.contextPct = contextPct
            self.costUSD = costUSD
            self.durationSec = durationSec
        }
    }
}

#else

public struct CodePulseAttributes: Codable, Sendable {
    public let sessionId: String
    public let projectName: String

    public init(sessionId: String, projectName: String) {
        self.sessionId = sessionId
        self.projectName = projectName
    }

    public struct ContentState: Codable, Hashable, Sendable {
        public let status: String
        public let model: String
        public let tasks: [TaskItem]
        public let completedCount: Int
        public let totalCount: Int
        public let currentTask: String?
        public let contextPct: Int
        public let costUSD: Double
        public let durationSec: Int

        public init(status: String, model: String, tasks: [TaskItem],
                    completedCount: Int, totalCount: Int, currentTask: String?,
                    contextPct: Int, costUSD: Double, durationSec: Int) {
            self.status = status
            self.model = model
            self.tasks = tasks
            self.completedCount = completedCount
            self.totalCount = totalCount
            self.currentTask = currentTask
            self.contextPct = contextPct
            self.costUSD = costUSD
            self.durationSec = durationSec
        }
    }
}

#endif
