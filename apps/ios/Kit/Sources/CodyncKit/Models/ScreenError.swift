import Foundation

/// Local negotiation and candidate validation failures, separate from host HTTP responses.
public enum ScreenError: LocalizedError, Sendable, Equatable {
    case invalidCandidate
    case candidateOverflow
    case candidatesAlreadyComplete
    case tooManyCandidates
    case duplicateSubscription
    case candidatesOutOfOrder
    case signalingFailed(String)
    case connectionFailed

    public var errorDescription: String? {
        switch self {
        case .invalidCandidate: "Invalid screen candidate."
        case .candidateOverflow: "Screen candidate queue overflow."
        case .candidatesAlreadyComplete: "Screen candidates already complete."
        case .tooManyCandidates: "Too many screen candidates."
        case .duplicateSubscription: "Duplicate screen signaling subscription."
        case .candidatesOutOfOrder: "Screen candidates out of order."
        case let .signalingFailed(message): message
        case .connectionFailed: "Couldn't start the video connection."
        }
    }
}
