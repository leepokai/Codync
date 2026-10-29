import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor
struct LiveCallTransportTests {
    private func event(_ type: String) -> Data { Data("{\"type\":\"\(type)\"}".utf8) }
    private func settle() async { for _ in 0..<30 { await Task.yield() } }

    @Test func onlyConversationLiveModelsUseTheNewTransport() {
        #expect(LiveCallTransport.isModel("gpt-live-1"))
        #expect(LiveCallTransport.isModel("gpt-live-1.1"))
        #expect(!LiveCallTransport.isModel("gpt-live-transcribe"))
        #expect(!LiveCallTransport.isModel("gpt-realtime-2.1"))
    }

    @Test func updatesWaitForSessionStartedAndProgressIsCoalesced() async {
        var updates: [String] = []
        var ready = 0
        var failures: [String] = []
        let bridge = LiveCallTransport(event: { _, _ in [] }, update: { id, kind, text in
            #expect(id == "session-1")
            updates.append("\(kind.rawValue):\(text)")
            return []
        }, send: { _ in }, ready: { ready += 1 }, failed: { failures.append($0) })
        bridge.attach("session-1")
        bridge.update(.progress, "old")
        bridge.update(.reply, "answer")
        bridge.update(.progress, "current")
        await settle()
        #expect(updates.isEmpty)
        bridge.receive(event("session.started"))
        bridge.receive(event("session.started"))
        await settle()
        #expect(ready == 1)
        #expect(updates == ["reply:answer", "progress:current"])
        #expect(failures.isEmpty)
    }

    @Test func forwardingPreservesOrderAndNeverForwardsAudio() async {
        var forwarded: [String] = []
        let bridge = LiveCallTransport(event: { id, data in
            #expect(id == "session-2")
            forwarded.append(String(decoding: data, as: UTF8.self))
            return []
        }, update: { _, _, _ in [] }, send: { _ in }, ready: {}, failed: { _ in })
        bridge.attach("session-2")
        for type in ["session.input_transcript.delta", "response.event", "session.output_audio.delta",
                     "session.output_transcript.delta", "session.delegation.created"] {
            bridge.receive(event(type))
        }
        await settle()
        #expect(forwarded == ["session.input_transcript.delta", "response.event", "session.output_transcript.delta",
                             "session.delegation.created"].map { String(decoding: event($0), as: UTF8.self) })
    }

    @Test func anOldSessionsLateToolResultCannotReachANewChannel() async {
        var result: CheckedContinuation<[Data], Error>?
        var sent: [Data] = []
        let bridge = LiveCallTransport(event: { _, _ in
            try await withCheckedThrowingContinuation { result = $0 }
        }, update: { _, _, _ in [] }, send: { sent.append($0) }, ready: {}, failed: { _ in })
        bridge.attach("old-session")
        bridge.receive(event("response.event"))
        await settle()
        #expect(result != nil)
        bridge.stop()
        result?.resume(returning: [event("response.create")])
        await settle()
        #expect(sent.isEmpty)
    }

    @Test func closeUsesLiveCommandAndAcceptsTerminalAcknowledgment() async {
        var sent: [Data] = []
        var failures: [String] = []
        let bridge = LiveCallTransport(event: { _, _ in [] }, update: { _, _, _ in [] },
            send: { sent.append($0) }, ready: {}, failed: { failures.append($0) })
        bridge.attach("session-3")
        bridge.requestClose()
        bridge.requestClose()
        bridge.receive(event("session.closed"))
        bridge.update(.reply, "too late")
        await settle()
        #expect(sent == [event("session.close")])
        #expect(bridge.isClosed)
        #expect(failures.isEmpty)
    }

    @Test func providerErrorsAreVisibleInsteadOfLeavingTheCallConnecting() {
        var failures: [String] = []
        let bridge = LiveCallTransport(event: { _, _ in [] }, update: { _, _, _ in [] },
            send: { _ in }, ready: {}, failed: { failures.append($0) })
        bridge.receive(Data(#"{"type":"error","error":{"message":"Model unavailable"}}"#.utf8))
        #expect(failures == ["Model unavailable"])
    }
}
