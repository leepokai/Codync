import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A provider's recording goes out in 20 s pieces that cover every byte in order; a tail under
/// half a second (noise a model would invent words for) stays with the piece before.
@Test @MainActor func dictationPiecesCoverTheRecording() {
    let piece = Dictation.pieceBytes, min = Dictation.minBytes
    #expect(Dictation.pieces(of: piece / 2) == [0..<piece / 2])
    #expect(Dictation.pieces(of: piece * 2) == [0..<piece, piece..<piece * 2])
    #expect(Dictation.pieces(of: piece * 2 + min) == [0..<piece, piece..<piece * 2, piece * 2..<piece * 2 + min])
    #expect(Dictation.pieces(of: piece + min - 1) == [0..<piece + min - 1])
    #expect(Dictation.pieces(of: piece * 2 + 2) == [0..<piece, piece..<piece * 2 + 2])
}

@Test @MainActor func dictatedWordsGoAfterTheBox() {
    #expect(Dictation.join("", "hello") == "hello")
    #expect(Dictation.join("Fix the bug", "in the parser") == "Fix the bug in the parser")
    #expect(Dictation.join("Fix the bug\n", "now") == "Fix the bug\nnow")
    #expect(Dictation.join("Fix the bug", "") == "Fix the bug")
}

/// A provider's network call that the test answers when it chooses.
@MainActor private final class Pending<Value: Sendable> {
    private var waiting: CheckedContinuation<Value, any Error>?
    private(set) var calls = 0
    /// Calls that have had their answer.
    private(set) var returned = 0

    func call() async throws -> Value {
        calls += 1
        defer { returned += 1 }
        return try await withCheckedThrowingContinuation { waiting = $0 }
    }

    /// Whether the call is waiting for its answer (2 s at most).
    func asked() async -> Bool {
        await until { self.waiting != nil }
    }

    func answer(_ value: Value) {
        waiting?.resume(returning: value)
        waiting = nil
    }

    func fail(_ error: any Error) {
        waiting?.resume(throwing: error)
        waiting = nil
    }
}

/// The phone's microphone, hearing what the test plays it.
@MainActor private final class Mic {
    /// Where the open microphone's audio goes; nil until it's opened.
    var heard: (@Sendable (Data, Float) -> Void)?
}

private struct Offline: LocalizedError {
    var errorDescription: String? { "The computer is offline." }
}

/// Dictation with an engine picked, the computer's answers and the microphone in the test's hands, and a store to dictate through.
@MainActor private func dictating(picked: DictationEngine? = .provider(.openAI),
                                  check: @escaping @MainActor () async throws -> DictationGate = { .open },
                                  transcript: Pending<String> = Pending(), mic: Mic = Mic(),
                                  pillGone: (@MainActor (@escaping @MainActor () -> Void) -> Void)? = nil,
                                  _ body: (Dictation, BotStore) async -> Void) async {
    let suite = "DictationTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let store = BotStore(computer: Computer(id: "dictation", name: "Mac", signKey: ""), storage: storage) { _ in
        throw HostError.unreachable
    }
    defer { store.retire() }
    var service = DictationService(picked: { picked },
                                   check: { _, _ in try await check() },
                                   allowMic: { true },
                                   openMic: { _, heard in
                                       mic.heard = heard
                                       return true
                                   },
                                   transcribe: { _, _, _ in try await transcript.call() })
    if let pillGone {
        service.animate = { change, done in
            change()
            pillGone(done)
        }
    }
    await body(Dictation(service: service), store)
}

private extension Dictation {
    /// Records `bytes` of what `mic` hears and taps the pill's stop; the transcript changes the box through `edit`.
    func dictate(_ bytes: Int, from mic: Mic, on store: BotStore, edit: @escaping ((String) -> String) -> Void) async {
        await start(after: "", model: store, edit: edit)
        mic.heard?(Data(count: bytes), 0.5)
        #expect(await until { self.level > 0 })
        finish()
    }
}

