/// One of an agent's own `/` commands, as the agent lists them (ACP `available_commands_update`).
public struct AgentCommand: Codable, Hashable, Sendable {
    /// What follows the `/`.
    public var name: String
    public var description: String
    /// What to type after the name, for a command that takes input.
    public var hint: String?

    public init(name: String, description: String = "", hint: String? = nil) {
        (self.name, self.description, self.hint) = (name, description, hint)
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        hint = try c.decodeIfPresent(String.self, forKey: .hint)
    }
}
