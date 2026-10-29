import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@Test @MainActor func connectedMediaKeepsRunningAfterSignalingTransportLoss() async throws {
    for nativeCallbackPending in [false, true] {
        let transport = ScreenTransport()
        let local = AsyncThrowingStream<ScreenCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(129))
        let stopped = AsyncStream<Bool>.makeStream()
        local.continuation.onTermination = { _ in stopped.continuation.yield(true); stopped.continuation.finish() }
        var failures: [Error] = []
        let signaling = ScreenSignaling(client: HostClient(transport: transport), session: "session", localPair: local,
                                        applyCandidate: { _ in }, isMediaConnected: { nativeCallbackPending }, onFailure: { failures.append($0) })
        let subscription = Task { try await signaling.subscribe() }
        transport.emit(#"{"type":"ready"}"#)
        try await subscription.value
        try await signaling.installedAnswer()
        if !nativeCallbackPending { signaling.mediaConnected() }
        transport.sink.finish(throwing: HostError.unreachable)
        let detached = await waitForTestEvent(stopped.stream, matching: { $0 })
        try #require(detached)
        try signaling.check()
        #expect(failures.isEmpty)
        if case .terminated = local.continuation.yield(.complete) {} else { Issue.record("local candidate production continued after detaching") }
        let uploads = await transport.uploads
        #expect(uploads.isEmpty, "the detached signaling must never replay uploads after a route change")
        signaling.stop()
    }
}

@Test @MainActor func completedCandidatesSurviveTransportLossOnlyAfterUploadAcknowledgment() async throws {
    for acknowledged in [false, true] {
        let transport = ScreenTransport(gated: true)
        let local = AsyncThrowingStream<ScreenCandidate, Error>.makeStream(bufferingPolicy: .bufferingOldest(129))
        let stopped = AsyncStream<Bool>.makeStream()
        local.continuation.onTermination = { _ in stopped.continuation.yield(true); stopped.continuation.finish() }
        var failures: [Error] = []
        let signaling = ScreenSignaling(client: HostClient(transport: transport), session: "session", localPair: local,
                                        applyCandidate: { _ in }, onFailure: { failures.append($0) })
        let subscription = Task { try await signaling.subscribe() }
        transport.emit(#"{"type":"ready"}"#)
        try await subscription.value
        try await signaling.installedAnswer()
        local.continuation.yield(.complete)
        transport.emit(#"{"type":"complete"}"#)
        let uploading = await waitForTestEvent(transport.uploaded, matching: { $0 == 1 })
        try #require(uploading)
        if acknowledged {
            await transport.acknowledge()
            try await waitForScreenCondition { signaling.candidatesComplete }
        }
        transport.sink.finish()
        let ended = await waitForTestEvent(stopped.stream, matching: { $0 })
        try #require(ended)
        #expect(signaling.isDetached == acknowledged)
        #expect(failures.count == (acknowledged ? 0 : 1))
        if acknowledged { try signaling.check() }
        else {
            await transport.acknowledge()
            #expect(throws: HostError.unreachable) { try signaling.check() }
        }
        signaling.stop()
    }
}

@Test(.timeLimit(.minutes(1))) @MainActor func screenRecoveryWaitsForAReadyRouteWithoutSpendingAttempts() async throws {
    let transport = LateSignalingFailureTransport(gateRecovery: true)
    let session = ScreenSession(client: HostClient(transport: transport), display: nil)
    defer { session.close() }
    await session.start()
    let installations = await transport.answerInstalled("late-1")
    let installed = await waitForTestEvent(installations, matching: { $0 })
    try #require(installed)
    await transport.setRoute(.connecting)
    await transport.failSignaling("late-1")
    let waiting = await waitForTestEvent(transport.routeSubscribed, matching: { $0 })
    try #require(waiting)
    for state in [LinkState.failed("No network"), .connecting, .hostOffline(lastSeen: nil), .connecting] {
        await transport.setRoute(state)
    }
    let before = await transport.preparations
    #expect(before == 1)
    #expect(session.phase == .reconnecting)
    await transport.setRoute(.ready(.relay))
    let recovering = await waitForTestEvent(transport.recoveryRequested, matching: { $0 })
    try #require(recovering)
    let after = await transport.preparations
    #expect(after == 2)
    session.close()
    await transport.resumePreparation()
}

@Test(arguments: [false, true]) @MainActor func stoppedSignalingRoutesEndRecoveryWithTheirFailure(needsUpgrade: Bool) async throws {
    let identity = DeviceIdentity(signing: Curve25519.Signing.PrivateKey(), push: Curve25519.KeyAgreement.PrivateKey())
    let channel = ChannelTransport(computer: randomComputer("Mac"), identity: identity, pairingCode: nil,
                                   dial: { _ in throw HostError.unreachable }, watchesNetwork: false)
    defer { Task { await channel.shutdown() } }
    let failed = AsyncStream<Bool>.makeStream()
    let waiting = Task {
        do {
            try await channel.waitForRoute()
            Issue.record("a stopped route must fail recovery")
        } catch {
            #expect(error as? HostError == (needsUpgrade ? .upgradeRequired : .unreachable))
            failed.continuation.yield(true)
        }
        failed.continuation.finish()
    }
    defer { waiting.cancel() }
    await channel.stopForScreenRecoveryTest(needsUpgrade: needsUpgrade)
    let ended = await waitForTestEvent(failed.stream, matching: { $0 })
    try #require(ended, "a terminal failed route must not wait indefinitely")
}

private extension ChannelTransport {
    func stopForScreenRecoveryTest(needsUpgrade: Bool) {
        isStopped = true
        if needsUpgrade {
            let outcome = upgrade()
            if case let .stop(state) = outcome { setState(state) }
        } else {
            setState(.failed("Channel stopped."))
        }
    }
}
