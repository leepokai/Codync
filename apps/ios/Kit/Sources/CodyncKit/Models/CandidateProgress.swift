import Foundation

/// One direction of a screen candidate exchange, under the host's rules:
/// at most `ScreenCandidate.maxCount` candidates, then one completion, then nothing.
public struct CandidateProgress: Sendable {
    public private(set) var count = 0
    public private(set) var isComplete = false

    public init() {}

    public mutating func accept(_ event: ScreenCandidate) throws {
        guard !isComplete else { throw ScreenError.candidatesAlreadyComplete }
        if event.type == .complete {
            isComplete = true
        } else {
            guard count < ScreenCandidate.maxCount else { throw ScreenError.tooManyCandidates }
            count += 1
        }
    }
}
