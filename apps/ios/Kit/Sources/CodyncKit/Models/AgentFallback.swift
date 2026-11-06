import Foundation

// A bot's fallback agents: when its own agent can't answer (out of usage, rate limited, signed out, won't start),
// the next one carries the same turn on in the same chat (docs/features/agent-fallback.md). Reasons on the wire:
// usageLimit, outOfCredits, rateLimited, overloaded, signedOut, notStarting, modelMissing.

/// One step of a bot's fallback chain: another agent and the model to use on it.
public struct BotFallback: Codable, Hashable, Sendable {
    /// Most fallbacks a bot has; the host refuses more.
    public static let limit = 3

    public var backend: String
    /// Only on a step the host or the bot set up with a custom command; the app never offers one.
    public var command: String?
    public var model: String?
    public var modelOptions: [String: String] = [:]

    public init(backend: String, model: String? = nil, modelOptions: [String: String] = [:]) {
        self.backend = backend
        self.model = model
        self.modelOptions = modelOptions
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        backend = try c.decode(String.self, forKey: .backend)
        command = try c.decodeIfPresent(String.self, forKey: .command)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        modelOptions = try c.decodeIfPresent([String: String].self, forKey: .modelOptions) ?? [:]
    }
}

/// The fallback a bot runs on right now (`agentStep`); none while it's on its own agent.
public struct AgentStep: Codable, Hashable, Sendable {
    /// 1 for the first fallback.
    public var step: Int
    public var agent: String
    public var model: String?
    public var since: Int64
    public var reason: String?
}

/// An agent of the bot's chain (its own included) resting after it couldn't answer, until `until` (ms).
public struct AgentRest: Codable, Hashable, Sendable {
    public var agent: String
    public var until: Int64
    public var reason: String

    /// "Usage resets 3:00 PM" / "Resting until 3:00 PM".
    public var line: String {
        let time = RelativeTime.clock(Date(milliseconds: until))
        return reason == "usageLimit" ? "Usage resets \(time)" : "Resting until \(time)"
    }
}

/// notice: the bot moved to another agent (`back`: to its own), with what the new one was told (`carried`).
/// Lenient: a field it can't read is left out, so the line (and the chat) always decodes.
public struct AgentSwitch: Codable, Hashable, Sendable {
    public var from: String?
    public var to: String?
    public var back: Bool?
    public var reason: String?
    public var resetsAt: Int64?
    public var carried: String?

    public init(from decoder: any Decoder) throws {
        guard let c = try? decoder.container(keyedBy: CodingKeys.self) else { return }
        from = try? c.decodeIfPresent(String.self, forKey: .from)
        to = try? c.decodeIfPresent(String.self, forKey: .to)
        back = try? c.decodeIfPresent(Bool.self, forKey: .back)
        reason = try? c.decodeIfPresent(String.self, forKey: .reason)
        resetsAt = try? c.decodeIfPresent(Int64.self, forKey: .resetsAt)
        carried = try? c.decodeIfPresent(String.self, forKey: .carried)
    }
}

public extension Bot {
    /// Agent `agent`'s rest while it lasts: one whose time has passed is over, whatever the host last said.
    func rest(of agent: String, now: Date = .now) -> AgentRest? {
        resting.first { $0.agent == agent && Date(milliseconds: $0.until) > now }
    }
}

public extension BotDraft {
    /// The bot's own agent: a fallback on it goes, as the host drops it.
    mutating func setAgent(_ backend: String) {
        guard self.backend != backend else { return }
        self.backend = backend
        model = nil
        modelOptions = [:]
        fallbacks?.removeAll { $0.backend == backend }
    }
}

extension HostClient {
    /// The bot's own agent again: now when idle, otherwise for its next turn. The host posts the chat line.
    public func switchBack(_ botId: String) async throws {
        let _: Empty = try await call("switchBack", ["botId": botId])
    }
}
