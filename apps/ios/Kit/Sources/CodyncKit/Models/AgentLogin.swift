import Foundation

/// A login of a bot's chain: an agent on one of its login profiles, or (no login profile) on the login the computer already has.
/// Two steps are the same when they run the same agent on the same login.
public struct AgentLogin: Hashable, Sendable {
    public var backend: String
    public var loginProfile: String?

    public init(backend: String, loginProfile: String? = nil) {
        self.backend = backend
        self.loginProfile = loginProfile
    }
}

public extension BotFallback {
    var login: AgentLogin { AgentLogin(backend: backend, loginProfile: loginProfile) }

    /// A step on `agent`: `own` (the step before it changed agent) when it's on that agent and its login isn't elsewhere in
    /// the chain (`taken`), else the agent's first free login with its default model.
    static func on(_ agent: Backend, taken: [AgentLogin], own: BotFallback? = nil) -> BotFallback {
        if let own, own.backend == agent.id, !taken.contains(own.login) { return own }
        return BotFallback(backend: agent.id, loginProfile: agent.freeLogins(taken: taken).first ?? nil)
    }
}

public extension BotDraft {
    /// The bot's own login.
    var login: AgentLogin { AgentLogin(backend: backend, loginProfile: loginProfile) }
}

public extension AgentRest {
    var login: AgentLogin { AgentLogin(backend: agent, loginProfile: loginProfile) }
}

/// A sentence before it's translated: the phrase, and the names to put in once it's read in the app's language.
public struct FallbackPhrase: Sendable, Hashable {
    public let key: String
    public let values: [String: String]

    public init(key: String, values: [String: String]) {
        self.key = key
        self.values = values
    }
}

public extension Bot {
    /// The bot's own login.
    var login: AgentLogin { AgentLogin(backend: backend, loginProfile: loginProfile) }

    /// The step the bot runs on right now, as a login; nil on its own agent.
    var stepLogin: AgentLogin? { agentStep.map { AgentLogin(backend: $0.agent, loginProfile: $0.loginProfile) } }

    /// `login`'s rest while it lasts: one whose time has passed is over, whatever the host last said.
    func rest(of login: AgentLogin, now: Date = .now) -> AgentRest? {
        resting.first { $0.login == login && Date(milliseconds: $0.until) > now }
    }

    /// The Agent card's first line while the bot runs on a fallback, as a phrase and its names; nil on its own agent.
    /// `name` says what to call a login ("Claude Code (Work)" on a login profile); `own` asks for the computer's own
    /// login by its login profile too, which tells two logins of one agent apart. Translating the phrase before the
    /// names go in keeps a name that holds " while " from being read as the sentence's own word.
    func stepBannerPhrase(now: Date = .now, name: (AgentLogin, _ own: Bool) -> String) -> FallbackPhrase? {
        guard let step = stepLogin else { return nil }
        let same = step.backend == backend
        let to = name(step, same), own = name(login, same)
        guard let rest = rest(of: login, now: now) else {
            return FallbackPhrase(key: "Using {to} while {own} can't answer", values: ["to": to, "own": own])
        }
        // A reset time is real only for a usage limit; another rest names what it is (signed out, rate limited…).
        guard rest.reason == "usageLimit" else {
            return FallbackPhrase(key: "Using {to} while {own} \(rest.phrase)", values: ["to": to, "own": own])
        }
        return FallbackPhrase(
            key: "Using {to} until {own} resets at {time}",
            values: ["to": to, "own": own, "time": RelativeTime.clock(Date(milliseconds: rest.until))]
        )
    }
}

public extension Backend {
    /// The logins a new step can still take: the computer's own (nil, first) and the login profiles that are signed in,
    /// without the ones the chain has.
    func freeLogins(taken: [AgentLogin]) -> [String?] {
        let all: [String?] = [nil] + (loginProfiles ?? []).filter(\.signedIn).map(\.id)
        return all.filter { !taken.contains(AgentLogin(backend: id, loginProfile: $0)) }
    }

    /// The logins `step` can pick on this agent: the free ones, and its own, even one that has signed out since.
    func logins(for step: AgentLogin, taken: [AgentLogin]) -> [String?] {
        let free = freeLogins(taken: taken)
        return free.contains(step.loginProfile) ? free : free + [step.loginProfile]
    }
}

public extension [Backend] {
    /// The agents a step can use: each with a login the rest of the chain (`taken`) doesn't have, and the ones `keep` is on
    /// while no other step has that login (a step keeps its own login, even one that has signed out since).
    func usable(taken: [AgentLogin], keeping keep: [AgentLogin] = []) -> [Backend] {
        filter { backend in
            keep.contains { $0.backend == backend.id && !taken.contains($0) } || !backend.freeLogins(taken: taken).isEmpty
        }
    }
}
