import Foundation

/// A bot's character in three layers the user writes (the host's `agent::personality`). The bot can't edit them.
public struct Personality: Codable, Hashable, Sendable {
    public var soul: String
    public var persona: String
    public var heartbeat: String

    public init(soul: String = "", persona: String = "", heartbeat: String = "") {
        self.soul = soul
        self.persona = persona
        self.heartbeat = heartbeat
    }

    /// Lenient: a layer the host leaves out is empty.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        soul = try c.decodeIfPresent(String.self, forKey: .soul) ?? ""
        persona = try c.decodeIfPresent(String.self, forKey: .persona) ?? ""
        heartbeat = try c.decodeIfPresent(String.self, forKey: .heartbeat) ?? ""
    }
}