/// The pill's stop has the recording transcribed, and the words go after what's in the box when
/// they arrive, words typed while waiting included. They land once the pill has gone, so the box
/// is sized for its full width.
@MainActor @Test func aRecordingIsTranscribedAfterWhatTheBoxHoldsThen() async {
    let transcript = Pending<String>(), mic = Mic()
    await dictating(transcript: transcript, mic: mic) { dictation, store in
        var box = "Fix the bug"
        var pillUpWhenWritten = false
        await dictation.dictate(Dictation.minBytes, from: mic, on: store) {
            pillUpWhenWritten = dictation.transcribing
            box = $0(box)
        }
        #expect(dictation.transcribing && !dictation.listening)
        #expect(await transcript.asked())
        box += " now"
        transcript.answer("in the parser")
        #expect(await until { box == "Fix the bug now in the parser" })
        #expect(!pillUpWhenWritten)
        #expect(dictation.failure == nil)
    }
}

/// A tap of the mic after the pill has gone but before the words are in the box starts nothing, so
/// they still land.
@MainActor @Test func aTapBeforeTheTranscriptLandsKeepsIt() async {
    let transcript = Pending<String>(), mic = Mic()
    var exit: (@MainActor () -> Void)?
    await dictating(transcript: transcript, mic: mic, pillGone: { exit = $0 }) { dictation, store in
        var box = "Fix the bug"
        await dictation.dictate(Dictation.minBytes, from: mic, on: store) { box = $0(box) }
        #expect(await transcript.asked())
        transcript.answer("in the parser")
        #expect(await until { exit != nil })
        #expect(!dictation.transcribing)
        await dictation.start(after: box, model: store) { box = $0(box) }
        #expect(!dictation.listening)
        exit?()
        #expect(box == "Fix the bug in the parser")
    }
}

/// A stop (a send, another chat) while the transcript is on its way drops it when it comes.
@MainActor @Test func aTranscriptArrivingAfterAStopIsDropped() async {
    let transcript = Pending<String>(), mic = Mic()
    await dictating(transcript: transcript, mic: mic) { dictation, store in
        var box = "Fix the bug"
        await dictation.dictate(Dictation.minBytes, from: mic, on: store) { box = $0(box) }
        #expect(await transcript.asked())
        dictation.stop()
        #expect(!dictation.transcribing)
        transcript.answer("too late")
        #expect(await until { transcript.returned == 1 })
        #expect(box == "Fix the bug")
        #expect(dictation.failure == nil && !dictation.transcribing)
    }
}

/// A failed transcription says why and leaves the box as it was.
@MainActor @Test func aFailedTranscriptionSaysWhy() async {
    let transcript = Pending<String>(), mic = Mic()
    await dictating(transcript: transcript, mic: mic) { dictation, store in
        var box = "Fix the bug"
        await dictation.dictate(Dictation.minBytes, from: mic, on: store) { box = $0(box) }
        #expect(await transcript.asked())
        transcript.fail(Offline())
        #expect(await until { !dictation.transcribing })
        #expect(dictation.failure == "The computer is offline.")
        #expect(box == "Fix the bug")
    }
}

/// Under half a second is a tap of the mic: nothing goes out on the user's key.
@MainActor @Test func aTapOfTheMicSendsNothing() async {
    let transcript = Pending<String>(), mic = Mic()
    await dictating(transcript: transcript, mic: mic) { dictation, store in
        await dictation.dictate(Dictation.minBytes - 1, from: mic, on: store) { _ in }
        #expect(!dictation.transcribing)
        #expect(transcript.calls == 0)
    }
}

