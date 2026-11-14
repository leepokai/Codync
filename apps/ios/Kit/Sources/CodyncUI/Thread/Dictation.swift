import AVFoundation
import CodyncKit
import Speech
import SwiftUI

/// What a dictation start finds before the microphone opens.
enum DictationGate {
    case open
    /// No key for a provider: the mic button opens settings instead.
    case needsKey
    /// The computer says why it can't transcribe (no speech server is linked, say).
    case blocked(String)
}

/// What dictating with an engine needs from outside `Dictation`: the engine picked beside the
/// mic, whether it can start, the microphone, a recording's transcript, and the pill's exit.
struct DictationService {
    var picked: @MainActor () -> DictationEngine?
    var check: @MainActor (DictationEngine, BotStore) async throws -> DictationGate
    /// Whether the user lets the app record.
    var allowMic: @MainActor () async -> Bool
    /// Starts `engine`'s microphone, handing `heard` 16 kHz mono PCM16 and its level, 0…1; false when it can't be used.
    var openMic: @MainActor (_ engine: AVAudioEngine, _ heard: @escaping @Sendable (Data, Float) -> Void) -> Bool
    var transcribe: @MainActor (Data, DictationEngine, BotStore) async throws -> String
    /// Runs `change` as the pill's animated exit, then `done` once it has gone.
    var animate: @MainActor (_ change: () -> Void, _ done: @escaping @MainActor () -> Void) -> Void = { change, done in
        withAnimation(Motion.morph, change, completion: done)
    }

    /// The user's setting, this phone's microphone, and their computer's transcription.
    static var live: DictationService {
        DictationService(
            picked: { VoiceSettings.dictationEngine },
            check: { engine, model in
                let client = try await model.ready()
                switch engine {
                case .provider(let provider):
                    let status = try await client.voiceStatus()
                    VoiceSettings.remember(status)
                    return status.configured(provider) ? .open : .needsKey
                case .local:
                    let local = try await client.localDictation()
                    return local.ready ? .open : .blocked(local.reason)
                }
            },
            allowMic: { await AVAudioApplication.requestRecordPermission() },
            openMic: { engine, heard in
                #if DEBUG
                if let clip = DictationClip.fromEnvironment {
                    DictationClip.play(clip, into: heard)
                    return true
                }
                #endif
                return Dictation.openMic(engine, bufferSize: 2048) { MicTap.pcm16k(from: $0, out: heard) }
            },
            transcribe: { pcm, engine, model in
                let said = try await Dictation.transcribe(pcm, engine: engine, on: model)
                // The computer's speech server spends no provider minutes.
                if case .provider(let provider) = engine {
                    VoiceUsage.add(seconds: pcm.count / MicTap.bytesPerSecond, for: provider)
                }
                return said
            })
    }
}

/// Dictation into the message box. With on-device speech the words appear as they're recognized,
/// after what was already typed, and the recording pill's stop, or the recognizer finishing on a
/// pause, stops it; it uses the call's language setting. With an engine picked beside the mic, it
/// records, and the pill's stop has the computer transcribe all of it, on the user's key for a
/// provider or on the speech server linked on the computer; the transcript goes
/// after what's in the box by then.
@MainActor @Observable
final class Dictation {
    private(set) var listening = false {
        didSet { if listening != oldValue { CallAudioSession.dictating = listening } }
    }
    /// Recorded for an engine, waiting for its transcript.
    private(set) var transcribing = false
    /// When it started listening, for the recording pill's time.
    private(set) var startedAt = Date.now
    /// The microphone's level while listening, 0…1.
    private(set) var level: Float = 0
    /// Why the last dictation didn't start or ended early (a permission, the microphone, the network, the recognizer),
    /// until the next.
    var failure: String?
    var needsKey = false
    @ObservationIgnored private lazy var engine = AVAudioEngine()
    @ObservationIgnored private var request: SFSpeechAudioBufferRecognitionRequest?
    @ObservationIgnored private var task: SFSpeechRecognitionTask?
    /// Recording for an engine: what the microphone heard (16 kHz mono PCM16), and whose computer transcribes it.
    @ObservationIgnored private var recording: (engine: DictationEngine, model: BotStore, pcm: Data)?

    /// Changes the box's text: on-device words replace what follows `before`; a transcript goes after the box as it is then.
    @ObservationIgnored private var edit: (((String) -> String) -> Void)?
    /// The box when dictation started.
    @ObservationIgnored private var before = ""
    /// A recording's transcription, from the pill's stop until its words are in the box (after the pill has gone);
    /// `stop()` cancels it so no more pieces go out to the computer. A start before then does nothing.
    @ObservationIgnored private var transcription: Task<Void, Never>?
    /// Bumped per start and stop, so a start still waiting (key check, permission) or a late
    /// result from an earlier recognition is dropped.
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private let service: DictationService

