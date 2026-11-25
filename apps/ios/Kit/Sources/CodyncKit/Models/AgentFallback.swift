import Foundation

// A bot's fallback agents: when its own agent can't answer (out of usage, rate limited, signed out, won't start),
// the next one carries the same turn on in the same chat (docs/features/agent-fallback.md). Reasons on the wire:
// usageLimit, outOfCredits, rateLimited, overloaded, signedOut, notStarting, modelMissing, modeMissing.

/// One step of a bot's fallback chain: another agent (or the bot's own on another login profile) and the model to use on it.
public struct BotFallback: Codable, Hashable, Sendable {
    /// Most fallbacks a bot has; the host refuses more.
    public static let limit = 3

    public var backend: String
    /// Only on a step the host or the bot set up with a custom command; the app never offers one.
    public var command: String?
    /// The login profile the step runs on; nil for the login the computer already has.
    public var loginProfile: String?
    public var model: String?
    public var modelOptions: [String: String] = [:]

    public init(backend: String, loginProfile: String? = nil, model: String? = nil, modelOptions: [String: String] = [:]) {
        self.backend = backend
        self.loginProfile = loginProfile
        self.model = model
        self.modelOptions = modelOptions
    }

    private enum CodingKeys: String, CodingKey { case backend, command, loginProfile, model, modelOptions }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        backend = try c.decode(String.self, forKey: .backend)
        command = try c.decodeIfPresent(String.self, forKey: .command)
        loginProfile = try? c.decodeIfPresent(String.self, forKey: .loginProfile)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        modelOptions = try c.decodeIfPresent([String: String].self, forKey: .modelOptions) ?? [:]
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(backend, forKey: .backend)
        try c.encodeIfPresent(command, forKey: .command)
        // Always sent, null for the computer's own login: the host keeps the saved login profile of a step sent without
        // one (an app from before fallbacks had login profiles).
        try c.encode(loginProfile, forKey: .loginProfile)
        try c.encodeIfPresent(model, forKey: .model)
        try c.encode(modelOptions, forKey: .modelOptions)
    }
}

/// The fallback a bot runs on right now (`agentStep`); none while it's on its own agent.
public struct AgentStep: Codable, Hashable, Sendable {
    /// 1 for the first fallback.
    public var step: Int
    public var agent: String
    /// The login profile the step runs on; nil for the computer's own login.
    public var loginProfile: String?
    public var model: String?
    public var since: Int64
    public var reason: String?
}

extension Bot {
    /// The agent the bot runs on right now: its fallback's while it's on one.
    public var runningAgent: String { agentStep?.agent ?? backend }
}

extension Sequence<Bot> {
    /// The bots run more than one agent, so the roster marks each with its agent's logo.
    public var mixesAgents: Bool { Set(filter { !$0.isGroup }.map(\.runningAgent)).count > 1 }
}

/// An agent of the bot's chain (its own included) resting after it couldn't answer, until `until` (ms).
public struct AgentRest: Codable, Hashable, Sendable {
    public var agent: String
    /// The login profile that rests; nil for the computer's own login.
    public var loginProfile: String?
    public var until: Int64
    public var reason: String

    /// "Usage resets 3:00 PM" / "Resting until 3:00 PM".
    public var line: String {
        let time = RelativeTime.clock(Date(milliseconds: until))
        return reason == "usageLimit" ? "Usage resets \(time)" : "Resting until \(time)"
    }

    /// Why it rests, in the banner's words: "is signed out", "is rate limited"…
    /// A usage limit says when it resets instead, and a missing model or mode never rests an agent (`rest::record`).
    var phrase: String {
        switch reason {
        case "outOfCredits": "is out of credits"
        case "rateLimited": "is rate limited"
        case "overloaded": "is overloaded"
        case "signedOut": "is signed out"
        case "notStarting": "couldn't start"
        default: "can't answer"
        }
    }
}

