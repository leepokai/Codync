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
        didSet { onActivityChanged?(!ended && phase.isActive) }
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
    }

    private let speech: Speech
    private let chunk: Int
    private let engine = AVAudioEngine()
    private var audioRunning = false
    private var ended = false

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

    init(provider: VoiceProvider, speech: Speech) {
        self.provider = provider
        self.speech = speech
        chunk = provider.speechChunk
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.")
        }
        guard !ended else { return }
        do {
                let session = AVAudioSession.sharedInstance()
                try session.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetoothHFP])
                try session.setActive(true)
            let input = engine.inputNode
            let format = try MicTap.format(of: input)
            input.installTap(onBus: 0, bufferSize: 2048, format: format, block: MicTap.pcm16k(from: format) { [weak self] pcm, level in
                Task { @MainActor in self?.heard(pcm, level: level) }
            })
            engine.prepare()
            try engine.start()
            audioRunning = true
        } catch {
            return fail("Couldn't use the microphone: \(error.localizedDescription)")
        }
        phase = .listening
    }

    func end() {
        ended = true
        onActivityChanged?(false)
        stopSpeaking()
        transcribing?.cancel()
        if audioRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            audioRunning = false
        }
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func interrupt() {
        guard phase == .speaking else { return }
        stopSpeaking()
        phase = .listening
    }

    func speak(reply: String) { say(SpokenText.from(reply)) }

    func announce(_ notice: String) { say(notice) }

    // MARK: listening

    private func heard(_ pcm: Data, level: Float) {
        guard !ended, !muted, phase == .listening, Date.now >= deafUntil else { return }
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
                self?.fail(error.localizedDescription)
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
    }

    // MARK: speaking

    /// Queues text in clips the provider can turn into one channel message each.
    private func say(_ text: String) {
        guard !ended, !phase.isFailed, phase != .starting, !text.isEmpty else { return }
        queue += Self.clips(text, limit: chunk)
        if player == nil, next == nil { playNext() }
    }

    private func playNext() {
        guard !ended else { return }
        let generation = speakGeneration
        let pending = next ?? fetch(queue.isEmpty ? nil : queue.removeFirst())
        next = nil
        Task { [weak self] in
            guard let clip = await pending.value else {
                guard let self, generation == self.speakGeneration else { return }
                self.finishedSpeaking()
                return
            }
            guard let self, generation == self.speakGeneration, !self.ended else { return }
            self.play(clip)
            if !self.queue.isEmpty { self.next = self.fetch(self.queue.removeFirst()) }
        }
    }

    private func fetch(_ text: String?) -> Task<(Data, String)?, Never> {
        let speech = speech
        return Task { @MainActor in
            guard let text else { return nil }
            do {
                return try await speech.speak(text)
            } catch {
                log.error("speak: \(error.localizedDescription, privacy: .public)")
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
            player.play()
        } catch {
            try? FileManager.default.removeItem(at: url)
            log.error("play: \(error.localizedDescription, privacy: .public)")
            playNext()
        }
    }

    private func played(_ generation: Int) {
        guard generation == speakGeneration else { return }
        player = nil
        if queue.isEmpty, next == nil { finishedSpeaking() } else { playNext() }
    }

    private func finishedSpeaking() {
        player = nil
        deafUntil = .now.addingTimeInterval(0.4)
        if phase == .speaking { phase = .listening }
    }

    private func stopSpeaking() {
        speakGeneration += 1
        queue.removeAll()
        next?.cancel()
        next = nil
        player?.stop()
        player = nil
    }

    private func fail(_ message: String) {
        end()
        phase = .failed(message)
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
