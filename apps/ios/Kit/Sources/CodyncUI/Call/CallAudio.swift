import AVFoundation
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// A realtime call's audio on the phone: the voice-processing microphone (16 kHz PCM16 up) and the speaker (24 kHz
/// PCM16 down). Tests use a scripted one.
@MainActor
protocol CallAudio: AnyObject {
    func permission() async -> Bool
    /// Starts the microphone, handing each buffer (base64 PCM16) and its loudness to `mic`, and the speaker.
    func start(mic: @escaping @MainActor (String, Float) -> Void) throws
    /// Plays a piece of the voice; `done` runs once it has been rendered.
    func play(_ pcm: Data, done: @escaping @MainActor () -> Void)
    /// Drops what's queued to play.
    func stopPlayback()
    /// Starts the microphone and speaker again after an interruption.
    func resume() throws
    func stop()
    /// Another app took the audio, or gave it back.
    var interruption: (@MainActor (AudioInterruption) -> Void)? { get set }
}

/// Another app's audio (a phone call, Siri, an alarm) pausing the call, and giving it back.
enum AudioInterruption {
    case began
    /// With whether the system says to pick up again on our own.
    case ended(shouldResume: Bool)
}

/// The call's audio session, set up the same way for every engine; each activates it as its audio starts and
/// releases it as it stops (OpenAI's WebRTC manages its own, with the same options).
@MainActor
enum CallAudioSession {
    /// The category options of every engine's audio, WebRTC's too.
    static let options: AVAudioSession.CategoryOptions = [.defaultToSpeaker, .allowBluetoothHFP]

    /// The session was activated and not released since: releasing an untouched one would start the audio system
    /// for nothing (in tests that start CoreAudio, which can deadlock on a simulator).
    private static var active = false

    /// Before an engine's audio starts or resumes.
    static func activate() throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .voiceChat, options: options)
        try session.setActive(true)
        active = true
    }

    /// After an engine stops.
    static func release() {
        guard active else { return }
        active = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}

/// The audio session's events a call follows: interruptions, and `engine`
/// reconfiguring (a route change such as AirPods, voice processing settling), which stops it.
@MainActor
final class AudioSessionEvents {
    private var observers: [any NSObjectProtocol] = []

    init(engine: AVAudioEngine?, interrupted: @escaping @MainActor (AudioInterruption) -> Void,
         reconfigured: @escaping @MainActor () -> Void = {}) {
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { note in
            let type = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt).flatMap(AVAudioSession.InterruptionType.init)
            let options = AVAudioSession.InterruptionOptions(rawValue: note.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0)
            MainActor.assumeIsolated {
                switch type {
                case .began: interrupted(.began)
                case .ended: interrupted(.ended(shouldResume: options.contains(.shouldResume)))
                default: break
                }
            }
        })
        if let engine {
            observers.append(center.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { _ in
                MainActor.assumeIsolated { reconfigured() }
            })
        }
    }

    func stop() {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
    }
}

/// The phone's audio for Gemini Live: an echo-cancelled (voice-processing) microphone tap and a player node.
final class VoiceProcessingAudio: CallAudio {
    var interruption: (@MainActor (AudioInterruption) -> Void)?
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private var started = false
    private var events: AudioSessionEvents?
    private var mic: (@MainActor (String, Float) -> Void)?

    private static let playFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 24000, channels: 1, interleaved: false)

    func permission() async -> Bool { await AVAudioApplication.requestRecordPermission() }

    func start(mic: @escaping @MainActor (String, Float) -> Void) throws {
        self.mic = mic
        try CallAudioSession.activate()
        try engine.inputNode.setVoiceProcessingEnabled(true)
        engine.attach(player)
        // The engine stops itself when it reconfigures; without a restart the microphone goes quiet.
        events = AudioSessionEvents(engine: engine, interrupted: { [weak self] in self?.interruption?($0) },
                                    reconfigured: { [weak self] in self?.restart() })
        try run()
        started = true
    }

    /// Wires the speaker and taps the microphone for the current hardware, then starts the engine.
    private func run() throws {
        // Wired again on every start: after the engine reconfigures, the old connections stay in place but
        // play nothing, so the player never finishes its buffers and replies wait forever (#214).
        engine.connect(player, to: engine.mainMixerNode, format: Self.playFormat)
        engine.connect(engine.mainMixerNode, to: engine.outputNode, format: nil)
        let input = engine.inputNode
        input.removeTap(onBus: 0)
        let format = try MicTap.format(of: input)
        input.installTap(onBus: 0, bufferSize: 2048, format: format, block: MicTap.pcm16k(from: format) { [weak self] pcm, level in
            let audio = pcm.base64EncodedString()
            Task { @MainActor in self?.mic?(audio, level) }
        })
        engine.prepare()
        try engine.start()
        player.play()
    }

    private func restart() {
        guard started, !engine.isRunning else { return }
        log.info("call audio engine reconfigured; restarting")
        do { try run() } catch { log.error("call audio restart: \(error.localizedDescription, privacy: .public)") }
    }

    func resume() throws {
        try CallAudioSession.activate()
        if !engine.isRunning { try run() }
    }

    func play(_ pcm: Data, done: @escaping @MainActor () -> Void) {
        guard let format = Self.playFormat, pcm.count >= 2 else { return }
        let frames = pcm.count / 2
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames)),
              let out = buffer.floatChannelData?[0] else { return }
        buffer.frameLength = AVAudioFrameCount(frames)
        pcm.withUnsafeBytes { raw in
            for i in 0..<frames {
                let sample = Int16(littleEndian: raw.loadUnaligned(fromByteOffset: i * 2, as: Int16.self))
                out[i] = Float(sample) / 32768
            }
        }
        player.scheduleBuffer(buffer, completionCallbackType: .dataRendered) { _ in
            Task { @MainActor in done() }
        }
    }

    func stopPlayback() {
        guard started else { return }
        player.stop()
        // A stopped engine (mid-restart, or failing) would throw; restarting plays the player again.
        if engine.isRunning { player.play() }
    }

    func stop() {
        events?.stop()
        events = nil
        if started {
            player.stop()
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            started = false
        }
        CallAudioSession.release()
    }
}
