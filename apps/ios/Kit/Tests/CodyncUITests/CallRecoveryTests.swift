import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

// Recorded Gemini Live and OpenAI Realtime event streams replayed through the engines, with a scripted socket and
// scripted audio (no CoreAudio in tests): phases, cues, resumption, interruptions, the #214 shapes and failure.

/// A Gemini Live socket the test plays: it hands the engine the server's frames and keeps what the engine sent.
@MainActor
final class ScriptedSocket: LiveSocket {
    private(set) var sent: [[String: Any]] = []
    private var frames: [String] = []
    private var waiting: CheckedContinuation<String, Error>?
    private var closed = false
    var closeCode = 0
    var closeReason = ""

    func send(_ text: String) {
        sent.append((try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] ?? [:])
    }

    func receive() async throws -> String {
        if !frames.isEmpty { return frames.removeFirst() }
        if closed { throw URLError(.networkConnectionLost) }
        return try await withCheckedThrowingContinuation { waiting = $0 }
    }

    func close() { drop() }

    /// The server sends `json`.
    func play(_ json: String) {
        if let waiting {
            self.waiting = nil
            waiting.resume(returning: json)
        } else {
            frames.append(json)
        }
    }

    /// The connection drops (with the server's close code and reason, when it gave them).
    func drop(code: Int = 0, reason: String = "") {
        closeCode = code
        closeReason = reason
        closed = true
        if let waiting {
            self.waiting = nil
            waiting.resume(throwing: URLError(.networkConnectionLost))
        }
    }

    func sent(_ key: String) -> [[String: Any]] { sent.filter { $0[key] != nil } }
}

@MainActor
final class ScriptedSockets {
    private(set) var all: [ScriptedSocket] = []
    var last: ScriptedSocket? { all.last }

    func open(_ url: URL) -> any LiveSocket {
        let socket = ScriptedSocket()
        all.append(socket)
        return socket
    }
}

/// The phone's audio, scripted: the test makes the microphone deliver and decides whether playback finishes.
@MainActor
final class ScriptedAudio: CallAudio {
    var interruption: (@MainActor (AudioInterruption) -> Void)?
    private var mic: (@MainActor (String, Float) -> Void)?
    private var playing: [@MainActor () -> Void] = []
    private(set) var resumed = 0
    var started: Bool { mic != nil }

    func permission() async -> Bool { true }
    func start(mic: @escaping @MainActor (String, Float) -> Void) throws { self.mic = mic }
    func play(_ pcm: Data, done: @escaping @MainActor () -> Void) { playing.append(done) }
    func stopPlayback() { playing.removeAll() }
    func resume() throws { resumed += 1 }
    func stop() { mic = nil }

    /// The microphone delivers a buffer.
    func deliver(level: Float = 0.5) { mic?(Data(count: 640).base64EncodedString(), level) }
    /// What was queued has played.
    func finish() {
        let done = playing
        playing.removeAll()
        done.forEach { $0() }
    }
}

/// A Gemini call on a scripted computer (`FakeRemote` answers `startCall` with a fresh credential each time).
@MainActor
final class GeminiCall {
    let fake = FakeRemote(.ready(.direct))
    let store: BotStore
    let audio = ScriptedAudio()
    let sockets = ScriptedSockets()
    let engine: GeminiLiveEngine
    private let suite: String

    init() async throws {
        let (storage, suite) = context()
        self.suite = suite
        let fake = self.fake
        store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
        store.setActive(true)
        let link = CallLink(model: store, botId: "b1", call: UUID().uuidString)
        let sockets = self.sockets
        engine = GeminiLiveEngine(model: "gemini-live", voice: "Kore", link: link, audio: audio) { sockets.open($0) }
        #expect(await until { await fake.subscribed })
        await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
        #expect(await until { self.store.connection == .online })
    }

    /// The engine started and its first session is set up, with the microphone delivering.
    func ready() async throws {
        Task { await engine.start() }
        #expect(await until { self.sockets.last?.sent("setup").isEmpty == false })
        sockets.last?.play(#"{"setupComplete":{}}"#)
        #expect(await until { self.audio.started })
        audio.deliver()
        #expect(await until { self.engine.phase == .listening })
    }

    func close() {
        engine.end()
        store.retire()
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
    }
}

@Suite(.serialized) @MainActor struct CallRecoveryTests {
    init() {
        Reconnect.delays = [0.01, 0.01, 0.01]
        GeminiLiveEngine.stallTimeout = .milliseconds(60)
    }

    @Test func geminiIsReadyOnlyOnceTheMicrophoneDelivers() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        Task { await call.engine.start() }
        #expect(await until { call.sockets.last?.sent("setup").isEmpty == false })
        call.sockets.last?.play(#"{"setupComplete":{}}"#)
        try await Task.sleep(for: .milliseconds(50))
        #expect(call.engine.phase == .starting, "setup alone isn't ready: the microphone hasn't delivered")
        call.audio.deliver()
        #expect(await until { call.engine.phase == .listening })
        #expect(call.sockets.last?.sent("realtimeInput").isEmpty == false, "the microphone's audio goes out")
    }

