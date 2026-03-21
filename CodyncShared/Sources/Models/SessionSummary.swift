import Foundation

public struct SessionSummary: Codable, Hashable, Sendable {
    public let sessionId: String
    public let projectName: String
    public let status: SessionStatus
    public let model: String
    public let currentTask: String?
    public let costUSD: Double

    public init(sessionId: String, projectName: String, status: SessionStatus,
                model: String, currentTask: String?, costUSD: Double) {
        self.sessionId = sessionId
        self.projectName = projectName
        self.status = status
        self.model = model
        self.currentTask = currentTask
        self.costUSD = costUSD
    }
}
