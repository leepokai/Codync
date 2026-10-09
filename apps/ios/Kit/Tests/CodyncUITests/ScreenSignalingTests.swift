import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI
@preconcurrency import WebRTC

private actor ScreenTransport: HostTransport {
    nonisolated let events: AsyncThrowingStream<Data, Error>
    nonisolated let sink: AsyncThrowingStream<Data, Error>.Continuation
    private(set) var uploads: [ScreenCandidate] = []
    private var acknowledgment: CheckedContinuation<Void, Never>?
    let gated: Bool

    init(gated: Bool = false) {
        self.gated = gated
        let pair = AsyncThrowingStream<Data, Error>.makeStream()
        events = pair.stream
        sink = pair.continuation
    }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { events }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        struct Body: Decodable { var session: String; var candidate: ScreenCandidate }
        let value = try JSONDecoder().decode(Body.self, from: body)
        #expect(method == "screenCandidate" && value.session == "session")
        uploads.append(value.candidate)
        if gated { await withCheckedContinuation { acknowledgment = $0 } }
        return Data("{}".utf8)
    }

    func acknowledge() { acknowledgment?.resume(); acknowledgment = nil }

    nonisolated func emit(_ json: String) { sink.yield(Data(json.utf8)) }
}

@MainActor
private func waitUntil(attempts: Int = 200, _ condition: @MainActor () async -> Bool) async throws {
    for _ in 0..<attempts {
        let satisfied = await condition()
        if satisfied { return }
        try await Task.sleep(for: .milliseconds(5))
    }
    throw HostError.unreachable
}

private let firstCandidate = ScreenCandidate(candidate: "candidate:1 1 UDP 1 192.0.2.1 5000 typ relay", sdpMLineIndex: 0, sdpMid: "0")
private let secondCandidate = ScreenCandidate(candidate: "candidate:2 1 UDP 1 192.0.2.2 5001 typ relay", sdpMLineIndex: 0, sdpMid: "0")

