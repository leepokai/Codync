import Foundation

/// A snapshot reported by the agent, with percentage and severity computed by the host.
public struct ContextUsage: Codable, Hashable, Sendable {
    public var usedTokens: UInt64
    public var maxTokens: UInt64
    public var percent: Int
    /// normal | warning | danger. Future levels keep decoding and use the neutral presentation.
    public var level: String
}
