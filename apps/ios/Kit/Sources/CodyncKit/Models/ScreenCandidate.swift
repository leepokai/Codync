import Foundation

/// Private screen signaling events; credentials and candidates stay in memory.
public struct ScreenCandidate: Codable, Sendable, Equatable {
    public static let maxCount = 128
    private static let maxBytes = 4096
    private static let maxMediaIndex: Int32 = 16
    private static let maxMidBytes = 256
    public enum Kind: String, Codable, Sendable { case ready, candidate, complete, error }
    public let type: Kind
    public let candidate: String?
    public let sdpMLineIndex: Int32?
    public let sdpMid: String?
    public let message: String?

    public init(candidate: String, sdpMLineIndex: Int32, sdpMid: String?) {
        type = .candidate
        self.candidate = candidate
        self.sdpMLineIndex = sdpMLineIndex
        self.sdpMid = sdpMid
        message = nil
    }

    private init(_ type: Kind) {
        self.type = type
        candidate = nil
        sdpMLineIndex = nil
        sdpMid = nil
        message = nil
    }

    public static let complete = ScreenCandidate(.complete)

    public func validate() throws {
        if type == .candidate {
            guard let candidate, candidate.hasPrefix("candidate:"), candidate.utf8.count <= Self.maxBytes,
                  !candidate.contains("\r"), !candidate.contains("\n"),
                  let sdpMLineIndex, (0...Self.maxMediaIndex).contains(sdpMLineIndex),
                  (sdpMid?.utf8.count ?? 0) <= Self.maxMidBytes
            else { throw HostError.http(400, "Invalid screen candidate.") }
        }
    }
}