    @Test func aDroppedConnectionResumesWithoutHangingUp() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.sockets.last?.play(#"{"sessionResumptionUpdate":{"newHandle":"h1","resumable":true}}"#)
        try await Task.sleep(for: .milliseconds(20))
        call.sockets.last?.drop(reason: "network")
        #expect(await until { call.engine.phase == .reconnecting })
        // A reply from the computer while it reconnects waits for the new session.
        call.engine.speak(reply: "The tests pass.")
        #expect(await until { call.sockets.all.count == 2 })
        let setup = try #require(call.sockets.last?.sent("setup").first?["setup"] as? [String: Any])
        #expect((setup["sessionResumption"] as? [String: Any])?["handle"] as? String == "h1")
        #expect(await call.fake.calls.filter { $0 == "startCall" }.count == 2, "a fresh credential from the computer")
        call.sockets.last?.play(#"{"setupComplete":{}}"#)
        #expect(await until { call.sockets.last?.sent("clientContent").count == 1 }, "the waiting reply is said")
        #expect(call.engine.phase == .reconnecting, "ready again once the microphone delivers")
        call.audio.deliver()
        #expect(await until { call.engine.phase == .listening })
    }

    @Test func goAwayCarriesOnInAResumedSession() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.sockets.last?.play(#"{"sessionResumptionUpdate":{"newHandle":"h2","resumable":true}}"#)
        call.sockets.last?.play(#"{"goAway":{"timeLeft":"5s"}}"#)
        #expect(await until { call.sockets.all.count == 2 })
        #expect(call.engine.phase == .listening, "a planned handover isn't a reconnect")
    }

    @Test func reconnectsRunOutAndTheCallFails() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        for attempt in 1...Reconnect.delays.count {
            call.sockets.last?.drop()
            #expect(await until { call.sockets.all.count == attempt + 1 })
        }
        call.sockets.last?.drop()
        #expect(await until { call.engine.phase.isFailed })
        #expect(call.engine.phase == .failed("Lost the connection to Gemini.", .connection))
    }

    @Test func aRefusedSessionFailsAtOnce() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.sockets.last?.drop(code: 1008, reason: "API key not valid.")
        #expect(await until { call.engine.phase.isFailed })
        #expect(call.engine.phase == .failed("API key not valid.", .provider), "trying again won't help")
        #expect(call.sockets.all.count == 1)
    }

