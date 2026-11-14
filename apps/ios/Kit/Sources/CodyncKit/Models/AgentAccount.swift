import Foundation

// Agent accounts (docs/features/agent-accounts.md): an agent's other logins, beside the one the computer already has.
// A bot picks one (`Bot.account`); bots that pick none run on the computer's own.

/// One more login to an agent. `name`: who it signs in (an email), once it has.
public struct AgentAccount: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String?
    public var signedIn: Bool

    public init(id: String, name: String? = nil, signedIn: Bool = false) {
        self.id = id
        self.name = name
        self.signedIn = signedIn
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        signedIn = (try? c.decodeIfPresent(Bool.self, forKey: .signedIn)) ?? false
    }
}

public extension Backend {
    /// The accounts a bot on `account` can pick: one that isn't signed in can't run a bot; the bot's own stays
    /// listed so its row shows it.
    func accountChoices(picked account: String?) -> [AgentAccount] {
        (accounts ?? []).filter { $0.signedIn || $0.id == account }
    }
}
