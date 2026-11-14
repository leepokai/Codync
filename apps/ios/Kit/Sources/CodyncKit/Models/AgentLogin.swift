import Foundation

/// A login of a bot's chain: an agent on one of its accounts, or (no account) on the login the computer already has.
/// Two steps are the same when they run the same agent on the same login.
public struct AgentLogin: Hashable, Sendable {
    public var backend: String
    public var account: String?

    public init(backend: String, account: String? = nil) {
        self.backend = backend
        self.account = account
    }
}

public extension BotFallback {
    var login: AgentLogin { AgentLogin(backend: backend, account: account) }
}

public extension BotDraft {
    /// The bot's own login.
    var login: AgentLogin { AgentLogin(backend: backend, account: account) }
}

public extension AgentRest {
    var login: AgentLogin { AgentLogin(backend: agent, account: account) }
}

public extension Bot {
    /// The bot's own login.
    var login: AgentLogin { AgentLogin(backend: backend, account: account) }

    /// The step the bot runs on right now, as a login; nil on its own agent.
    var stepLogin: AgentLogin? { agentStep.map { AgentLogin(backend: $0.agent, account: $0.account) } }

    /// `login`'s rest while it lasts: one whose time has passed is over, whatever the host last said.
    func rest(of login: AgentLogin, now: Date = .now) -> AgentRest? {
        resting.first { $0.login == login && Date(milliseconds: $0.until) > now }
    }

    /// The Agent card's first line while the bot runs on a fallback; nil on its own agent. `name` says what to call
    /// a login ("Claude Code (Work)" on an account); `own` asks for the computer's own login by its account too,
    /// which tells two logins of one agent apart.
    func stepBanner(now: Date = .now, name: (AgentLogin, _ own: Bool) -> String) -> String? {
        guard let step = stepLogin else { return nil }
        let same = step.backend == backend
        let to = name(step, same), own = name(login, same)
        guard let rest = rest(of: login, now: now) else { return "Using \(to) while \(own) can't answer" }
        return "Using \(to) until \(own) resets at \(RelativeTime.clock(Date(milliseconds: rest.until)))"
    }
}

public extension Backend {
    /// The logins a new step can still take: the computer's own (nil, first) and the accounts that are signed in,
    /// without the ones the chain has.
    func freeLogins(taken: [AgentLogin]) -> [String?] {
        let all: [String?] = [nil] + (accounts ?? []).filter(\.signedIn).map(\.id)
        return all.filter { !taken.contains(AgentLogin(backend: id, account: $0)) }
    }
}