    @Test func goAwayMidTurnDoesNotLeaveTheCallSpeaking() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.sockets.last?.play(#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AAAAAA=="}}]}}}"#)
        #expect(await until { call.engine.phase == .speaking })
        call.sockets.last?.play(#"{"goAway":{"timeLeft":"5s"}}"#)
        #expect(await until { call.sockets.all.count == 2 })
        call.audio.finish()
        #expect(await until { call.engine.phase == .listening }, "the resumed session won't complete that turn")
        call.engine.speak(reply: "Deployed.")
        call.sockets.last?.play(#"{"setupComplete":{}}"#)
        #expect(await until { call.sockets.last?.sent("clientContent").count == 1 }, "replies aren't held behind it")
    }

    @Test func playbackThatNeverFinishesGoesBackToListening() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        var stalls = 0
        call.engine.onStall = { stalls += 1 }
        try await call.ready()
        call.sockets.last?.play(#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AAAAAA=="}}]}}}"#)
        #expect(await until { call.engine.phase == .speaking })
        call.sockets.last?.play(#"{"serverContent":{"turnComplete":true}}"#)
        call.engine.speak(reply: "Deployed.")
        // The speaker never reports the buffer rendered (#214): the watchdog takes the call back.
        #expect(await until { call.engine.phase == .listening })
        #expect(stalls == 1)
        #expect(await until { call.sockets.last?.sent("clientContent").count == 1 }, "the reply isn't stuck behind it")
    }

    @Test func playbackThatFinishesReturnsToListeningAtOnce() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        var stalls = 0
        call.engine.onStall = { stalls += 1 }
        try await call.ready()
        call.sockets.last?.play(#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AAAAAA=="}}]}}}"#)
        #expect(await until { call.engine.phase == .speaking })
        call.sockets.last?.play(#"{"serverContent":{"turnComplete":true}}"#)
        try await Task.sleep(for: .milliseconds(20))
        call.audio.finish()
        #expect(await until { call.engine.phase == .listening })
        try await Task.sleep(for: .milliseconds(120))
        #expect(stalls == 0)
    }

    @Test func anInterruptionPausesTheCallAndItPicksUpAgain() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.audio.interruption?(.began)
        #expect(call.engine.phase == .interrupted)
        #expect(call.engine.phase.holdsCall, "the call isn't over")
        call.engine.speak(reply: "Done.")
        try await Task.sleep(for: .milliseconds(30))
        #expect(call.sockets.last?.sent("clientContent").isEmpty == true, "nothing is said while another app has the audio")
        call.audio.interruption?(.ended(shouldResume: true))
        #expect(call.audio.resumed == 1)
        #expect(call.engine.phase == .reconnecting)
        call.audio.deliver()
        #expect(await until { call.engine.phase == .listening })
        #expect(await until { call.sockets.last?.sent("clientContent").count == 1 })
    }

    @Test func theVoiceIsNotHeardWhileAnotherAppHasTheAudio() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.audio.interruption?(.began)
        // A tool's answer that was already on its way gets a spoken reply meanwhile.
        call.sockets.last?.play(#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AAAAAA=="}}]}}}"#)
        call.sockets.last?.play(#"{"serverContent":{"turnComplete":true}}"#)
        try await Task.sleep(for: .milliseconds(30))
        #expect(call.engine.phase == .interrupted, "still paused, so a tap or the interruption's end picks it up")
        call.audio.interruption?(.ended(shouldResume: true))
        #expect(call.audio.resumed == 1)
    }

    @Test func anInterruptionThatDoesNotEndWaitsForATap() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        try await call.ready()
        call.audio.interruption?(.began)
        call.audio.interruption?(.ended(shouldResume: false))
        #expect(call.engine.phase == .interrupted, "without shouldResume the call stays paused")
        call.engine.resume()
        call.audio.deliver()
        #expect(await until { call.engine.phase == .listening })
    }

    @Test func openAIPhasesFollowItsEvents() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        let engine = OpenAIRealtimeEngine(model: "gpt-realtime", voice: "marin", link: CallLink(model: call.store, botId: "b1", call: "c"))
        defer { engine.end() }
        let event = { (type: String) in engine.received(Data(#"{"type":"\#(type)"}"#.utf8)) }
        event("output_audio_buffer.started")
        #expect(engine.phase == .starting, "nothing plays before the session is set up")
        event("session.updated")
        #expect(engine.phase == .listening)
        event("output_audio_buffer.started")
        #expect(engine.phase == .speaking)
        event("output_audio_buffer.stopped")
        #expect(engine.phase == .listening)
    }

    @Test func openAIReconnectsADroppedSessionAndGivesUpAfterThreeTries() async throws {
        let call = try await GeminiCall()
        defer { call.close() }
        let link = CallLink(model: call.store, botId: "b1", call: "c")
        let starting = OpenAIRealtimeEngine(model: "gpt-realtime", voice: "marin", link: link)
        starting.iceChanged(.failed)
        #expect(starting.phase == .failed("Couldn't reach OpenAI.", .connection), "a call that never started falls back")

        let engine = OpenAIRealtimeEngine(model: "gpt-realtime", voice: "marin", link: link)
        defer { engine.end() }
        engine.received(Data(#"{"type":"session.updated"}"#.utf8))
        #expect(engine.phase == .listening)
        // The computer can't be reached for the new sessions either.
        await call.fake.fail("startCall", times: 10)
        engine.iceChanged(.failed)
        #expect(engine.phase == .reconnecting, "tried again, not hung up")
        #expect(engine.phase.holdsCall)
        #expect(await until { engine.phase.isFailed })
        guard case .failed(_, let kind) = engine.phase else { return }
        #expect(kind == .connection)
        #expect(await call.fake.calls.filter { $0 == "startCall" }.count == Reconnect.delays.count, "a new session each try")
    }

    @Test func qualityReportsReadyTimeReconnectsStallsAndFailure() async throws {
        var events: [AnalyticsEvent] = []
        let quality = CallQuality { events.append($0) }
        quality.begin(.gemini)
        quality.phase(.listening)
        quality.phase(.reconnecting)
        quality.phase(.listening)
        quality.stalled()
        quality.phase(.failed("Lost the connection to Gemini.", .connection))
        quality.begin(.onDevice)
        guard case let .voiceCallQuality(engine, readyMs, reconnects, stuck, failure) = try #require(events.first) else {
            Issue.record("not a quality event")
            return
        }
        #expect(engine == .gemini && readyMs != nil && reconnects == 1 && stuck == .speaking && failure == .connection)
        quality.report()
        #expect(events.count == 2, "one report per engine")
    }

    @Test func qualityCountsAPhaseStuckConnecting() async throws {
        var events: [AnalyticsEvent] = []
        var now = ContinuousClock.now
        let quality = CallQuality(capture: { events.append($0) }, now: { now })
        quality.begin(.openai)
        now += CallQuality.stuckAfter
        quality.report()
        #expect(events == [.voiceCallQuality(.openai, readyMs: nil, reconnects: 0, stuck: .connecting, failure: .none)])
    }
}