/// What the microphone heard before a stop never joins the next recording.
@MainActor @Test func audioHeardBeforeAStopIsDropped() async {
    let mic = Mic()
    await dictating(mic: mic) { dictation, store in
        await dictation.start(after: "", model: store) { _ in }
        let earlier = mic.heard
        dictation.stop()
        await dictation.start(after: "", model: store) { _ in }
        earlier?(Data(count: Dictation.minBytes), 0.9)
        mic.heard?(Data(), 0.5)
        #expect(await until { dictation.level == 0.5 })
        dictation.finish()
        #expect(!dictation.transcribing)
    }
}

/// Without the provider's key on the computer it opens settings, and never opens the microphone.
@MainActor @Test func dictatingWithoutAKeyRequestsSettings() async {
    let key = Pending<DictationGate>(), mic = Mic()
    await dictating(check: { try await key.call() }, mic: mic) { dictation, store in
        let starting = Task { await dictation.start(after: "", model: store) { _ in } }
        #expect(await key.asked())
        key.answer(.needsKey)
        await starting.value
        #expect(dictation.failure == nil && dictation.needsKey)
        #expect(!dictation.listening && mic.heard == nil)
    }
}

/// With the computer out of reach the readiness check says why, and no recording starts.
@MainActor @Test func anUnreachableComputerSaysWhy() async {
    let key = Pending<DictationGate>(), mic = Mic()
    await dictating(check: { try await key.call() }, mic: mic) { dictation, store in
        let starting = Task { await dictation.start(after: "", model: store) { _ in } }
        #expect(await key.asked())
        key.fail(Offline())
        await starting.value
        #expect(dictation.failure == "The computer is offline.")
        #expect(!dictation.listening && mic.heard == nil)
    }
}

/// A stop while the readiness check is on its way wins, whatever the check says: no recording
/// starts and nothing is reported.
@MainActor @Test(arguments: [true, false]) func aStopWhileTheKeyIsCheckedWins(checkAnswers: Bool) async {
    let key = Pending<DictationGate>(), mic = Mic()
    await dictating(check: { try await key.call() }, mic: mic) { dictation, store in
        let starting = Task { await dictation.start(after: "", model: store) { _ in } }
        #expect(await key.asked())
        dictation.stop()
        if checkAnswers { key.answer(.open) } else { key.fail(Offline()) }
        await starting.value
        #expect(!dictation.listening && mic.heard == nil && dictation.failure == nil)
    }
}

/// The local engine with no speech server linked says what the computer reported, as a
/// dictation failure — not the missing-key settings prompt — and never opens the microphone.
@MainActor @Test func localDictationWithoutTheModelSaysWhy() async {
    let mic = Mic()
    let message = "No speech model is linked on Mac. Link one in Codync on that computer: Settings, Voice chat."
    await dictating(picked: .local, check: { .blocked(message) }, mic: mic) { dictation, store in
        await dictation.start(after: "", model: store) { _ in }
        #expect(dictation.failure == message)
        #expect(!dictation.needsKey && !dictation.listening && mic.heard == nil)
    }
}

/// A local-engine recording is transcribed by the linked speech server: `localDictation`
/// says it's ready, the pieces go out with provider "local" and no model, and no usage minutes are
/// counted for any provider.
@MainActor @Test func aLocalRecordingIsTranscribedOnTheComputer() async throws {
    let suite = "DictationTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    await remote.setCanned("localDictation", #"{"ready":true,"message":null,"server":"http://127.0.0.1:8080/v1/audio/transcriptions"}"#)
    await remote.setCanned("voiceTranscribe", #"{"text":"heard on this computer"}"#)
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })

    // The live service's transcription path, with the test's microphone.
    let mic = Mic()
    var service = DictationService.live
    service.picked = { .local }
    service.allowMic = { true }
    service.openMic = { _, heard in
        mic.heard = heard
        return true
    }
    let dictation = Dictation(service: service)
    var box = ""
    await dictation.start(after: "", model: store) { box = $0(box) }
    #expect(dictation.listening, "failure: \(dictation.failure ?? "none")")
    mic.heard?(Data(count: Dictation.minBytes), 0.5)
    #expect(await until { dictation.level > 0 })
    dictation.finish()
    #expect(await until { box == "heard on this computer" }, "failure: \(dictation.failure ?? "none")")

    #expect(await remote.calls.contains("localDictation"))
    #expect(await !remote.calls.contains("voiceStatus"))
    #expect(await until { await remote.calls.contains("voiceTranscribe") })
    let calls = await remote.calls
    let providers = await remote.bodies("provider"), models = await remote.bodies("model")
    #expect(calls.enumerated().contains { calls[$0.offset] == "voiceTranscribe"
        && providers[$0.offset] == "local" && models[$0.offset] == "" })

    let usage = UserDefaults.standard.dictionaryRepresentation().keys.filter { $0.hasPrefix("callUsage.") }
    defer { usage.forEach(UserDefaults.standard.removeObject(forKey:)) }
    #expect(usage.isEmpty)
}

