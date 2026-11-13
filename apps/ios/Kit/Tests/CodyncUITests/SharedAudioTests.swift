import AVFoundation
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A sound's fetches from the computer that the test answers when it chooses, newest first.
@MainActor private final class HeldLoad {
    private var waiting: [CheckedContinuation<Data?, Never>] = []
    private(set) var calls = 0

    func call() async -> Data? {
        calls += 1
        return await withCheckedContinuation { waiting.append($0) }
    }

    func asked() async -> Bool { await until { !self.waiting.isEmpty } }

    func answer(_ data: Data?) {
        waiting.popLast()?.resume(returning: data)
    }
}

/// A second of silence as a 16-bit mono WAV: a sound `AVAudioPlayer` would play.
private func silentWAV() -> Data {
    let rate: UInt32 = 8000
    let samples = Data(count: Int(rate) * 2)
    func le<T: FixedWidthInteger>(_ value: T) -> Data { withUnsafeBytes(of: value.littleEndian) { Data($0) } }
    var wav = Data("RIFF".utf8) + le(UInt32(36 + samples.count)) + Data("WAVEfmt ".utf8)
    wav += le(UInt32(16)) + le(UInt16(1)) + le(UInt16(1)) + le(rate) + le(rate * 2) + le(UInt16(2)) + le(UInt16(16))
    return wav + Data("data".utf8) + le(UInt32(samples.count)) + samples
}

/// A voice note's player whose fetch `load` holds, on a store; handing the audio back calls `deactivate`.
@MainActor private func playing(deactivate: @escaping @MainActor () -> Void = {},
                                _ body: (SharedAudio, HeldLoad, BotStore, SharedFile) async throws -> Void) async rethrows {
    let suite = "SharedAudioTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in FakeRemote(.connecting) }
    defer { store.retire() }
    let load = HeldLoad()
    let audio = SharedAudio(load: { _, _, _ in await load.call() }, deactivate: deactivate)
    defer { audio.stop() }
    try await body(audio, load, store, SharedFile(id: UUID().uuidString, name: "note.wav", size: 0, sha256: ""))
}

/// The audio session and its interruption notice are shared by the whole process, so these run one at a time.
@Suite(.serialized) @MainActor struct SharedAudioPlayerTests {
    @Test func aSecondTapWhileTheSoundLoadsPlaysItOnce() async {
        await playing { audio, load, store, note in
            let first = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            #expect(audio.state == .loading)
            await audio.toggle(note, entryId: "entry", model: store)
            load.answer(silentWAV())
            await first.value

            #expect(load.calls == 1)
            #expect(audio.state == .playing)
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(audio.state == .paused)
        }
    }

    @Test func aVoiceNoteThatFailsToPlayShowsItsFailureAndTheNextTapLoadsAgain() async {
        await playing { audio, load, store, note in
            let first = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(nil)
            await first.value

            #expect(audio.state == .failed)
            #expect(audio.failure == .load)
            #expect(store.lastError == nil)

            let second = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await second.value
            #expect(load.calls == 2)
            #expect(audio.state == .playing)
            #expect(audio.duration == 1)
        }
    }

    @Test func aVoiceNoteWhoseCardGoesWhileItLoadsNeverPlays() async {
        await playing { audio, load, store, note in
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            audio.stop()
            load.answer(silentWAV())
            await tap.value

            #expect(audio.state == .idle)
            #expect(store.lastError == nil)
        }
    }

