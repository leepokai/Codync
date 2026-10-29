import Foundation

/// The computer's networks of the user's computers (`networkView`, and the `network` event): read-only
/// on phones, since who is in is decided at a computer. Hosts before it answer "unknown method".
public struct NetworkView: Codable, Hashable, Sendable {
    public var joined: Bool
    /// Every network the computer is in, the default first.
    public var networks: [Network]?
    /// The user's cloud assistant may message this computer's bots.
    public var assistants: Bool?
    /// Cloud assistants can reach this computer at all: it's in a Codync account.
    public var assistantsAvailable: Bool?

    public struct Network: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        /// What the user calls it (`home`).
        public var name: String
        /// This computer's name in it.
        public var computer: String?
        public var role: Role?
        public var computers: [Computer]?
        /// Who on its other computers may message this computer's bots, the first match deciding.
        public var rules: [Rule]?

        /// "Deny: any bot on laptop → Reviewer", names from what each computer offers.
        public func text(of rule: Rule) -> String {
            let agents = { (me: Bool, computer: String?) in
                (computers ?? []).first { $0.me == me && (me || $0.name.caseInsensitiveCompare(computer ?? "") == .orderedSame) }?.agents ?? []
            }
            let named = { (id: String, among: [Agent]) in among.first { $0.id == id }?.name ?? id }
            // `lead@*` (only from the command line) reads "lead on any computer": never wider than it is.
            let from = rule.fromComputer == Rule.any && rule.fromAgent == Rule.any
                ? L10n.tr("any bot on any computer")
                : rule.fromComputer == Rule.any
                ? L10n.tr("{bot} on any computer").replacingOccurrences(of: "{bot}", with: rule.fromAgent)
                : rule.fromAgent == Rule.any
                ? L10n.tr("any bot on {computer}").replacingOccurrences(of: "{computer}", with: rule.fromComputer)
                : L10n.tr("{bot} on {computer}")
                    .replacingOccurrences(of: "{bot}", with: named(rule.fromAgent, agents(false, rule.fromComputer)))
                    .replacingOccurrences(of: "{computer}", with: rule.fromComputer)
            let to = rule.toAgent == Rule.any ? L10n.tr("any bot here") : named(rule.toAgent, agents(true, nil))
            let template = rule.access == .allow ? "Allow: {from} → {to}" : "Deny: {from} → {to}"
            return L10n.tr(template).replacingOccurrences(of: "{from}", with: from).replacingOccurrences(of: "{to}", with: to)
        }
    }

    public struct Rule: Codable, Hashable, Sendable {
        public static let any = "*"
        public var fromComputer: String
        public var fromAgent: String
        public var toAgent: String
        public var access: Access

        public enum Access: String, Codable, Hashable, Sendable {
            case allow, deny
            /// Anything a newer host adds reads as a refusal: showing more access than there is would mislead.
            public init(from decoder: any Decoder) throws {
                self = Access(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .deny
            }
        }
    }

    public enum Role: String, Codable, Hashable, Sendable {
        case owner, admin, member
        /// Any role a newer host adds reads as a plain member.
        public init(from decoder: any Decoder) throws {
            self = Role(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .member
        }

        public var title: String {
            switch self {
            case .owner: "Owner"
            case .admin: "Admin"
            case .member: "Member"
            }
        }
    }

    public struct Computer: Codable, Hashable, Sendable, Identifiable {
        public var name: String
        public var role: Role
        public var me: Bool
        public var online: Bool
        /// The bots it offers, by address name and display name.
        public var agents: [Agent]?

        public var id: String { name }
    }

    public struct Agent: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        public var name: String
    }

    /// Worth showing: in a network, or cloud assistants can reach this computer's bots.
    public var hasContent: Bool { joined || (assistantsAvailable == true && assistants == true) }
}
