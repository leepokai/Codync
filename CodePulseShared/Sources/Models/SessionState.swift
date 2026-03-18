import Foundation

public struct SessionState: Codable, Identifiable, Equatable, Sendable {
    public let sessionId: String
    public var projectName: String
    public var gitBranch: String
    public var status: SessionStatus
    public var model: String
    public var summary: String
    public var currentTask: String?
    public var lastEvent: String?
    public var waitingReason: WaitingReason?
    public var tasks: [TaskItem]
    public var contextPct: Int
    public var costUSD: Double
    public var startedAt: Date
    public var durationSec: Int
    public var deviceId: String
    public var updatedAt: Date

    public var id: String { sessionId }

    public var completedTaskCount: Int {
        tasks.filter { $0.status == .completed }.count
    }

    public var totalTaskCount: Int {
        tasks.count
    }

    public var truncatedTasks: [TaskItem] {
        Array(tasks.suffix(10))
    }

    public init(
        sessionId: String, projectName: String, gitBranch: String,
        status: SessionStatus, model: String, summary: String,
        currentTask: String? = nil, lastEvent: String? = nil,
        waitingReason: WaitingReason? = nil, tasks: [TaskItem] = [],
        contextPct: Int = 0, costUSD: Double = 0,
        startedAt: Date = Date(), durationSec: Int = 0,
        deviceId: String = "", updatedAt: Date = Date()
    ) {
        self.sessionId = sessionId
        self.projectName = projectName
        self.gitBranch = gitBranch
        self.status = status
        self.model = model
        self.summary = summary
        self.currentTask = currentTask
        self.lastEvent = lastEvent
        self.waitingReason = waitingReason
        self.tasks = tasks
        self.contextPct = contextPct
        self.costUSD = costUSD
        self.startedAt = startedAt
        self.durationSec = durationSec
        self.deviceId = deviceId
        self.updatedAt = updatedAt
    }
}
