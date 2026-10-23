import Foundation

// Stub: the Effort level isn't built yet.
public struct SettingIssue: Codable, Hashable, Sendable {
    public var issue: String
    public var value: String
}

public enum EffortLevel: Equatable, Sendable {
    case none
    case later
    case row(setting: AgentModels.Option?, caption: String?)

    public static func setting(in settings: [AgentModels.Option]) -> AgentModels.Option? { nil }
    public static func row(catalog: [AgentModels.Option], saved: String?, report: SessionReport?, loading: Bool, failed: Bool, chosenModel: Bool) -> EffortLevel { .none }
}

public struct SessionReport: Equatable, Sendable {
    public var live: Bool
    public var options: [String: String]?
    public var settings: [AgentModels.Option]?
    public var issues: [String: SettingIssue]
    public var pending: Bool
}

extension Bot {
    public func report(for backend: String) -> SessionReport? { nil }
}