    init(service: DictationService = .live) {
        self.service = service
    }

    /// A transcription request has to fit one channel message once base64-encoded: 20 s of audio
    /// (20.5 s with a short tail still fits).
    static let pieceBytes = MicTap.bytesPerSecond * 20
    /// Under half a second is a tap of the mic, not speech (and speech-to-text models invent words for silence).
    static let minBytes = MicTap.bytesPerSecond / 2
    /// Transcription requests in flight at once, under the channel's and the providers' limits.
    private static let inFlight = 3

    /// `said` after what's already in the box, with one space between.
    static func join(_ before: String, _ said: String) -> String {
        if before.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return said }
        if said.isEmpty { return before }
        return before.last?.isWhitespace == true ? before + said : before + " " + said
    }

    /// Starts with the engine picked beside the mic, after `draft`; a picked engine goes through
    /// `model`'s computer. A `stop()` (another chat, a call, a send) or another tap while it waits wins.
    func start(after draft: String, model: BotStore, edit: @escaping ((String) -> String) -> Void) async {
        guard !listening, transcription == nil else { return }
        failure = nil
        needsKey = false
        generation += 1
        before = draft
        if let engine = service.picked() {
            await record(engine, model: model, generation: generation, edit: edit)
        } else {
            await recognize(generation: generation, edit: edit)
        }
    }

    /// The pill's stop: on-device speech has already typed its words; a recording gets transcribed.
    func finish() {
        guard let recording else { return stop() }
        let edit = edit
        stop()
        guard recording.pcm.count >= Self.minBytes else { return }
        let generation = generation
        transcribing = true
        transcription = Task {
            do {
                let said = try await service.transcribe(recording.pcm, recording.engine, recording.model)
                guard generation == self.generation else { return }
                // The pill gives the box its width back first: text set in the same update is sized for the
                // pill's narrower box and keeps that height.
                service.animate({ transcribing = false }) { [weak self] in
                    guard let self, generation == self.generation else { return }
                    transcription = nil
                    edit? { Self.join($0, said) }
                }
            } catch {
                guard generation == self.generation else { return }
                failure = error.localizedDescription
                transcribing = false
                transcription = nil
            }
        }
    }

    /// Stops now and drops anything still on its way.
    func stop() {
        generation += 1
        transcription?.cancel()
        transcription = nil
        task?.cancel()
        task = nil
        request?.endAudio()
        request = nil
        recording = nil
        transcribing = false
        edit = nil
        // Only a running engine: touching `inputNode` starts the audio system (on every send and chat change), and on
        // a busy simulator its start can time out and abort the process. A tap left on a stopped engine is replaced
        // when the microphone opens again.
        if listening {
            if engine.isRunning {
                engine.inputNode.removeTap(onBus: 0)
                engine.stop()
            }
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        }
        listening = false
        level = 0
    }

    private func recognize(generation: Int, edit: @escaping ((String) -> String) -> Void) async {
        let micAllowed = await AVAudioApplication.requestRecordPermission()
        guard generation == self.generation else { return }
        guard micAllowed else {
            failure = "Allow microphone access in Settings to dictate."
            return
        }
        let speechAllowed = await OnDeviceSpeech.authorized()
        guard generation == self.generation else { return }
        guard speechAllowed else {
            failure = "Allow Speech Recognition in Settings to dictate."
            return
        }
        guard let recognizer = OnDeviceSpeech.recognizer(), recognizer.isAvailable else {
            failure = OnDeviceSpeech.unavailable
            return
        }
        let request = OnDeviceSpeech.request(for: recognizer)
        guard Self.openMic(engine, bufferSize: 1024, tap: { _ in OnDeviceEngine.tap(request) { [weak self] level in
            Task { @MainActor in if self?.listening == true { self?.level = level } }
        } }) else {
            failure = "The microphone isn't available."
            return
        }
        self.edit = edit
        self.request = request
        startedAt = .now
        listening = true
        task = recognizer.recognitionTask(with: request, resultHandler: Self.results(self, generation: generation))
    }

    /// Records for `engine` once the computer says it can transcribe, so nothing said is lost.
    private func record(_ engine: DictationEngine, model: BotStore, generation: Int, edit: @escaping ((String) -> String) -> Void) async {
        do {
            let gate = try await service.check(engine, model)
            guard generation == self.generation else { return }
            switch gate {
            case .open: break
            case .needsKey:
                needsKey = true
                return
            case .blocked(let why):
                failure = why
                return
            }
        } catch {
            if generation == self.generation { failure = error.localizedDescription }
            return
        }
        let allowed = await service.allowMic()
        guard generation == self.generation else { return }
        guard allowed else {
            failure = "Allow microphone access in Settings to dictate."
            return
        }
        guard service.openMic(self.engine, { [weak self] pcm, level in
            Task { @MainActor in self?.recorded(pcm, level: level, generation: generation) }
        }) else {
            failure = "The microphone isn't available."
            return
        }
        self.edit = edit
        recording = (engine, model, Data())
        startedAt = .now
        listening = true
    }

    private func recorded(_ pcm: Data, level: Float, generation: Int) {
        guard generation == self.generation, recording != nil else { return }
        recording?.pcm.append(pcm)
        self.level = level
    }

    /// The call's audio session (a recording tap on a `.record` session fails on some routes) and
    /// `tap` on `engine`'s microphone. False, with everything let go, when the microphone can't be used.
    fileprivate static func openMic(_ engine: AVAudioEngine, bufferSize: AVAudioFrameCount, tap: (AVAudioFormat) -> AVAudioNodeTapBlock) -> Bool {
        let audio = AVAudioSession.sharedInstance()
        do {
            try audio.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetoothHFP, .duckOthers])
            try audio.setActive(true)
            let input = engine.inputNode
            let format = try MicTap.format(of: input)
            input.removeTap(onBus: 0)
            input.installTap(onBus: 0, bufferSize: bufferSize, format: format, block: tap(format))
            engine.prepare()
            try engine.start()
            return true
        } catch {
            engine.inputNode.removeTap(onBus: 0)
            if engine.isRunning { engine.stop() }
            try? audio.setActive(false, options: .notifyOthersOnDeactivation)
            return false
        }
    }

    /// Byte ranges of a recording's 20 s pieces; a tail under half a second stays with the piece before.
    static func pieces(of count: Int) -> [Range<Int>] {
        var starts = Array(stride(from: 0, to: count, by: pieceBytes))
        if starts.count > 1, let last = starts.last, count - last < minBytes { starts.removeLast() }
        return zip(starts, starts.dropFirst() + [count]).map { $0..<$1 }
    }

    /// The recording in pieces, a few transcribed at a time and joined in order.
    fileprivate static func transcribe(_ pcm: Data, engine: DictationEngine, on store: BotStore) async throws -> String {
        // Asked for now, not at the start: a reconnect while recording replaces the client.
        let client = try await store.ready()
        let request = engine.transcribeRequest

        let wavs = pieces(of: pcm.count).map { MicTap.wav(pcm.subdata(in: $0)) }
        let texts = try await withThrowingTaskGroup(of: (Int, String).self) { group in
            var texts = [String](repeating: "", count: wavs.count)
            func keep(_ result: (Int, String)) { texts[result.0] = result.1.trimmingCharacters(in: .whitespacesAndNewlines) }
            for (i, wav) in wavs.enumerated() {
                if i >= inFlight, let done = try await group.next() { keep(done) }
                try Task.checkCancellation()
                group.addTask { (i, try await client.voiceTranscribe(provider: request.provider, model: request.model, wav: wav)) }
            }
            for try await done in group { keep(done) }
            return texts
        }
        return VoiceSettings.inReadersScript(texts.filter { !$0.isEmpty }.reduce("", join))
    }

    private func recognized(_ text: String, final: Bool, generation: Int) {
        guard generation == self.generation else { return }
        let before = before
        edit? { _ in Self.join(before, text) }
        if final { stop() }
    }

    private nonisolated static func results(_ dictation: Dictation, generation: Int) -> @Sendable (SFSpeechRecognitionResult?, (any Error)?) -> Void {
        { result, error in
            let text = result?.bestTranscription.formattedString
            let final = result?.isFinal ?? false
            // Silence just ends it; any other failure says why instead of ending without a word.
            let failure = error.flatMap { OnDeviceSpeech.isSilence($0) ? nil : OnDeviceSpeech.unheard }
            let failed = error != nil
            Task { @MainActor in
                if let text { dictation.recognized(text, final: final, generation: generation) }
                guard failed, generation == dictation.generation else { return }
                dictation.failure = failure
                dictation.stop()
            }
        }
    }
}

private extension DictationEngine {
    /// The `voiceTranscribe` arguments an engine asks for: a provider's speech-to-text model on
    /// the user's key, or the computer's own model (`provider "local"`).
    var transcribeRequest: (provider: String, model: String) {
        switch self {
        case .provider(let provider): (provider.rawValue, VoiceSettings.transcribeModel(provider))
        case .local: ("local", "")
        }
    }
}
