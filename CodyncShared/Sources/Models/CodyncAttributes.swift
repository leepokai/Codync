import Foundation

#if os(iOS)
import ActivityKit

public struct CodyncAttributes: ActivityAttributes, Codable, Sendable {
    public let sessionId: String
    public let projectName: String
    public let summary: String

    public init(sessionId: String, projectName: String, summary: String = "") {
        self.sessionId = sessionId
        self.projectName = projectName
        self.summary = summary
    }

    public struct ContentState: Codable, Hashable, Sendable {
        public let status: String
        public let model: String
        public let tasks: [TaskItem]
        public let completedCount: Int
        public let totalCount: Int
        public let currentTask: String?
        public let previousTask: String?
        public let contextPct: Int
        public let costUSD: Double
        public let durationSec: Int
        public let sessionStartDate: Date

        public init(status: String, model: String, tasks: [TaskItem],
                    completedCount: Int, totalCount: Int, currentTask: String?,
                    previousTask: String? = nil,
                    contextPct: Int, costUSD: Double, durationSec: Int,
                    sessionStartDate: Date = Date()) {
            self.status = status
            self.model = model
            self.tasks = tasks
            self.completedCount = completedCount
            self.totalCount = totalCount
            self.currentTask = currentTask
            self.previousTask = previousTask
            self.contextPct = contextPct
            self.costUSD = costUSD
            self.durationSec = durationSec
            self.sessionStartDate = sessionStartDate
        }

        /// Whether the session is actively processing (working or compacting)
        public var isBusy: Bool { status == "working" || status == "compacting" }

        /// Whether the session has finished
        public var isCompleted: Bool { status == "completed" }
    }
}

#else

public struct CodyncAttributes: Codable, Sendable {
    public let sessionId: String
    public let projectName: String
    public let summary: String

    public init(sessionId: String, projectName: String, summary: String = "") {
        self.sessionId = sessionId
        self.projectName = projectName
        self.summary = summary
    }

    public struct ContentState: Codable, Hashable, Sendable {
        public let status: String
        public let model: String
        public let tasks: [TaskItem]
        public let completedCount: Int
        public let totalCount: Int
        public let currentTask: String?
        public let previousTask: String?
        public let contextPct: Int
        public let costUSD: Double
        public let durationSec: Int
        public let sessionStartDate: Date

        public init(status: String, model: String, tasks: [TaskItem],
                    completedCount: Int, totalCount: Int, currentTask: String?,
                    previousTask: String? = nil,
                    contextPct: Int, costUSD: Double, durationSec: Int,
                    sessionStartDate: Date = Date()) {
            self.status = status
            self.model = model
            self.tasks = tasks
            self.completedCount = completedCount
            self.totalCount = totalCount
            self.currentTask = currentTask
            self.previousTask = previousTask
            self.contextPct = contextPct
            self.costUSD = costUSD
            self.durationSec = durationSec
            self.sessionStartDate = sessionStartDate
        }

        /// Whether the session is actively processing (working or compacting)
        public var isBusy: Bool { status == "working" || status == "compacting" }

        /// Whether the session has finished
        public var isCompleted: Bool { status == "completed" }
    }
}

#endif
