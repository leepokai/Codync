import Foundation

/// Destinations are validated and dispatched by the host.
public struct RoutineOutput: Codable, Hashable, Sendable {
    public var kind: String
    public var target: String
    public var account: String?
    public var value: String?

    public init(kind: String = "slack", target: String = "", account: String? = nil, value: String? = nil) {
        self.kind = kind
        self.target = target
        self.account = account
        self.value = value
    }

    public static let kinds = [
        (id: "slack", label: "Slack"), (id: "telegram", label: "Telegram"),
        (id: "whatsapp", label: "WhatsApp"), (id: "discord", label: "Discord"),
        (id: "discordReact", label: "Discord reaction"), (id: "githubComment", label: "GitHub comment"),
        (id: "githubIssue", label: "GitHub issue"), (id: "githubLabel", label: "GitHub label"),
        (id: "githubAssign", label: "GitHub assign"), (id: "githubReview", label: "Request review")
    ]

    public var destinationPlaceholder: String {
        if kind.hasPrefix("github") { return "owner/name" }
        switch kind {
        case "whatsapp": return "5215555555555@s.whatsapp.net"
        case "telegram": return "-1001234567890"
        case "discordReact": return "👍"
        default: return "1234567890"
        }
    }

    public var missingField: String? {
        if target.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return "An output needs a destination." }
        if kind == "whatsapp", account?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false {
            return "Use the linked WhatsApp account id as account."
        }
        return nil
    }
}
