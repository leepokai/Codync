import Foundation

// Login profiles (docs/features/login-profiles.md): an agent's default login profile (the login the computer already
// has) and its other logins. A bot picks one (`Bot.loginProfile`); bots that pick none run on the default one.

/// One more login to an agent. `name`: the user's name for it, else who it signs in (an email), once it has.
public struct LoginProfile: Codable, Hashable, Sendable, Identifiable {
    public enum Kind: String, Codable, Sendable {
        /// The agent's own sign-in, in a folder of the profile's.
        case account
        /// Named environment variables, typed or imported from the login shell.
        case apiKey
    }

    public var id: String
    public var kind: Kind
    public var name: String?
    /// The name the user gave it.
    public var label: String?
    public var email: String?
    /// An API key profile is signed in once it has a key.
    public var signedIn: Bool
    /// An API key profile's variable names; their values never leave the computer.
    public var vars: [String]

    public init(id: String, kind: Kind = .account, name: String? = nil, label: String? = nil, email: String? = nil,
                signedIn: Bool = false, vars: [String] = []) {
        self.id = id
        self.kind = kind
        self.name = name
        self.label = label
        self.email = email
        self.signedIn = signedIn
        self.vars = vars
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        kind = (try? c.decodeIfPresent(Kind.self, forKey: .kind)) ?? .account
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        label = try? c.decodeIfPresent(String.self, forKey: .label)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
        signedIn = (try? c.decodeIfPresent(Bool.self, forKey: .signedIn)) ?? false
        vars = (try? c.decodeIfPresent([String].self, forKey: .vars)) ?? []
    }

    /// The variables its key form starts with: the ones it has, else the names its agent's key usually goes in
    /// (`Backend.loginKeys`), else one to name.
    public func keyNames(suggested: [String]) -> [String] {
        !vars.isEmpty ? vars : !suggested.isEmpty ? suggested : [""]
    }

    /// What its key form sends: each typed value under its name, and an empty one for each of `removed` (a saved
    /// variable taken off), which removes it. A removed name typed again keeps the new value.
    public static func keysToSave(rows: [(name: String, value: String)], removed: [String] = []) -> [String: String] {
        let typed = rows.map { ($0.name.trimmingCharacters(in: .whitespaces), $0.value) }.filter { !$0.0.isEmpty && !$0.1.isEmpty }
        return Dictionary(removed.map { ($0, "") } + typed) { _, new in new }
    }
}

/// The login the computer already has, beside the login profiles: the name the user gave it and who it signs in,
/// when known.
public struct DefaultLoginProfile: Codable, Hashable, Sendable {
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
    /// The login profiles a bot on `loginProfile` can pick: one that isn't signed in can't run a bot; the bot's own
    /// stays listed so its row shows it.
    func loginProfileChoices(picked loginProfile: String?) -> [LoginProfile] {
        (loginProfiles ?? []).filter { $0.signedIn || $0.id == loginProfile }
    }
}
