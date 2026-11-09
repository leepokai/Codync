import Foundation
@preconcurrency import WebRTC

/// A trickle session's candidates: ours go to the host in order, the phone's go to the peer.
@MainActor
final class SessionCandidates {
    nonisolated let sink: AsyncThrowingStream<SessionCandidate, Error>.Continuation
    private var task: Task<Void, Never>?
    private var incoming = CandidateProgress()

    init(helper: ScreenHelper, session: String) {
        let pair = AsyncThrowingStream<SessionCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(CandidateProgress.maxCount + 1))
        sink = pair.continuation
        task = Task { [weak helper] in
            do {
                var outgoing = CandidateProgress()
                for try await candidate in pair.stream {
                    try Task.checkCancellation()
                    try outgoing.accept(candidate)
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
        let candidate = try SessionCandidate(json: value)
        try incoming.accept(candidate)
        // Native Apple WebRTC has no usable empty/null end-of-candidates operation.
        guard case let .candidate(sdp, index, mid) = candidate else { return }
        try await pc.add(RTCIceCandidate(sdp: sdp, sdpMLineIndex: index, sdpMid: mid))
    }

    func stop() {
        sink.finish()
        task?.cancel()
        task = nil
    }
}
