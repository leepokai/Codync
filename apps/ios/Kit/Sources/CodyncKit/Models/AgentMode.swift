import Foundation

/// One of the modes an agent works in (Claude's Manual / Accept edits / Plan / Auto, Codex's Read-only / Auto review…),
/// as `agentModels` lists them: its Auto mode first and listed as `auto`.
public struct AgentMode: Decodable, Hashable, Sendable {
    /// Codync's id for the agent's Auto mode, whatever the agent calls it: new bots start there.
    public static let auto = "auto"

    public let id: String
    public let name: String
    public let description: String?
}

/// A bot's Approvals choice: the agent's modes, then Codync approving everything. An agent without modes offers
/// Ask me instead; while its list loads (or from an older host), Auto and Ask me.
public struct ApprovalChoice: Identifiable, Hashable, Sendable {
    public static let ask = "ask"
    public static let approve = "approve"
    static let asks = "Anything the agent still asks about comes to you as an approval card."

    public let id: String
    public let label: String
    /// The agent's own words for its mode, untranslated.
    public let note: String?
    /// What Codync does, to translate.
    public let detail: String

    static let askMe = ApprovalChoice(id: ask, label: "Ask me", note: nil,
                                      detail: "You'll get an approval card and a notification whenever the agent asks.")

    /// The choices for a bot on an agent with this model list (nil while it loads), and the one its setting shows as.
    /// A bot without a mode (or in Auto on an agent without one) shows as Ask me, and one in a mode the list lacks as
    /// that mode's id: never as more careful than it is.
    public static func choices(permission: String, mode: String?, catalog: AgentModels?) -> (all: [ApprovalChoice], selected: ApprovalChoice) {
        let approve = ApprovalChoice(id: approve, label: "Approve automatically", note: nil,
                                     detail: "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply.")
        var all: [ApprovalChoice]
        if let modes = catalog?.modes {
            all = modes.isEmpty ? [askMe] : modes.map { ApprovalChoice(id: modeId($0.id), label: $0.name, note: $0.description, detail: asks) }
        } else {
            all = [ApprovalChoice(id: modeId(AgentMode.auto), label: "Auto", note: nil,
                                  detail: "The agent decides what's safe and asks you about the rest."), askMe]
        }
        all.append(approve)
        if permission == "auto" { return (all, approve) }
        // Unlisted: Auto on an agent without it is its default, like no mode; any other mode is itself.
        let unlisted = mode.flatMap { $0 == AgentMode.auto ? nil : ApprovalChoice(id: modeId($0), label: $0, note: nil, detail: asks) } ?? askMe
        if let found = all.first(where: { $0.id == mode.map(modeId) ?? ask }) ?? all.first(where: { $0.id == unlisted.id }) {
            return (all, found)
        }
        all.insert(unlisted, at: all.count - 1)
        return (all, unlisted)
    }

    static func modeId(_ mode: String) -> String { "mode:\(mode)" }
}

public extension BotDraft {
    /// Picks an Approvals choice (`ApprovalChoice`).
    mutating func setApproval(_ id: String) {
        switch id {
        case ApprovalChoice.approve: (permission, mode) = ("auto", nil)
        case ApprovalChoice.ask: (permission, mode) = ("ask", nil)
        default: (permission, mode) = ("ask", String(id.dropFirst("mode:".count)))
        }
    }
}
