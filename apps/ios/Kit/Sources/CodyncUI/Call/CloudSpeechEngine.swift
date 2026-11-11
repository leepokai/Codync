import AVFoundation
import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// The provider's speech models without a live conversation: each utterance is cut at a pause,
/// transcribed with the provider's speech-to-text, and sent as an ordinary message; the bot's
/// replies are read aloud with its text-to-speech. Both go through the computer, which holds the key.
@MainActor
@Observable
final class CloudSpeechEngine: VoiceEngine {
    private(set) var phase: VoicePhase = .starting {
        didSet { onActivityChanged?(!ended && phase.holdsCall) }
    }
    private(set) var level: Float = 0
    var muted = false {
        didSet {
            guard muted != oldValue else { return }
            if muted { dropUtterance() }
        }
    }
    let provider: VoiceProvider?
    @ObservationIgnored var onActivityChanged: ((Bool) -> Void)?

    struct Speech {
        let transcribe: @MainActor (Data) async throws -> String
        let speak: @MainActor (String) async throws -> (Data, String)
        let send: @MainActor (String) -> Void
        var stream: (@MainActor (String) async throws -> VoiceSpeechStream)? = nil
    }

    private let speech: Speech
    private let chunk: Int
    /// Created when the microphone first opens: touching the audio system starts it.
    @ObservationIgnored private lazy var engine = AVAudioEngine()
    private var audioRunning = false
    private var ended = false
    private var events: AudioSessionEvents?

    // Listening: PCM since speech began (plus a little before), and when it last went quiet.
    private var preroll = Data()
    private var utterance = Data()
    private var speaking = false
    private var quietSince: Date?
    /// How much of the utterance was actually loud (bytes of PCM above the threshold).
    private var voiced = 0
    /// Playback just ended; the room's echo of it isn't speech.
    private var deafUntil = Date.distantPast
    /// Transcriptions finish in the order they were spoken.
    private var transcribing: Task<Void, Never>?

    // Speaking: the reply's clips, fetched one ahead of the one playing.
    private var queue: [String] = []
    private var player: AVAudioPlayer?
    private var playDone: PlayDone?
    private var next: Task<(Data, String)?, Never>?
    private var speakGeneration = 0
    private var currentFetch: Task<(Data, String)?, Never>?
    private var barge = Data()
    private var streamingTask: Task<Void,Never>?
    private var streamingSource: VoiceSpeechStream?
    private var frameDone: CheckedContinuation<Void,Never>?

    private static let threshold: Float = 0.3
    private static let prerollBytes = MicTap.bytesPerSecond * 3 / 10
    /// An utterance has to fit one channel message once base64-encoded.
    private static let maxBytes = MicTap.bytesPerSecond * 20
    /// Under 0.4 s of voice is a cough or noise, not something to send (and speech-to-text models
    /// invent words for noise).
    private static let minVoicedBytes = MicTap.bytesPerSecond * 4 / 10

    private var endOfUtterance: TimeInterval {
        let seconds = UserDefaults.standard.double(forKey: OnDeviceEngine.pauseKey)
        return seconds > 0 ? seconds : 1.5
    }

