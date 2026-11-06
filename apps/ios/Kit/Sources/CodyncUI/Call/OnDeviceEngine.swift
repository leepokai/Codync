import AVFoundation
import CodyncKit
import NaturalLanguage
import Observation
import os
import Speech

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// A hands-free voice loop with one bot, all on the phone: speech is transcribed on device,
/// sent as an ordinary message, and the bot's final replies are read aloud. The host sees text only.
@MainActor
@Observable
final class OnDeviceEngine: VoiceEngine {
    private(set) var phase: VoicePhase = .starting {
        didSet { onActivityChanged?(!ended && phase.holdsCall) }
    }
    var provider: VoiceProvider? { nil }
    /// Keep the transport attached while audio is active, even without a visible UI update.
    @ObservationIgnored var onActivityChanged: ((Bool) -> Void)?
    /// What you're saying right now (live caption).
    private(set) var heard = ""
    /// The reply being read aloud.
    private(set) var said = ""
    /// Microphone loudness, 0…1, while listening.
    private(set) var level: Float = 0
    var muted = false {
        didSet {
            guard muted != oldValue, phase == .listening else { return }
            if muted { stopListening() } else { listen() }
        }
    }

    private let send: (String) -> Void
    /// The language picked in the call settings, else the phone's.
    private let recognizer = UserDefaults.standard.string(forKey: OnDeviceEngine.languageKey)
        .flatMap { $0.isEmpty ? nil : SFSpeechRecognizer(locale: Locale(identifier: $0)) } ?? SFSpeechRecognizer()
    /// Created when the microphone first opens: touching the audio system starts it.
    @ObservationIgnored private lazy var engine = AVAudioEngine()
    /// The microphone tap is on.
    private var tapped = false
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    /// Bumped per utterance so late callbacks from a cancelled recognition are dropped.
    private var generation = 0
    private var silence: Task<Void, Never>?
    private let synthesizer = AVSpeechSynthesizer()
    private var speechDone: SpeechDone?
    private var ended = false
    private var events: AudioSessionEvents?
    /// Replies and notices that came while another app had the audio, said once it's back.
    private var unsaid: [String] = []

    /// Call settings (the gear), kept across calls.
    static let pauseKey = "callPause"
    static let rateKey = "callRate"
    /// On-device recognition language (a locale identifier); empty follows the system.
    nonisolated static let languageKey = "callLanguage"

    /// Settings options: the system's language, then every language the recognizer supports, by name.
    static var languageOptions: [(String, String)] {
        let named = SFSpeechRecognizer.supportedLocales().map { ($0.identifier, Locale.current.localizedString(forIdentifier: $0.identifier) ?? $0.identifier) }
        return [("", "System")] + named.sorted { $0.1.localizedStandardCompare($1.1) == .orderedAscending }
    }

    /// A pause this long ends what you're saying and sends it.
    private var endOfUtterance: Duration {
        let seconds = UserDefaults.standard.double(forKey: Self.pauseKey)
        return .milliseconds(Int((seconds > 0 ? seconds : 1.5) * 1000))
    }

