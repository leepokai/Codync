import Foundation

/// One screen candidate event on the wire, matching the host's `Candidate`.
/// Plain values, so they can leave WebRTC's signaling thread.
enum SessionCandidate: Sendable, Equatable {
    case candidate(sdp: String, index: Int32, mid: String?)
    case complete

    private static let maxBytes = 4096
    private static let maxMediaIndex: Int32 = 16

    /// Parses and validates one candidate from the host.
    init(json: [String: Any]) throws {
        switch json["type"] as? String {
        case "complete": self = .complete
        case "candidate":
            guard let sdp = json["candidate"] as? String, sdp.hasPrefix("candidate:"), sdp.utf8.count <= Self.maxBytes,
                  !sdp.contains("\r"), !sdp.contains("\n"),
                  let index = json["sdpMLineIndex"] as? Int32, (0...Self.maxMediaIndex).contains(index)
            else { throw HelperError("invalid screen candidate") }
            self = .candidate(sdp: sdp, index: index, mid: json["sdpMid"] as? String)
        default: throw HelperError("invalid screen candidate")
        }
    }

    var json: [String: Any] {
        guard case let .candidate(sdp, index, mid) = self else { return ["type": "complete"] }
        var value: [String: Any] = ["type": "candidate", "candidate": sdp, "sdpMLineIndex": index]
        if let mid { value["sdpMid"] = mid }
        return value
    }
}

/// One direction of the exchange, under the host's rules: at most `maxCount`
/// candidates, then one completion, then nothing.
struct CandidateProgress {
    static let maxCount = 128
    private(set) var count = 0
    private(set) var isComplete = false

    mutating func accept(_ candidate: SessionCandidate) throws {
        guard !isComplete else { throw HelperError("screen candidates already complete") }
        if candidate == .complete {
            isComplete = true
        } else {
            guard count < Self.maxCount else { throw HelperError("too many screen candidates") }
            count += 1
        }
    }
}