    init(provider: VoiceProvider, speech: Speech, localOutput: Bool = false) {
        self.provider = provider
        self.speech = speech
        chunk = localOutput ? 120 : provider.speechChunk
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.", .microphone)
        }
        guard !ended else { return }
        do {
            try CallAudioSession.activate()
            try runMic()
            audioRunning = true
        } catch {
            return fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone)
        }
        // A route change (AirPods) stops the engine: the microphone picks up again on the new route.
        events = AudioSessionEvents(engine: engine, interrupted: { [weak self] in self?.interrupted($0) },
                                    reconfigured: { [weak self] in
            guard let self, !self.ended, self.audioRunning, self.phase != .interrupted, !self.engine.isRunning else { return }
            do { try self.runMic() } catch { self.fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone) }
        })
    }

    /// Taps the microphone in its current format and starts the engine.
    private func runMic() throws {
        let input = engine.inputNode
        if !input.isVoiceProcessingEnabled { try input.setVoiceProcessingEnabled(true) }
        input.removeTap(onBus: 0)
        let format = try MicTap.format(of: input)
        input.installTap(onBus: 0, bufferSize: 2048, format: format, block: MicTap.pcm16k(from: format) { [weak self] pcm, level in
            Task { @MainActor in self?.heard(pcm, level: level) }
        })
        engine.prepare()
        try engine.start()
    }

    /// Back from an interruption (it ended, or the user tapped the call's line): listening once the microphone
    /// delivers, then the replies that waited.
    func resume() {
        guard !ended, phase == .interrupted else { return }
        do {
            try CallAudioSession.activate()
            try runMic()
        } catch {
            return fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone)
        }
        phase = .reconnecting
    }

    private func interrupted(_ change: AudioInterruption) {
        guard !ended else { return }
        switch change {
        case .began:
            guard phase.isActive else { return }
            stopSpeaking()
            dropUtterance()
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
        stopSpeaking()
        transcribing?.cancel()
        if audioRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            audioRunning = false
        }
        CallAudioSession.release()
    }

    func interrupt() {
        guard phase == .speaking || currentFetch != nil || next != nil || streamingTask != nil || !queue.isEmpty else { return }
        stopSpeaking()
        phase = .listening
    }

    func speak(reply: String) { say(SpokenText.from(reply)) }

    func announce(_ notice: String) { say(notice) }

    // MARK: listening

    private func heard(_ pcm: Data, level: Float) {
        // Listening once the microphone actually delivers.
        if !ended, phase.isConnecting {
            phase = .listening
            if !queue.isEmpty, player == nil, next == nil { playNext() }
        }
        guard !ended, !muted else { return }
        if phase == .speaking {
            self.level = level
            if level > Self.threshold + 0.1 { barge.append(pcm) } else { barge.removeAll() }
            guard barge.count >= MicTap.bytesPerSecond / 5 else { return }
            let start = barge
            interrupt()
            dropUtterance()
            preroll = start
            voiced = start.count
            speaking = true
            deafUntil = .distantPast
            return
        }
        guard phase == .listening, Date.now >= deafUntil else { return }
        self.level = level
        if level > Self.threshold {
            speaking = true
            quietSince = nil
            voiced += pcm.count
        } else if speaking, quietSince == nil {
            quietSince = .now
        }
        if speaking {
            utterance.append(pcm)
            let paused = quietSince.map { Date.now.timeIntervalSince($0) >= endOfUtterance } ?? false
            if paused || utterance.count >= Self.maxBytes { submit() }
        } else {
            preroll.append(pcm)
            if preroll.count > Self.prerollBytes { preroll.removeFirst(preroll.count - Self.prerollBytes) }
        }
    }

    private func submit() {
        let wav = MicTap.wav(preroll + utterance)
        let enough = voiced >= Self.minVoicedBytes
        dropUtterance()
        guard enough else { return }
        stopSpeaking()
        let previous = transcribing
        let speech = speech
        transcribing = Task { [weak self] in
            await previous?.value
            do {
                let text = try await speech.transcribe(wav).trimmingCharacters(in: .whitespacesAndNewlines)
                guard let self, !self.ended, !text.isEmpty else { return }
                speech.send(text)
            } catch {
                log.error("transcribe: \(error.localizedDescription, privacy: .public)")
                self?.fail(error.localizedDescription, .provider)
            }
        }
    }

    private func dropUtterance() {
        preroll = Data()
        utterance = Data()
        speaking = false
        quietSince = nil
        voiced = 0
        level = 0
        barge.removeAll()
    }

    // MARK: speaking

    /// Queues text in clips the provider can turn into one channel message each.
    private func say(_ text: String) {
        guard !ended, !phase.isFailed, phase != .starting, !text.isEmpty else { return }
        queue += Self.clips(text, limit: chunk)
        // Interrupted or reconnecting, it waits for the audio to be back.
        if phase.isActive, player == nil, next == nil { playNext() }
    }

    private func playNext() {
        guard !ended else { return }
        if let stream = speech.stream { playStream(stream); return }
        let generation = speakGeneration
        let pending = next ?? fetch(queue.isEmpty ? nil : queue.removeFirst())
        currentFetch = pending
        next = nil
        Task { [weak self] in
            guard let clip = await pending.value else {
                guard let self, generation == self.speakGeneration else { return }
                self.currentFetch = nil
                self.finishedSpeaking()
                return
            }
            guard let self, generation == self.speakGeneration, !self.ended else { return }
            self.currentFetch = nil
            self.play(clip)
            if !self.queue.isEmpty { self.next = self.fetch(self.queue.removeFirst()) }
        }
    }

    private func playStream(_ open: @escaping @MainActor (String) async throws -> VoiceSpeechStream) {
        guard streamingTask == nil else { return }
        let generation = speakGeneration
        streamingTask = Task { [weak self] in
            guard let self else { return }
            do {
                while !queue.isEmpty {
                    try Task.checkCancellation()
                    let source = try await open(queue.removeFirst())
                    streamingSource = source
                    do {
                        var frame = try await source.next()
                        while let clip = frame {
                            try Task.checkCancellation()
                            // Exactly one frame is prefetched while the current WAV plays.
                            let pending = Task { try await source.next() }
                            await withCheckedContinuation { continuation in
                                frameDone = continuation
                                play(clip)
                            }
                            do { frame = try await pending.value }
                            catch { pending.cancel(); throw error }
                        }
                    } catch { await source.cancel(); throw error }
                    await source.cancel()
                    streamingSource = nil
                }
            } catch {
                if !Task.isCancelled, !ended { fail(error.localizedDescription,.provider) }
            }
            guard generation == speakGeneration else { return }
            streamingTask = nil
            finishedSpeaking()
        }
    }

    private func fetch(_ text: String?) -> Task<(Data, String)?, Never> {
        let speech = speech
        return Task { @MainActor [weak self] in
            guard let text else { return nil }
            do {
                return try await speech.speak(text)
            } catch {
                if !Task.isCancelled, let self, !self.ended { self.fail(error.localizedDescription, .provider) }
                return nil
            }
        }
    }

    private func play(_ clip: (Data, String)) {
        // Audio from a file with the right extension: AVAudioPlayer reads the container from it.
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("codync-speech-\(UUID().uuidString).\(clip.1)")
        do {
            try clip.0.write(to: url)
            let player = try AVAudioPlayer(contentsOf: url)
            try? FileManager.default.removeItem(at: url)
            let generation = speakGeneration
            let done = PlayDone { [weak self] in Task { @MainActor in self?.played(generation) } }
            player.delegate = done
            playDone = done
            self.player = player
            dropUtterance()
            phase = .speaking
            if !player.play() { played(generation) }
        } catch {
            try? FileManager.default.removeItem(at: url)
            log.error("play: \(error.localizedDescription, privacy: .public)")
            if let done = frameDone { frameDone = nil; done.resume() } else { playNext() }
        }
    }

    private func played(_ generation: Int) {
        guard generation == speakGeneration else { return }
        player = nil
        if let done = frameDone { frameDone = nil; done.resume(); return }
        if queue.isEmpty, next == nil { finishedSpeaking() } else { playNext() }
    }

    private func finishedSpeaking() {
        player = nil
        deafUntil = .now.addingTimeInterval(0.4)
        if phase == .speaking { phase = .listening }
    }

    private func stopSpeaking() {
        speakGeneration += 1
        streamingTask?.cancel()
        streamingTask = nil
        if let source = streamingSource { Task { await source.cancel() } }
        streamingSource = nil
        frameDone?.resume()
        frameDone = nil
        currentFetch?.cancel()
        currentFetch = nil
        queue.removeAll()
        next?.cancel()
        next = nil
        player?.stop()
        player = nil
    }

    private func fail(_ message: String, _ kind: VoiceFailureKind) {
        end()
        phase = .failed(message, kind)
    }

    /// Sentences grouped into clips of at most `limit` characters (longer sentences are cut).
    static func clips(_ text: String, limit: Int) -> [String] {
        var out: [String] = []
        var current = ""
        var sentence = ""
        func flush(_ s: inout String) {
            let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
            if !t.isEmpty { out.append(t) }
            s = ""
        }
        func add(_ piece: String) {
            if current.count + piece.count > limit { flush(&current) }
            var piece = piece
            while piece.count > limit {
                out.append(String(piece.prefix(limit)))
                piece = String(piece.dropFirst(limit))
            }
            current += piece
        }
        for ch in text {
            sentence.append(ch)
            if ".!?。！？\n".contains(ch) {
                add(sentence)
                sentence = ""
            }
        }
        add(sentence)
        flush(&current)
        return out
    }
}

/// Player delegate: a clip that played to its end moves on to the next one.
private final class PlayDone: NSObject, AVAudioPlayerDelegate, Sendable {
    let onFinish: @Sendable () -> Void

    init(onFinish: @escaping @Sendable () -> Void) { self.onFinish = onFinish }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) { onFinish() }
}
