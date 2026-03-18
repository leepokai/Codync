import Foundation

public struct TaskItem: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let content: String
    public let status: TaskStatus
    public let activeForm: String?

    public init(id: String, content: String, status: TaskStatus, activeForm: String? = nil) {
        self.id = id
        self.content = content
        self.status = status
        self.activeForm = activeForm
    }

    public var truncatedContent: String {
        if content.count > 50 {
            return String(content.prefix(47)) + "..."
        }
        return content
    }
}

public enum TaskStatus: String, Codable, Sendable {
    case pending
    case inProgress = "in_progress"
    case completed
}
