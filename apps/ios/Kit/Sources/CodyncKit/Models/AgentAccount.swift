import Foundation

// Agent accounts (docs/features/agent-accounts.md): an agent's default account (the login the computer already has)
// and its other logins. A bot picks one (`Bot.account`); bots that pick none run on the default one.

/// One more login to an agent. `name`: the user's name for it, else who it signs in (an email), once it has.
public struct AgentAccount: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String?
    /// The name the user gave it.
    public var label: String?
    public var email: String?
    public var signedIn: Bool

    public init(id: String, name: String? = nil, label: String? = nil, email: String? = nil, signedIn: Bool = false) {
        self.id = id
        self.name = name
        self.label = label
        self.email = email
        self.signedIn = signedIn
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        label = try? c.decodeIfPresent(String.self, forKey: .label)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
        signedIn = (try? c.decodeIfPresent(Bool.self, forKey: .signedIn)) ?? false
    }
}

/// The login the computer already has, beside the accounts: the name the user gave it and who it signs in, when known.
public struct OwnAccount: Codable, Hashable, Sendable {
    public var name: String?
    public var label: String?
    public var email: String?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        label = try? c.decodeIfPresent(String.self, forKey: .label)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
    }
}

public extension Backend {
    /// The accounts a bot on `account` can pick: one that isn't signed in can't run a bot; the bot's own stays
    /// listed so its row shows it.
    func accountChoices(picked account: String?) -> [AgentAccount] {
        (accounts ?? []).filter { $0.signedIn || $0.id == account }
    }
}
