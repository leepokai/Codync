import Foundation

/// A saved setting that didn't take: `not_offered` (left out, never sent) or `rejected` (the agent refused it), and
/// the value it was about.
public struct SettingIssue: Codable, Hashable, Sendable {
    public var issue: String
    public var value: String
}

/// What the running agent session says about its settings, for the step of the chain it runs.
public struct SessionReport: Equatable, Sendable {
    public var live: Bool
    /// Setting id → value, as the session last listed them; nil: none listed.
    public var options: [String: String]?
    public var settings: [AgentModels.Option]?
    public var issues: [String: SettingIssue]
    /// A change waits for the running reply, then starts a new agent session.
    public var pending: Bool
}

extension Bot {
    /// What agent `backend` says, when the bot runs on it now: its own agent unless it's on a fallback. Another
    /// agent's session says nothing about it.
    public func report(for backend: String) -> SessionReport? {
        guard (agentStep?.agent ?? self.backend) == backend else { return nil }
        return SessionReport(live: sessionLive, options: sessionOptions, settings: sessionSettings, issues: settingIssues, pending: restartPending)
    }
}

/// The Effort level row of one step of a bot's chain (its own agent or a fallback).
public enum EffortLevel: Equatable, Sendable {
    /// No row: the agent has no effort level for this model, as far as anyone knows, and nothing is saved.
    case none
    /// Unknown until the agent runs this model.
    case later
    /// The setting to pick from (when known) and the one line under it.
    case row(setting: AgentModels.Option?, caption: String?)

    /// The agent's effort level: the setting of ACP category `thought_level`, whatever its id.
    public static func setting(in settings: [AgentModels.Option]) -> AgentModels.Option? {
        settings.first { $0.category == "thought_level" }
    }

    /// The settings a step shows.
    public static func shown(catalog: [AgentModels.Option], report: SessionReport?, saved: [String: String]) -> [AgentModels.Option] {
        catalog
    }

    /// `catalog`: what the agent lists for the step's model; `saved`: what the host saved; `report`: the step's own
    /// agent's session while it runs. First match wins; nothing is inferred from a saved value.
    public static func row(
        catalog: [AgentModels.Option], saved: String?, report: SessionReport?, loading: Bool, failed: Bool, chosenModel: Bool
    ) -> EffortLevel {
        // While nothing is pending, the running session's own list is for the model it runs.
        let listed = report.flatMap { $0.pending ? nil : $0.settings }.flatMap(setting(in:))
        guard let found = listed ?? setting(in: catalog) else {
            if loading { return saved.map { .row(setting: nil, caption: "Saved: \($0) · checking this agent’s options") } ?? .none }
            if failed { return saved.map { .row(setting: nil, caption: "Saved: \($0) · couldn’t check this agent’s options") } ?? .none }
            if chosenModel && catalog.isEmpty { return .later }
            return saved.map { .row(setting: nil, caption: "Saved: \($0) · not offered for this model") } ?? .none
        }
        func name(_ v: String) -> String { found.values.first { $0.id == v }?.name ?? v }
        let reported = report?.options?[found.id]
        let caption: String
        if let s = saved, let issue = report?.issues[found.id], issue.value == s {
            if issue.issue == "rejected" {
                caption = "Saved: \(name(s)) · not applied (the agent refused it)"
            } else {
                caption = reported.map { "In use: \(name($0)) · Saved: \(name(s)) not offered" } ?? "Saved: \(name(s)) · not offered for this model"
            }
        } else if report?.pending == true, saved != reported {
            let next = saved.map(name) ?? "Default"
            caption = reported.map { "In use: \(name($0)) · \(next) pending" } ?? "Current: unknown · \(next) pending"
        } else if report?.live != true {
            caption = saved.map { "Not running · Saved: \(name($0))" } ?? "Not running"
        } else if let r = reported {
            if let s = saved {
                caption = s == r ? "In use: \(name(r))" : "Saved: \(name(s)) · not confirmed (in use: \(name(r)))"
            } else {
                caption = "In use: \(name(r)) (agent default)"
            }
        } else {
            caption = "Current: unknown"
        }
        return .row(setting: found, caption: caption)
    }

    /// The line under a fallback's row, without starting its agent: its effort level as its own running session
    /// reports it, or else what is saved for it (Edit loads the agent's list to change it).
    public static func fallbackLine(saved: [String: String], report: SessionReport?) -> String? {
        if let found = report?.settings.flatMap(setting(in:)),
           case .row(_, let caption?) = row(catalog: [found], saved: saved[found.id], report: report, loading: false, failed: false, chosenModel: false) {
            return "Effort level · \(caption)"
        }
        let values = saved.keys.sorted().compactMap { saved[$0] }
        return values.isEmpty ? nil : "Saved: \(values.joined(separator: ", "))"
    }
}