    @Test func aStaleLoadFinishingAfterANewerTapLeavesThatTapPlaying() async {
        await playing { audio, load, store, note in
            let old = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            audio.stop()
            let new = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await until { load.calls == 2 })
            load.answer(silentWAV())
            await new.value
            load.answer(silentWAV())
            await old.value

            #expect(audio.state == .playing)
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(audio.state == .paused)
        }
    }

    @Test func seekingHoldsTheVoiceNotesPlayheadToItsLengthAndKeepsItPlayingOrPaused() async {
        await playing { audio, load, store, note in
            audio.seek(to: 0.5)
            #expect(audio.elapsed == 0, "nothing to seek before the sound loads")
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value

            audio.seek(to: 0.5)
            #expect(abs(audio.elapsed - 0.5) < 0.01)
            audio.seek(to: -3)
            #expect(audio.elapsed == 0)
            await audio.toggle(note, entryId: "entry", model: store)
            audio.seek(to: 9)
            #expect(abs(audio.elapsed - (audio.duration ?? 0)) < 0.01)
            #expect(audio.state == .paused)
        }
    }

    @Test func aCallOrSiriTakingTheAudioPausesAPlayingVoiceNote() async {
        await playing { audio, load, store, note in
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value
            #expect(audio.state == .playing)

            NotificationCenter.default.post(name: AVAudioSession.interruptionNotification, object: nil,
                                            userInfo: [AVAudioSessionInterruptionTypeKey: AVAudioSession.InterruptionType.began.rawValue])
            #expect(audio.state == .paused)
            audio.stop()
        }
    }

    @Test func startingASecondVoiceNotePausesTheFirstSoItKeepsTheAudio() async {
        var deactivations = 0
        await playing(deactivate: { deactivations += 1 }) { first, load, store, note in
            let second = SharedAudio(load: { _, _, _ in silentWAV() }, deactivate: { deactivations += 1 })
            let tap = Task { await first.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value
            await second.toggle(note, entryId: "entry", model: store)
            #expect(first.state == .paused)
            #expect(second.state == .playing)

            await first.toggle(note, entryId: "entry", model: store)
            #expect(second.state == .paused, "the note playing before hands it back")
            #expect(first.state == .playing)
            first.stop()
            second.stop()
            #expect(deactivations == 3, "each pause and the stop hand the audio back once")
        }
    }

    @Test func pausingAVoiceNoteAfterACallTookTheAudioLeavesTheCallsSessionActive() async throws {
        let session = AVAudioSession.sharedInstance()
        defer {
            CallAudioSession.release()
            try? session.setCategory(.soloAmbient)
        }
        var deactivations = 0
        try await playing(deactivate: { deactivations += 1 }) { audio, load, store, note in
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(deactivations == 1, "a note's own pause hands the audio back")

            await audio.toggle(note, entryId: "entry", model: store)
            #expect(audio.state == .playing)
            try CallAudioSession.activate()
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(audio.state == .paused)
            #expect(deactivations == 1, "the call that took the audio keeps it")
        }
    }

    @Test func aVoiceNoteAfterACallResetsRecordingOptionsAndReleasesItsSession() async throws {
        let session = AVAudioSession.sharedInstance()
        defer {
            CallAudioSession.release()
            try? session.setActive(false)
            try? session.setCategory(.soloAmbient)
        }
        try CallAudioSession.activate()
        CallAudioSession.release()
        var deactivations = 0
        await playing(deactivate: { deactivations += 1 }) { audio, load, store, note in
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value
            #expect(audio.state == .playing)
            #expect(session.category == .playback)
            #expect(session.mode == .default)
            #expect(session.categoryOptions.isEmpty)
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(deactivations == 1)
        }
    }

    @Test func aVoiceNoteBorrowsLiveDictationButOwnsPlaybackAfterDictationStops() async throws {
        let session = AVAudioSession.sharedInstance()
        defer {
            try? session.setActive(false)
            try? session.setCategory(.soloAmbient)
        }
        let service = DictationService(picked: { .provider(.openAI) }, check: { _, _ in .open }, allowMic: { true },
                                      openMic: { _, _ in true }, transcribe: { _, _, _ in "" })
        let dictation = Dictation(service: service)
        defer { dictation.stop() }
        var deactivations = 0
        try await playing(deactivate: { deactivations += 1 }) { audio, load, store, note in
            await dictation.start(after: "", model: store) { _ in }
            try session.setCategory(.playAndRecord, options: [.duckOthers])
            try session.setActive(true)
            let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
            #expect(await load.asked())
            load.answer(silentWAV())
            await tap.value
            #expect(audio.state == .playing)
            #expect(session.category == .playAndRecord)
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(deactivations == 0)

            dictation.stop()
            await audio.toggle(note, entryId: "entry", model: store)
            #expect(audio.state == .playing)
            #expect(session.category == .playback)
            #expect(!session.categoryOptions.contains(.duckOthers))
            audio.stop()
            #expect(deactivations == 1)
        }
    }
}

@MainActor @Test func aVoiceNoteCardCaptionPutsItsFailureOverTheDownloadThePositionAndTheSize() {
    let size: Int64 = 1_250_000
    #expect(FileDownloadCard.detail(.failed("Couldn't download."), play: .failed, duration: 12, size: size)
        == "Couldn't play. Tap to retry.")
    #expect(FileDownloadCard.detail(.downloading(500_000), play: .failed, size: size) == "Couldn't play. Tap to retry.")
    #expect(FileDownloadCard.detail(.downloading(500_000), play: .playing, duration: 12, size: size) == "500 KB of 1.3 MB")
    #expect(FileDownloadCard.detail(.saved, size: size) == "1.3 MB · Downloaded")
    #expect(FileDownloadCard.detail(.saved, play: .paused, elapsed: 7, duration: 48, size: size) == "0:07 / 0:48")
    #expect(FileDownloadCard.detail(nil, play: .playing, duration: 12, size: size) == "0:00 / 0:12")
    #expect(FileDownloadCard.detail(nil, play: .paused, elapsed: 7, duration: 48, size: size) == "0:07 / 0:48")
    #expect(FileDownloadCard.detail(nil, size: size) == "1.3 MB")
}
