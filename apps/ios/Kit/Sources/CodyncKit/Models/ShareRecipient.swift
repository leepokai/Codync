import Foundation

/// A chat the share sheet sent to, remembered per account so the next share starts there.
public struct ShareRecipient: Codable, Hashable, Sendable {
    public var computerId: ComputerID
    public var botId: String

    public init(computerId: ComputerID, botId: String) {
        self.computerId = computerId
        self.botId = botId
    }

    /// How many the Recent section shows.
    public static let kept = 3

    /// `recent` with `self` moved to the front, at most `kept` long.
    public func remembered(in recent: [ShareRecipient]) -> [ShareRecipient] {
        Array(([self] + recent.filter { $0 != self }).prefix(Self.kept))
    }
}