/// notice: the bot moved to another agent (`back`: to its own), with what the new one was told (`carried`).
/// Lenient: a field it can't read is left out, so the line (and the chat) always decodes.
/// A switch the user made: its next message hasn't gone yet, it carried the conversation, or nothing.
public enum UserSwitchState: String, Codable, Hashable, Sendable {
    case pending, handedOff, switched
}

public struct AgentSwitch: Codable, Hashable, Sendable {
    public var from: String?
    public var to: String?
    public var back: Bool?
    public var reason: String?
    public var resetsAt: Int64?
    public var carried: String?
    /// The user made the switch (another agent or login profile): the next message carries the conversation.
    public var manual: Bool?
    /// Where a switch the user made is (nil from an older host, or a state this app doesn't know).
    public var state: UserSwitchState?
    /// What the chat calls each side: the agent, with its login profile ("Claude Code (Work)").
    public var fromName: String?
    public var toName: String?

    public init(from decoder: any Decoder) throws {
        guard let c = try? decoder.container(keyedBy: CodingKeys.self) else { return }
        from = try? c.decodeIfPresent(String.self, forKey: .from)
        to = try? c.decodeIfPresent(String.self, forKey: .to)
        back = try? c.decodeIfPresent(Bool.self, forKey: .back)
        reason = try? c.decodeIfPresent(String.self, forKey: .reason)
        resetsAt = try? c.decodeIfPresent(Int64.self, forKey: .resetsAt)
        carried = try? c.decodeIfPresent(String.self, forKey: .carried)
        manual = try? c.decodeIfPresent(Bool.self, forKey: .manual)
        state = try? c.decodeIfPresent(UserSwitchState.self, forKey: .state)
        fromName = try? c.decodeIfPresent(String.self, forKey: .fromName)
        toName = try? c.decodeIfPresent(String.self, forKey: .toName)
    }
}

public extension BotDraft {
    /// The bot on `login` (an agent on one of its logins): a fallback on that login goes (a login is in the chain once,
    /// as the host keeps it). Another agent starts on its default model.
    mutating func setAgent(_ login: AgentLogin) {
        guard self.login != login else { return }
        loginProfile = login.loginProfile
        fallbacks?.removeAll { $0.login == login }
        guard backend != login.backend else { return }
        backend = login.backend
        model = nil
        modelOptions = [:]
        // A picked mode becomes the new agent's Auto mode (or, without one, asking).
        if mode != nil { mode = AgentMode.auto }
    }

    /// Advisor mode on: its threads start on the bot's own agent, model and fallbacks, for the user to change (on a
    /// custom command, which the threads' picker doesn't offer, on the first installed agent); off: none.
    mutating func setAdvisor(_ on: Bool, backends: [Backend]) {
        guard on else {
            threadAgent = nil
            threadFallbacks = []
            return
        }
        guard backend == "custom" else {
            threadAgent = BotFallback(backend: backend, loginProfile: loginProfile, model: model, modelOptions: modelOptions)
            threadFallbacks = fallbacks ?? []
            return
        }
        guard let first = backends.first(where: \.available) ?? backends.first else { return }
        threadAgent = BotFallback.on(first, taken: [])
        threadFallbacks = []
    }

    /// Its threads on `login`, as `setAgent` puts the bot on one: a threads' fallback on that login goes.
    mutating func setThreadAgent(_ login: AgentLogin) {
        guard threadAgent?.login != login else { return }
        if threadAgent?.backend == login.backend { threadAgent?.loginProfile = login.loginProfile }
        else { threadAgent = BotFallback(backend: login.backend, loginProfile: login.loginProfile) }
        threadFallbacks?.removeAll { $0.login == login }
    }
}

extension HostClient {
    /// The bot's own agent again: now when idle, otherwise for its next turn. The host posts the chat line.
    public func switchBack(_ botId: String) async throws {
        let _: Empty = try await call("switchBack", ["botId": botId])
    }
}