/// The pref's value maps to the engine: "local" is the computer's linked speech server (dictation
/// only, never a call engine), a provider's raw value is its engine, and "device" / unknown stays on-device.
@MainActor @Test func theDictationEnginePrefMapsEngines() {
    defer { UserDefaults.standard.removeObject(forKey: VoiceSettings.dictationEngineKey) }
    #expect(DictationEngine(pref: "local") == .local)
    #expect(DictationEngine(pref: "openai") == .provider(.openAI))
    #expect(DictationEngine(pref: "device") == nil && DictationEngine(pref: "bogus") == nil)

    UserDefaults.standard.set("local", forKey: VoiceSettings.dictationEngineKey)
    #expect(VoiceSettings.dictationEngine == .local)
    UserDefaults.standard.set("gemini", forKey: VoiceSettings.dictationEngineKey)
    #expect(VoiceSettings.dictationEngine == .provider(.gemini))
    UserDefaults.standard.removeObject(forKey: VoiceSettings.dictationEngineKey)
    #expect(VoiceSettings.dictationEngine == nil)

    // Local is not a `VoiceProvider`: nothing that iterates call engines can pick it up.
    #expect(VoiceProvider(rawValue: "local") == nil)
    #expect(VoiceProvider.allCases.allSatisfy { DictationEngine.provider($0) != .local })
}

/// `localDictation` decodes, and a not-ready computer without a reason reads as "update your computer".
@MainActor @Test func localDictationDecodesAndDefaultsToAnUpdateNote() throws {
    let decoder = JSONDecoder()
    let notReady = try decoder.decode(LocalDictation.self, from: Data(#"{"ready":false,"message":"Install the model."}"#.utf8))
    #expect(!notReady.ready && notReady.reason == "Install the model.")
    #expect(try decoder.decode(LocalDictation.self, from: Data(#"{"ready":true,"message":null,"server":"http://127.0.0.1:8080/v1/audio/transcriptions"}"#.utf8)).ready)
    let silent = try decoder.decode(LocalDictation.self, from: Data(#"{"ready":false}"#.utf8))
    #expect(!silent.ready && silent.reason == VoiceStatus.localUpdateMessage)
}

/// A computer from before `localDictation` (it answers 404, unknown method) blocks the local engine
/// with the update note instead of failing.
@MainActor @Test func anOldComputerAsksForAnUpdate() async throws {
    let suite = "DictationTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    await remote.setCanned("localDictation", throwing: HostError.http(404, "unknown method"))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })

    let mic = Mic()
    var service = DictationService.live
    service.picked = { .local }
    service.allowMic = { true }
    service.openMic = { _, heard in
        mic.heard = heard
        return true
    }
    let dictation = Dictation(service: service)
    await dictation.start(after: "", model: store) { _ in }
    #expect(dictation.failure == VoiceStatus.localUpdateMessage)
    #expect(await remote.calls.contains("localDictation"))
    #expect(await !remote.calls.contains("voiceStatus"))
    #expect(!dictation.listening && mic.heard == nil)
}
