import AVFoundation
import CodyncKit
import Foundation

/// A call's audio, the same on every engine (and on Android and the desktop): connecting until the provider confirmed
/// the session and the microphone delivers, then listening or speaking; reconnecting after a drop, interrupted while
/// another app (a phone call, Siri) has the audio, failed when it can't go on.
enum VoicePhase: Equatable {
    case starting
    case listening
    case speaking
    case reconnecting
    case interrupted
    case failed(String, VoiceFailureKind)

    var isFailed: Bool { if case .failed = self { true } else { false } }
    var isActive: Bool { self == .listening || self == .speaking }
    /// The connecting line shows: not ready yet, or ready again soon.
    var isConnecting: Bool { self == .starting || self == .reconnecting }
    /// The call still holds its transport and hears the computer (a reconnect or an interruption doesn't end it).
    var holdsCall: Bool { isActive || self == .reconnecting || self == .interrupted }
}

/// Why a call's engine gave up, for analytics (`voice_call_quality`).
enum VoiceFailureKind: String, Sendable {
    case microphone, connection, provider
}

/// How often a dropped provider connection is tried again before the call falls back to on-device voice: after
/// these waits, in seconds.
@MainActor
enum Reconnect {
    static var delays: [Double] = [1, 2, 4]
}

/// What carries a call's audio: on-device speech (default) or a realtime model on the user's own key.
/// `CallView` only talks to this; nothing on the host changes.
@MainActor
protocol VoiceEngine: AnyObject, Observable {
    var searchResult: CallSearchResult? { get }
    var phase: VoicePhase { get }
    /// Microphone loudness, 0…1, while listening.
    var level: Float { get }
    var muted: Bool { get set }
    /// Where the audio goes when it leaves the phone; nil on device.
    var provider: VoiceProvider? { get }
    /// Keeps the transport attached while audio is active, even without a visible UI update.
    var onActivityChanged: ((Bool) -> Void)? { get set }
    func start() async
    func end()
    /// Cuts what's being said short.
    func interrupt()
    /// A new final reply from the bot to say.
    func speak(reply: String)
    /// A notice from the computer (an approval is waiting) to say.
    func announce(_ notice: String)
    /// What the bot is doing while it works, from the computer: for a realtime voice to mention.
    func progress(_ text: String)
    /// Picks the audio up again after an interruption that didn't end on its own (the user taps the call's line).
    func resume()
}

extension VoiceEngine {
    var searchResult: CallSearchResult? { nil }
    /// What the computer tells the call's voice, handed to the method that says it.
    func hear(_ cue: CallCue) {
        switch cue.kind {
        case .reply: speak(reply: cue.text)
        case .notice: announce(cue.text)
        case .progress: progress(cue.text)
        }
    }

    /// Engines that read replies aloud have nothing to say about progress.
    func progress(_ text: String) {}

    func resume() {}

    /// The engine's name in analytics.
    var analyticsName: AnalyticsEvent.VoiceEngine {
        if self is CloudSpeechEngine { return .cloud }
        return switch provider {
        case .openAI: .openai
        case .gemini: .gemini
        case nil: .onDevice
        }
    }
}

struct VoiceError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

/// The engine picked in the call settings, kept on this device.
enum VoiceSettings {
    static let engineKey = "callEngine"
    static let googleSearchKey = "callGoogleSearch.gemini"
    static var googleSearch: Bool { UserDefaults.standard.bool(forKey: googleSearchKey) }

    static var provider: VoiceProvider? {
        UserDefaults.standard.string(forKey: engineKey).flatMap(VoiceProvider.init(rawValue:))
    }

    /// The same choice for dictation into the message box, picked beside its mic.
    static let dictationEngineKey = "dictationEngine"

    static var dictationProvider: VoiceProvider? {
        UserDefaults.standard.string(forKey: dictationEngineKey).flatMap(VoiceProvider.init(rawValue:))
    }

    static func modelKey(_ provider: VoiceProvider) -> String { "callModel.\(provider.rawValue)" }
    /// The provider's model list as last fetched with the user's key.
    static func modelsKey(_ provider: VoiceProvider) -> String { "callModels.\(provider.rawValue)" }

    static func model(_ provider: VoiceProvider) -> String {
        UserDefaults.standard.string(forKey: modelKey(provider)) ?? defaults(provider).realtime
    }

    static func defaultsKey(_ provider: VoiceProvider) -> String { "callDefaults.\(provider.rawValue)" }

    /// The newest models as the computer last reported them, or the app's own until it has.
    static func defaults(_ provider: VoiceProvider) -> VoiceDefaults {
        UserDefaults.standard.data(forKey: defaultsKey(provider)).flatMap { try? JSONDecoder().decode(VoiceDefaults.self, from: $0) }
            ?? VoiceDefaults(realtime: provider.model, transcribe: provider.transcribeModel, speech: provider.speechModel)
    }

    static func remember(_ defaults: VoiceDefaults?, for provider: VoiceProvider) {
        guard let defaults, let data = try? JSONEncoder().encode(defaults) else { return }
        UserDefaults.standard.set(data, forKey: defaultsKey(provider))
    }

    static func remember(_ status: VoiceStatus) {
        for entry in status.providers { remember(entry.defaults, for: entry.provider) }
    }

    /// `realtime` (a live conversation) or `speech` (speech to text, then the reply read aloud).
    static func modeKey(_ provider: VoiceProvider) -> String { "callMode.\(provider.rawValue)" }

