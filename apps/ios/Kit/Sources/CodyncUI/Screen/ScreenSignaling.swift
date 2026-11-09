import CodyncKit
import Foundation
@preconcurrency import WebRTC

/// Two bounded FIFOs. Each direction has one consumer; neither applies candidates before the answer.
@MainActor
final class ScreenSignaling {
    private enum State { case active, detached, stopped }
    private let client: HostClient
    private let session: String
    private let applyCandidate: @MainActor (ScreenCandidate) async throws -> Void
    private let isMediaConnected: @MainActor () -> Bool
    private let onFailure: @MainActor (Error) -> Void
    nonisolated let local: AsyncThrowingStream<ScreenCandidate, Error>.Continuation
    private var localTask: Task<Void, Never>?
    private var remoteTask: Task<Void, Never>?
    private var deadline: Task<Void, Never>?
    private var ready: CheckedContinuation<Void, Error>?
    private var isReady = false
    private var answered = false
    private var state = State.active
    private var failure: Error?
    private var mediaEstablished = false
    private var localComplete = false
    private var remote = CandidateProgress()
    private var answerWaiter: CheckedContinuation<Void, Never>?
    private let localStream: AsyncThrowingStream<ScreenCandidate, Error>

    var isDetached: Bool { state == .detached }
    var candidatesComplete: Bool { localComplete && remote.isComplete }

    typealias LocalPair = (stream: AsyncThrowingStream<ScreenCandidate, Error>, continuation: AsyncThrowingStream<ScreenCandidate, Error>.Continuation)

    convenience init(client: HostClient, session: String, pc: RTCPeerConnection, localPair: LocalPair, onFailure: @escaping @MainActor (Error) -> Void) {
        self.init(client: client, session: session, localPair: localPair, applyCandidate: { event in
            guard let sdp = event.candidate, let index = event.sdpMLineIndex else { throw ScreenError.invalidCandidate }
            try await pc.add(RTCIceCandidate(sdp: sdp, sdpMLineIndex: index, sdpMid: event.sdpMid))
        }, isMediaConnected: { pc.iceConnectionState == .connected || pc.iceConnectionState == .completed }, onFailure: onFailure)
    }

    init(client: HostClient, session: String, localPair: LocalPair, applyCandidate: @escaping @MainActor (ScreenCandidate) async throws -> Void, isMediaConnected: @escaping @MainActor () -> Bool = { false }, onFailure: @escaping @MainActor (Error) -> Void) {
        self.client = client
        self.session = session
        self.applyCandidate = applyCandidate
        self.isMediaConnected = isMediaConnected
        self.onFailure = onFailure
        local = localPair.continuation
        localStream = localPair.stream
    }

    func subscribe() async throws {
        remoteTask = Task { [weak self, client, session] in
            do {
                for try await event in client.screenCandidates(session: session) {
                    guard let self, self.state == .active else { return }
                    try await self.receive(event)
                }
                self?.transportEnded(HostError.unreachable)
            } catch { self?.handle(error) }
        }
        deadline = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(10)) } catch { return }
            self?.fail(HostError.unreachable)
        }
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { c in
                if isReady { c.resume() }
                else if state == .stopped { c.resume(throwing: failure ?? CancellationError()) }
                else { ready = c }
            }
        } onCancel: { Task { @MainActor in self.stop() } }
        deadline?.cancel()
    }

    func check() throws {
        if let failure { throw failure }
        guard state != .stopped else { throw CancellationError() }
    }

    func mediaConnected() {
        guard state == .active else { return }
        mediaEstablished = true
    }

    func installedAnswer() async throws {
        try check()
        guard state == .active else { return }
        answered = true
        answerWaiter?.resume()
        answerWaiter = nil
        localTask = Task { [weak self, localStream, client, session] in
            do {
                var local = CandidateProgress()
                for try await event in localStream {
                    try Task.checkCancellation()
                    try local.accept(event)
                    try await client.screenCandidate(session: session, candidate: event)
                    try Task.checkCancellation()
                    guard let self, self.state == .active else { return }
                    if event.type == .complete { self.localComplete = true }
                }
            } catch { self?.handle(error) }
        }
    }

    private func receive(_ event: ScreenCandidate) async throws {
        switch event.type {
        case .ready:
            guard !isReady else { throw ScreenError.duplicateSubscription }
            isReady = true
            ready?.resume()
            ready = nil
        case .error: throw ScreenError.signalingFailed(event.message ?? "Screen signaling failed.")
        case .candidate, .complete:
            guard isReady, !remote.isComplete else { throw ScreenError.candidatesOutOfOrder }
            try remote.accept(event)
            // Suspend this consumer: its bounded transport stream buffers further events.
            if !answered { await withCheckedContinuation { answerWaiter = $0 } }
            try Task.checkCancellation()
            guard state == .active else { throw CancellationError() }
            if event.type == .candidate { try await applyCandidate(event) }
            // Native Apple WebRTC exposes no usable empty/null end-of-candidates operation.
        }
    }

    private func fail(_ error: Error) {
        guard state == .active else { return }
        failure = error
        stop()
        onFailure(error)
    }

    private func handle(_ error: Error) {
        if let error = error as? HostError, error.isTransient { transportEnded(error) }
        else { fail(error) }
    }

    private func transportEnded(_ error: Error) {
        guard state == .active else { return }
        // The native state may lead its queued main-actor delegate callback during a route change.
        guard mediaEstablished || isMediaConnected() || candidatesComplete else { fail(error); return }
        state = .detached
        finishTasks()
    }

    func stop() {
        guard state != .stopped else { return }
        state = .stopped
        finishTasks()
    }

    private func finishTasks() {
        deadline?.cancel()
        ready?.resume(throwing: failure ?? CancellationError())
        ready = nil
        answerWaiter?.resume()
        answerWaiter = nil
        local.finish()
        localTask?.cancel()
        remoteTask?.cancel()
    }
}
