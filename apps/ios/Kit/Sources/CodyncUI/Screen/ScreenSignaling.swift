import CodyncKit
import Foundation
@preconcurrency import WebRTC

/// Two bounded FIFOs. Each direction has one consumer; neither applies candidates before the answer.
@MainActor
final class ScreenSignaling {
    private let client: HostClient
    private let session: String
    private let applyCandidate: @MainActor (ScreenCandidate) async throws -> Void
    private let onFailure: @MainActor (Error) -> Void
    nonisolated let local: AsyncThrowingStream<ScreenCandidate, Error>.Continuation
    private var localTask: Task<Void, Never>?
    private var remoteTask: Task<Void, Never>?
    private var deadline: Task<Void, Never>?
    private var ready: CheckedContinuation<Void, Error>?
    private var isReady = false
    private var answered = false
    private var stopped = false
    private var remoteComplete = false
    private var remoteCount = 0
    private var answerWaiter: CheckedContinuation<Void, Never>?
    private let localStream: AsyncThrowingStream<ScreenCandidate, Error>

    typealias LocalPair = (stream: AsyncThrowingStream<ScreenCandidate, Error>, continuation: AsyncThrowingStream<ScreenCandidate, Error>.Continuation)

    convenience init(client: HostClient, session: String, pc: RTCPeerConnection, localPair: LocalPair, onFailure: @escaping @MainActor (Error) -> Void) {
        self.init(client: client, session: session, localPair: localPair, applyCandidate: { event in
            guard let sdp = event.candidate, let index = event.sdpMLineIndex else { throw HostError.http(400, "Invalid screen candidate.") }
            let candidate = RTCIceCandidate(sdp: sdp, sdpMLineIndex: index, sdpMid: event.sdpMid)
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
                pc.add(candidate) { error in
                    if let error { c.resume(throwing: error) } else { c.resume() }
                }
            }
        }, onFailure: onFailure)
    }

    init(client: HostClient, session: String, localPair: LocalPair, applyCandidate: @escaping @MainActor (ScreenCandidate) async throws -> Void, onFailure: @escaping @MainActor (Error) -> Void) {
        self.client = client
        self.session = session
        self.applyCandidate = applyCandidate
        self.onFailure = onFailure
        local = localPair.continuation
        localStream = localPair.stream
    }

    func subscribe() async throws {
        remoteTask = Task { [weak self, client, session] in
            do {
                for try await event in client.screenCandidates(session: session) {
                    guard let self, !self.stopped else { return }
                    try await self.receive(event)
                }
                throw HostError.unreachable
            } catch { self?.fail(error) }
        }
        deadline = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(10)) } catch { return }
            self?.fail(HostError.unreachable)
        }
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { c in
                if isReady { c.resume() }
                else if stopped { c.resume(throwing: HostError.unreachable) }
                else { ready = c }
            }
        } onCancel: { Task { @MainActor in self.stop() } }
        deadline?.cancel()
    }

    func check() throws {
        guard !stopped else { throw HostError.unreachable }
    }

    func installedAnswer() async throws {
        try check()
        answered = true
        answerWaiter?.resume()
        answerWaiter = nil
        localTask = Task { [weak self, localStream, client, session] in
            do {
                var complete = false
                var count = 0
                for try await event in localStream {
                    try Task.checkCancellation()
                    guard !complete else { throw HostError.http(400, "Screen candidates already complete.") }
                    if event.type == .complete { complete = true }
                    else { count += 1; guard count <= ScreenCandidate.maxCount else { throw HostError.http(400, "Too many screen candidates.") } }
                    try await client.screenCandidate(session: session, candidate: event)
                }
            } catch { self?.fail(error) }
        }
    }

    private func receive(_ event: ScreenCandidate) async throws {
        switch event.type {
        case .ready:
            guard !isReady else { throw HostError.http(400, "Duplicate screen signaling subscription.") }
            isReady = true
            ready?.resume()
            ready = nil
        case .error: throw HostError.http(400, event.message ?? "Screen signaling failed.")
        case .candidate, .complete:
            guard isReady, !remoteComplete else { throw HostError.http(400, "Screen candidates out of order.") }
            if event.type == .complete { remoteComplete = true }
            else { remoteCount += 1; guard remoteCount <= ScreenCandidate.maxCount else { throw HostError.http(400, "Too many screen candidates.") } }
            // Suspend this consumer: its bounded transport stream buffers further events.
            if !answered { await withCheckedContinuation { answerWaiter = $0 } }
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            if event.type == .candidate { try await applyCandidate(event) }
            // Native Apple WebRTC exposes no usable empty/null end-of-candidates operation.
        }
    }

    private func fail(_ error: Error) {
        guard !stopped else { return }
        stop()
        onFailure(error)
    }

    func stop() {
        guard !stopped else { return }
        stopped = true
        deadline?.cancel()
        ready?.resume(throwing: CancellationError())
        ready = nil
        answerWaiter?.resume()
        answerWaiter = nil
        local.finish()
        localTask?.cancel()
        remoteTask?.cancel()
    }
}