@Test @MainActor func screenCandidatesWaitForAnswerThenUploadWithSerialAcknowledgments() async throws {
    let transport = ScreenTransport(gated: true)
    let local = AsyncThrowingStream<ScreenCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(129))
    var applied: [ScreenCandidate] = []
    var failures: [String] = []
    let signaling = ScreenSignaling(client: HostClient(transport: transport), session: "session", localPair: local,
                                    applyCandidate: { applied.append($0) }, onFailure: { failures.append($0.localizedDescription) })
    let subscription = Task { try await signaling.subscribe() }
    transport.emit(#"{"type":"ready"}"#)
    transport.emit(#"{"type":"candidate","candidate":"candidate:1 1 UDP 1 192.0.2.1 5000 typ relay","sdpMLineIndex":0,"sdpMid":"0"}"#)
    try await subscription.value
    local.continuation.yield(firstCandidate)
    local.continuation.yield(secondCandidate)
    local.continuation.yield(.complete)
    try await Task.sleep(for: .milliseconds(30))
    let before = await transport.uploads
    #expect(before.isEmpty && applied.isEmpty)
    try await signaling.installedAnswer()
    try await waitUntil { let uploads = await transport.uploads; return uploads.count == 1 }
    #expect(applied == [firstCandidate])
    let one = await transport.uploads
    #expect(one == [firstCandidate])
    await transport.acknowledge()
    try await waitUntil { let uploads = await transport.uploads; return uploads.count == 2 }
    await transport.acknowledge()
    try await waitUntil { let uploads = await transport.uploads; return uploads.count == 3 }
    let all = await transport.uploads
    #expect(all == [firstCandidate, secondCandidate, .complete])
    await transport.acknowledge()
    #expect(failures.isEmpty)
    signaling.stop()
}

@Test @MainActor func remoteCandidatesAwaitNativeCompletionBeforeApplyingTheNext() async throws {
    let transport = ScreenTransport()
    let local = AsyncThrowingStream<ScreenCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(129))
    var applied: [ScreenCandidate] = []
    var acknowledgment: CheckedContinuation<Void, Never>?
    var failure: Error?
    let signaling = ScreenSignaling(client: HostClient(transport: transport), session: "session", localPair: local,
                                    applyCandidate: { event in applied.append(event); await withCheckedContinuation { acknowledgment = $0 } }, onFailure: { failure = $0 })
    let subscription = Task { try await signaling.subscribe() }
    transport.emit(#"{"type":"ready"}"#)
    try await subscription.value
    try await signaling.installedAnswer()
    for candidate in [firstCandidate, secondCandidate, .complete] {
        let data = try JSONEncoder().encode(candidate)
        transport.sink.yield(data)
    }
    try await waitUntil { applied.count == 1 }
    #expect(applied == [firstCandidate])
    acknowledgment?.resume()
    try await waitUntil { applied.count == 2 }
    #expect(applied == [firstCandidate, secondCandidate])
    acknowledgment?.resume()
    // Completion is recorded at app level, never added as a null native candidate.
    try await Task.sleep(for: .milliseconds(20))
    #expect(failure == nil)
    signaling.stop()
}

@Test @MainActor func signalingLossAndCandidateAfterCompletionFailTheAttempt() async throws {
    for disconnected in [false, true] {
        let transport = ScreenTransport()
        let local = AsyncThrowingStream<ScreenCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(129))
        var failures: [Error] = []
        let signaling = ScreenSignaling(client: HostClient(transport: transport), session: "session", localPair: local,
                                        applyCandidate: { _ in Issue.record("unexpected native candidate") }, onFailure: { failures.append($0) })
        let subscription = Task { try await signaling.subscribe() }
        transport.emit(#"{"type":"ready"}"#)
        try await subscription.value
        try await signaling.installedAnswer()
        if disconnected { transport.sink.finish() }
        else {
            transport.emit(#"{"type":"complete"}"#)
            transport.sink.yield(try JSONEncoder().encode(firstCandidate))
        }
        try await waitUntil { !failures.isEmpty }
        #expect(failures.count == 1)
        #expect(throws: HostError.self) { try signaling.check() }
        signaling.stop()
    }
}

private actor FailingSignalingTransport: HostTransport {
    private(set) var preparations = 0

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        switch method {
        case "screenPrepare":
            preparations += 1
            return Data(#"{"session":"attempt-\#(preparations)","iceServers":[],"expiresAt":4102444800000,"trickle":true}"#.utf8)
        case "screenOffer": throw HostError.unreachable
        default: return Data("{}".utf8)
        }
    }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { c in
            c.yield(Data(#"{"type":"ready"}"#.utf8))
            c.yield(Data(#"{"type":"error","message":"Signaling disconnected."}"#.utf8))
            c.finish()
        }
    }

    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

@Test(.timeLimit(.minutes(1))) @MainActor func repeatedSignalingLossUsesFreshSessionsAndExhaustsTheExistingRetryBudget() async throws {
    let transport = FailingSignalingTransport()
    let session = ScreenSession(client: HostClient(transport: transport), display: nil)
    await session.start()
    try await waitUntil(attempts: 2000) {
        if case .failed = session.phase { return true }
        return false
    }
    let preparations = await transport.preparations
    #expect(preparations == 4, "one initial attempt and the existing three recovery attempts")
    session.close()
}

private actor LateSignalingFailureTransport: HostTransport {
    private(set) var preparations = 0
    private var streams: [String: AsyncThrowingStream<Data, Error>.Continuation] = [:]
    private var installedAnswers: Set<String> = []
    private var answerWaiters: [String: AsyncStream<Bool>.Continuation] = [:]
    private let gateRecovery: Bool
    private let preparationGate: AsyncStream<Void>
    private let preparationRelease: AsyncStream<Void>.Continuation
    nonisolated let recoveryRequested: AsyncStream<Bool>
    private let recoverySink: AsyncStream<Bool>.Continuation

    init(gateRecovery: Bool = false) {
        self.gateRecovery = gateRecovery
        let gate = AsyncStream<Void>.makeStream()
        preparationGate = gate.stream
        preparationRelease = gate.continuation
        let requested = AsyncStream<Bool>.makeStream()
        recoveryRequested = requested.stream
        recoverySink = requested.continuation
    }

    func resumePreparation() { preparationRelease.yield(()) }

    func answerInstalled(_ session: String) -> AsyncStream<Bool> {
        let pair = AsyncStream<Bool>.makeStream()
        if installedAnswers.contains(session) {
            pair.continuation.yield(true)
            pair.continuation.finish()
        } else {
            answerWaiters[session] = pair.continuation
        }
        return pair.stream
    }

    func failSignaling(_ session: String) {
        streams.removeValue(forKey: session)?.finish(throwing: HostError.unreachable)
    }

    private func installed(_ session: String) {
        guard installedAnswers.insert(session).inserted else { return }
        if let waiter = answerWaiters.removeValue(forKey: session) {
            waiter.yield(true)
            waiter.finish()
        }
    }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        switch method {
        case "screenPrepare":
            preparations += 1
            if gateRecovery && preparations == 2 {
                recoverySink.yield(true)
                for await _ in preparationGate { break }
                try Task.checkCancellation()
            }
            return Data(#"{"session":"late-\#(preparations)","iceServers":[],"expiresAt":4102444800000,"trickle":true}"#.utf8)
        case "screenOffer":
            struct Offer: Decodable { let session: String; let sdp: String }
            let offer = try JSONDecoder().decode(Offer.self, from: body)
            let answer = try await Self.answer(offer.sdp)
            struct Answer: Encodable { let session: String; let sdp: String }
            return try JSONEncoder().encode(Answer(session: offer.session, sdp: answer))
        case "screenCandidate":
            struct Upload: Decodable { let session: String }
            let upload = try JSONDecoder().decode(Upload.self, from: body)
            // Candidate uploads start only after the phone's native answer completion.
            installed(upload.session)
            return Data("{}".utf8)
        case "screenClose":
            struct Close: Decodable { let session: String }
            let close = try JSONDecoder().decode(Close.self, from: body)
            streams.removeValue(forKey: close.session)?.finish()
            answerWaiters.removeValue(forKey: close.session)?.finish()
            return Data("{}".utf8)
        default: return Data("{}".utf8)
        }
    }

    @MainActor private static func answer(_ offer: String) async throws -> String {
        let config = RTCConfiguration()
        config.sdpSemantics = .unifiedPlan
        config.bundlePolicy = .maxBundle
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard let pc = ScreenSession.factory.peerConnection(with: config, constraints: constraints, delegate: nil) else {
            throw HostError.unreachable
        }
        defer { pc.close() }
        try await pc.setRemoteDescription(RTCSessionDescription(type: .offer, sdp: offer))
        let answer = try await pc.answer(for: constraints)
        try await pc.setLocalDescription(answer)
        return answer.sdp
    }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { c in
            guard case let .screenCandidates(session) = request else { c.finish(); return }
            Task { await self.open(session, sink: c) }
        }
    }

    private func open(_ session: String, sink: AsyncThrowingStream<Data, Error>.Continuation) {
        streams[session] = sink
        sink.yield(Data(#"{"type":"ready"}"#.utf8))
    }

    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

@Test(.timeLimit(.minutes(1))) @MainActor func signalingLossAfterAnswerDoesNotResetRecoveryUntilMediaIsLive() async throws {
    let transport = LateSignalingFailureTransport()
    let session = ScreenSession(client: HostClient(transport: transport), display: nil)
    defer { session.close() }
    await session.start()
    #expect(session.phase == .connecting, "the initial answer was installed successfully")
    for attempt in 1...4 {
        let id = "late-\(attempt)"
        let installations = await transport.answerInstalled(id)
        let installed = await waitForTestEvent(installations, matching: { $0 })
        try #require(installed)
        #expect(session.track != nil, "the negotiated peer exists before its signaling is lost")
        #expect(session.phase == (attempt == 1 ? .connecting : .reconnecting))
        let preparations = await transport.preparations
        #expect(preparations == attempt)
        await transport.failSignaling(id)
    }
    let exhausted = await waitForObservedCondition {
        if case .failed = session.phase { return true }
        return false
    }
    try #require(exhausted)
    let preparations = await transport.preparations
    #expect(preparations == 4, "one initial attempt and three retries, even when answers succeed")
}

@Test(.timeLimit(.minutes(1))) @MainActor func fatalSignalingLossRetiresTheOldPeerBeforePreparingItsReplacement() async throws {
    let transport = LateSignalingFailureTransport(gateRecovery: true)
    let session = ScreenSession(client: HostClient(transport: transport), display: nil)
    defer { session.close() }
    await session.start()
    #expect(session.track != nil)
    let installations = await transport.answerInstalled("late-1")
    let installed = await waitForTestEvent(installations, matching: { $0 })
    try #require(installed)
    await transport.failSignaling("late-1")
    let preparing = await waitForTestEvent(transport.recoveryRequested, matching: { $0 })
    try #require(preparing)
    let preparations = await transport.preparations
    #expect(preparations == 2)
    #expect(session.phase == .reconnecting)
    #expect(session.track == nil, "the failed peer must close even while replacement preparation waits")
    session.close()
    await transport.resumePreparation()
}