    init(send: @escaping (String) -> Void) {
        self.send = send
        let done = SpeechDone { [weak self] in Task { @MainActor in self?.finishedSpeaking() } }
        synthesizer.delegate = done
        speechDone = done
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.", .microphone)
        }
        guard await Self.speechAuthorized() else {
            return fail("Allow Speech Recognition for Codync in Settings to talk to your bots.", .microphone)
        }
        guard let recognizer, recognizer.isAvailable else {
            return fail("Speech recognition isn't available for this language right now.", .provider)
        }
        guard !ended else { return }
        do {
            try CallAudioSession.activate()
        } catch {
            return fail("Couldn't start audio: \(error.localizedDescription)", .microphone)
        }
        // A route change (AirPods) stops the engine mid-utterance: listen again on the new microphone.
        events = AudioSessionEvents(engine: engine, interrupted: { [weak self] in self?.interrupted($0) },
                                    reconfigured: { [weak self] in
            guard let self, !self.ended, self.phase == .listening, !self.engine.isRunning else { return }
            self.listen()
        })
        listen()
    }

    /// Back from an interruption (it ended, or the user tapped the call's line): what came meanwhile is said, then
    /// listening again.
    func resume() {
        guard !ended, phase == .interrupted else { return }
        try? CallAudioSession.activate()
        listen()
        guard !unsaid.isEmpty else { return }
        let waiting = unsaid.joined(separator: "\n\n")
        unsaid.removeAll()
        speak(reply: waiting)
    }

    private func interrupted(_ change: AudioInterruption) {
        guard !ended else { return }
        switch change {
        case .began:
            guard phase.isActive else { return }
            stopListening()
            synthesizer.stopSpeaking(at: .immediate)
            phase = .interrupted
        case .ended(let shouldResume):
            if shouldResume { resume() }
        }
    }

    func end() {
        ended = true
        onActivityChanged?(false)
        events?.stop()
        events = nil
        stopListening()
        synthesizer.stopSpeaking(at: .immediate)
        CallAudioSession.release()
    }

    /// Reads a reply aloud (the bot's final message), pausing the microphone meanwhile.
    func speak(reply markdown: String) {
        let text = SpokenText.from(markdown)
        guard !ended, !text.isEmpty else { return }
        if phase == .interrupted { return unsaid.append(markdown) }
        guard phase.isActive else { return }
        stopListening()
        said = text
        phase = .speaking
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = Self.voice(for: text)
        let rate = UserDefaults.standard.float(forKey: Self.rateKey)
        utterance.rate = rate > 0 ? rate : AVSpeechUtteranceDefaultSpeechRate
        synthesizer.speak(utterance)
    }

    func announce(_ notice: String) { speak(reply: notice) }

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
        do {
            let format = try MicTap.format(of: input)
            input.installTap(onBus: 0, bufferSize: 1024, format: format, block: Self.tap(request) { [weak self] level in
                Task { @MainActor in self?.level = level }
            })
            tapped = true
            engine.prepare()
            try engine.start()
        } catch {
            log.error("audio engine: \(error.localizedDescription, privacy: .public)")
            return fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone)
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
        if tapped {
            if engine.isRunning { engine.stop() }
            engine.inputNode.removeTap(onBus: 0)
            tapped = false
        }
        level = 0
    }

    fileprivate func recognized(_ text: String, final: Bool, generation: Int) {
        guard generation == self.generation, phase == .listening else { return }
        heard = text
        silence?.cancel()
        if final { return submit() }
        silence = Task {
            try? await Task.sleep(for: endOfUtterance)
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

    private func fail(_ message: String, _ kind: VoiceFailureKind) {
        stopListening()
        phase = .failed(message, kind)
    }

    // MARK: audio-thread closures (built outside the main actor)

    /// Feeds the recognizer and reports the microphone's level, 0…1 (dictation uses it too).
    nonisolated static func tap(_ request: SFSpeechAudioBufferRecognitionRequest,
                                        meter: @escaping @Sendable (Float) -> Void) -> AVAudioNodeTapBlock {
        { buffer, _ in
            request.append(buffer)
            guard let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return }
            let n = Int(buffer.frameLength)
            var sum: Float = 0
            for i in 0..<n { sum += samples[i] * samples[i] }
            // -50 dB (quiet room) … 0 dB mapped to 0…1.
            let db = 10 * log10(max(sum / Float(n), 1e-10))
            meter(min(1, max(0, (db + 50) / 50)))
        }
    }

    private nonisolated static func results(_ session: OnDeviceEngine, generation: Int) -> @Sendable (SFSpeechRecognitionResult?, (any Error)?) -> Void {
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

/// Synthesizer delegate: a finished utterance hands back to listening. Cancelled ones don't.
private final class SpeechDone: NSObject, AVSpeechSynthesizerDelegate, Sendable {
    let onFinish: @Sendable () -> Void

    init(onFinish: @escaping @Sendable () -> Void) { self.onFinish = onFinish }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        onFinish()
    }
}