    static func isSpeechMode(_ provider: VoiceProvider) -> Bool {
        UserDefaults.standard.string(forKey: modeKey(provider)) == "speech"
    }

    static func transcribeKey(_ provider: VoiceProvider) -> String { "callSpeechToText.\(provider.rawValue)" }
    static func speechKey(_ provider: VoiceProvider) -> String { "callSpeechModel.\(provider.rawValue)" }

    static func transcribeModel(_ provider: VoiceProvider) -> String {
        UserDefaults.standard.string(forKey: transcribeKey(provider)) ?? defaults(provider).transcribe
    }

    static func speechModel(_ provider: VoiceProvider) -> String {
        UserDefaults.standard.string(forKey: speechKey(provider)) ?? defaults(provider).speech
    }

    /// Keeps a model the user picked; the default isn't stored, so a newer default reaches them.
    static func save(_ model: String, default fallback: String, forKey key: String) {
        if model.isEmpty || model == fallback {
            UserDefaults.standard.removeObject(forKey: key)
        } else {
            UserDefaults.standard.set(model, forKey: key)
        }
    }

    /// gpt-transcribe writes Chinese in Simplified characters whatever it's told: turn it back
    /// into Traditional for someone who reads Traditional (their first Chinese language, or a
    /// Traditional region when none is listed).
    static func inReadersScript(_ text: String) -> String {
        let chinese = Locale.preferredLanguages.lazy.map(Locale.Language.init(identifier:)).first { $0.languageCode == .chinese }
        let traditional = if let chinese { chinese.script == .hanTraditional }
            else { ["TW", "HK", "MO"].contains(Locale.current.region?.identifier ?? "") }
        guard traditional else { return text }
        return text.applyingTransform(StringTransform("Hans-Hant"), reverse: false) ?? text
    }

    /// What a call's engine is built from (`CallView.begin`): a different one picked mid-call takes over the call.
    static func engineChoice(_ provider: VoiceProvider?) -> String {
        guard let provider else { return "device \(UserDefaults.standard.string(forKey: OnDeviceEngine.languageKey) ?? "")" }
        return isSpeechMode(provider)
            ? "\(provider.rawValue) speech \(transcribeModel(provider)) \(speechModel(provider)) \(voice(provider))"
            : "\(provider.rawValue) \(model(provider)) \(voice(provider)) \(provider == .gemini && googleSearch)"
    }

    static func voiceKey(_ provider: VoiceProvider) -> String { "callVoice.\(provider.rawValue)" }

    static func voice(_ provider: VoiceProvider) -> String {
        UserDefaults.standard.string(forKey: voiceKey(provider)).flatMap { provider.voices.contains($0) ? $0 : nil }
            ?? provider.voices[0]
    }
}

enum MicTap {
    /// Bytes per second of what `pcm16k` hands out: 16 kHz mono PCM16.
    static let bytesPerSecond = 16000 * 2

    struct Unavailable: LocalizedError {
        var errorDescription: String? { "no microphone is available right now" }
    }

    /// The microphone's format. With no usable input (no microphone, a route change, an
    /// interruption) it's 0 Hz, and a tap on it raises an exception that ends the app: throws instead.
    static func format(of input: AVAudioInputNode) throws -> AVAudioFormat {
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw Unavailable() }
        return format
    }

    /// Microphone → 16 kHz mono PCM16 (little-endian) with a loudness level, 0…1. Built off the
    /// main actor: the block runs on the audio thread.
    nonisolated static func pcm16k(from format: AVAudioFormat,
                                   out: @escaping @Sendable (Data, Float) -> Void) -> AVAudioNodeTapBlock {
        let target = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16000, channels: 1, interleaved: true)
        let converter = target.flatMap { AVAudioConverter(from: format, to: $0) }
        return { buffer, _ in
            guard let target, let converter else { return }
            let capacity = AVAudioFrameCount(Double(buffer.frameLength) * target.sampleRate / format.sampleRate) + 32
            guard let converted = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
            var given = false
            var error: NSError?
            converter.convert(to: converted, error: &error) { _, status in
                if given {
                    status.pointee = .noDataNow
                    return nil
                }
                given = true
                status.pointee = .haveData
                return buffer
            }
            guard converted.frameLength > 0, let samples = converted.int16ChannelData?[0] else { return }
            let n = Int(converted.frameLength)
            var sum: Float = 0
            for i in 0..<n {
                let v = Float(samples[i]) / 32768
                sum += v * v
            }
            // -50 dB (quiet room) … 0 dB mapped to 0…1.
            let db = 10 * log10(max(sum / Float(n), 1e-10))
            out(Data(bytes: samples, count: n * 2), min(1, max(0, (db + 50) / 50)))
        }
    }

    /// 16 kHz mono PCM16 wrapped in a WAV header.
    static func wav(_ pcm: Data) -> Data {
        var out = Data()
        func le<T: FixedWidthInteger>(_ v: T) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        out.append(contentsOf: Array("RIFF".utf8)); le(UInt32(36 + pcm.count))
        out.append(contentsOf: Array("WAVEfmt ".utf8)); le(UInt32(16)); le(UInt16(1)); le(UInt16(1))
        le(UInt32(16000)); le(UInt32(32000)); le(UInt16(2)); le(UInt16(16))
        out.append(contentsOf: Array("data".utf8)); le(UInt32(pcm.count))
        return out + pcm
    }
}
