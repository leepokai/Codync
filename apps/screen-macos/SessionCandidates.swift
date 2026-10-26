import Foundation
@preconcurrency import WebRTC

/// Value snapshots taken on WebRTC's signaling thread, before hopping to the main actor.
struct SessionCandidate: Sendable {
    let sdp: String?
    let index: Int32
    let mid: String?

    static let complete = SessionCandidate(sdp: nil, index: 0, mid: nil)

    var json: [String: Any] {
        guard let sdp else { return ["type": "complete"] }
        var value: [String: Any] = ["type": "candidate", "candidate": sdp, "sdpMLineIndex": index]
        if let mid { value["sdpMid"] = mid }
        return value
    }
}

@MainActor
final class SessionCandidates {
    private static let maxCount = 128
    private static let maxBytes = 4096
    private static let maxMediaIndex: Int32 = 16
    nonisolated let sink: AsyncThrowingStream<SessionCandidate, Error>.Continuation
    private var task: Task<Void, Never>?
    private var incomingComplete = false
    private var incomingCount = 0

    init(helper: ScreenHelper, session: String) {
        let pair = AsyncThrowingStream<SessionCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(Self.maxCount + 1))
        sink = pair.continuation
        task = Task { [weak helper] in
            do {
                var complete = false
                var count = 0
                for try await candidate in pair.stream {
                    try Task.checkCancellation()
                    guard !complete else { throw HelperError("screen candidates already complete") }
                    if candidate.sdp == nil { complete = true }
                    else {
                        count += 1
                        guard count <= Self.maxCount else { throw HelperError("too many screen candidates") }
                    }
                    guard let helper else { return }
                    helper.candidate(session: session, candidate: candidate.json)
                }
            } catch {
                guard !Task.isCancelled else { return }
                helper?.sessionEnded(session)
            }
        }
    }

    func add(_ value: [String: Any], to pc: RTCPeerConnection) async throws {
        guard !incomingComplete else { throw HelperError("screen candidates already complete") }
        if value["type"] as? String == "complete" {
            // Native Apple WebRTC has no usable empty/null end-of-candidates operation.
            incomingComplete = true
            return
        }
        guard value["type"] as? String == "candidate", let sdp = value["candidate"] as? String,
              sdp.hasPrefix("candidate:"), sdp.utf8.count <= Self.maxBytes, !sdp.contains("\r"), !sdp.contains("\n"),
              let index = value["sdpMLineIndex"] as? Int32, (0...Self.maxMediaIndex).contains(index), incomingCount < Self.maxCount
        else { throw HelperError("invalid screen candidate") }
        incomingCount += 1
        let candidate = RTCIceCandidate(sdp: sdp, sdpMLineIndex: index, sdpMid: value["sdpMid"] as? String)
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
            pc.add(candidate) { error in
                if let error { c.resume(throwing: error) } else { c.resume() }
            }
        }
    }

    func stop() {
        sink.finish()
        task?.cancel()
        task = nil
    }
}
