import Foundation

/// A turn that stopped half done (out of steps, or its answer cut at the output limit): the card
/// offers to go on with it, or to end it there.
public struct PausedTurn: Codable, Hashable, Sendable {
    public enum Verdict: String, Codable, Sendable {
        case paused
        case truncated
    }

    public enum Status: String, Codable, Sendable {
        case waiting
        case continued
        case closed
    }

    public var verdict: Verdict
    public var status: Status
    /// Continuations in a row before the card asks the person to say what remains.
    public var round: Int
    public var canContinue: Bool
}
