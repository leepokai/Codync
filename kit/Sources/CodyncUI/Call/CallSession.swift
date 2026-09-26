#if os(iOS)
import AVFoundation
import NaturalLanguage
import Observation
import os
import Speech

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// A hands-free voice loop with one bot, all on the phone: speech is transcribed on device,
/// sent as an ordinary message, and the bot's final replies are read aloud. The host sees text only.
@MainActor
@Observable
final class CallSession {
    enum Phase: Equatable {
        case starting
        case listening
        case speaking
        case failed(String)
    }

    private(set) var phase: Phase = .starting
    /// What you're saying right now (live caption).
    private(set) var heard = ""
    /// The reply being read aloud.
    private(set) var said = ""
    var muted = false {
        didSet {
            guard muted != oldValue, phase == .listening else { return }
            if muted { stopListening() } else { listen() }
        }
    }

    private let send: (String) -> Void
    private let recognizer = SFSpeechRecognizer()
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    /// Bumped per utterance so late callbacks from a cancelled recognition are dropped.
    private var generation = 0
    private var silence: Task<Void, Never>?
    private let synthesizer = AVSpeechSynthesizer()
    private var speechDone: SpeechDone?
    private var ended = false

    /// A pause this long ends what you're saying and sends it.
    static let endOfUtterance: Duration = .milliseconds(1500)

    init(send: @escaping (String) -> Void) {
        self.send = send
        let done = SpeechDone { [weak self] in Task { @MainActor in self?.finishedSpeaking() } }
        synthesizer.delegate = done
        speechDone = done
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.")
        }
        guard await Self.speechAuthorized() else {
            return fail("Allow Speech Recognition for Codync in Settings to talk to your bots.")
        }
        guard let recognizer, recognizer.isAvailable else {
            return fail("Speech recognition isn't available for this language right now.")
        }
        do {
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetoothHFP])
            try audio.setActive(true)
        } catch {
            return fail("Couldn't start audio: \(error.localizedDescription)")
        }
        guard !ended else { return }
        listen()
    }

    func end() {
        ended = true
        stopListening()
        synthesizer.stopSpeaking(at: .immediate)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    /// Reads a reply aloud (the bot's final message), pausing the microphone meanwhile.
    func speak(_ markdown: String) {
        let text = SpokenText.from(markdown)
        guard !ended, !text.isEmpty, phase != .starting, !(phase.isFailed) else { return }
        stopListening()
        said = text
        phase = .speaking
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = Self.voice(for: text)
        synthesizer.speak(utterance)
    }

    /// Cuts the reply short and goes back to listening.
    func interrupt() {
        guard phase == .speaking else { return }
        synthesizer.stopSpeaking(at: .immediate)
        listen()
    }

    // MARK: listening

    private func listen() {
        phase = .listening
        heard = ""
        guard !muted, !ended, let recognizer else { return }
        generation += 1
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
        self.request = request
        let input = engine.inputNode
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0), block: Self.tap(request))
        engine.prepare()
        do {
            try engine.start()
        } catch {
            log.error("audio engine: \(error.localizedDescription, privacy: .public)")
            return fail("Couldn't use the microphone: \(error.localizedDescription)")
        }
        task = recognizer.recognitionTask(with: request, resultHandler: Self.results(self, generation: generation))
    }

    private func stopListening() {
        silence?.cancel()
        silence = nil
        generation += 1
        task?.cancel()
        task = nil
        request?.endAudio()
        request = nil
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
    }

    fileprivate func recognized(_ text: String, final: Bool, generation: Int) {
        guard generation == self.generation, phase == .listening else { return }
        heard = text
        silence?.cancel()
        if final { return submit() }
        silence = Task {
            try? await Task.sleep(for: Self.endOfUtterance)
            guard !Task.isCancelled else { return }
            submit()
        }
    }

    private func submit() {
        let text = heard.trimmingCharacters(in: .whitespacesAndNewlines)
        stopListening()
        if !text.isEmpty { send(text) }
        // Keep listening while the bot works: more messages fold into its next turn.
        listen()
    }

    private func finishedSpeaking() {
        guard phase == .speaking else { return }
        listen()
    }

    private func fail(_ message: String) {
        stopListening()
        phase = .failed(message)
    }

    // MARK: audio-thread closures (built outside the main actor)

    private nonisolated static func tap(_ request: SFSpeechAudioBufferRecognitionRequest) -> AVAudioNodeTapBlock {
        { buffer, _ in request.append(buffer) }
    }

    private nonisolated static func results(_ session: CallSession, generation: Int) -> @Sendable (SFSpeechRecognitionResult?, (any Error)?) -> Void {
        { result, _ in
            guard let result else { return }
            let text = result.bestTranscription.formattedString
            let final = result.isFinal
            Task { @MainActor in session.recognized(text, final: final, generation: generation) }
        }
    }

    private nonisolated static func speechAuthorized() async -> Bool {
        await withCheckedContinuation { done in
            SFSpeechRecognizer.requestAuthorization { done.resume(returning: $0 == .authorized) }
        }
    }

    // MARK: voice

    /// A voice in the reply's own language (replies may be English while the phone is in Chinese).
    private static func voice(for text: String) -> AVSpeechSynthesisVoice? {
        guard let language = NLLanguageRecognizer.dominantLanguage(for: text) else { return nil }
        let code = switch language {
        case .traditionalChinese: "zh-TW"
        case .simplifiedChinese: "zh-CN"
        default: language.rawValue
        }
        let voices = AVSpeechSynthesisVoice.speechVoices().filter { $0.language.hasPrefix(code) }
        // Prefer the phone's own region for the language, then the best quality installed.
        let preferred = voices.filter { $0.language == Locale.current.identifier(.bcp47) }
        return (preferred.isEmpty ? voices : preferred).max { $0.quality.rawValue < $1.quality.rawValue }
    }
}

extension CallSession.Phase {
    var isFailed: Bool { if case .failed = self { true } else { false } }
}

/// Synthesizer delegate: a finished utterance hands back to listening. Cancelled ones don't.
private final class SpeechDone: NSObject, AVSpeechSynthesizerDelegate, Sendable {
    let onFinish: @Sendable () -> Void

    init(onFinish: @escaping @Sendable () -> Void) { self.onFinish = onFinish }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        onFinish()
    }
}
#endif
