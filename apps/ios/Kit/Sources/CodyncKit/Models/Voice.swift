import Foundation

// Realtime voice on the user's own provider keys (host/src/voice.rs). The keys stay on the
// computer; a call asks the host for a short-lived credential and streams audio to the provider.

public enum VoiceProvider: String, CaseIterable, Codable, Sendable {
    case openAI = "openai"
    case gemini

    public var name: String {
        switch self {
        case .openAI: "OpenAI"
        case .gemini: "Gemini"
        }
    }

    public var keyPage: URL {
        switch self {
        case .openAI: URL(string: "https://platform.openai.com/api-keys")!
        case .gemini: URL(string: "https://aistudio.google.com/apikey")!
        }
    }

    public var keyPrompt: String {
        switch self {
        case .openAI: "sk-…"
        case .gemini: "AIza…"
        }
    }

    /// Used until the user picks one, and until the computer has said what's newest
    /// (`VoiceDefaults`, worked out daily from the provider's list).
    public var model: String {
        switch self {
        case .openAI: "gpt-realtime-2.1"
        case .gemini: "gemini-3.8-live"
        }
    }

    /// Speech-to-text and text-to-speech defaults for the non-realtime mode.
    public var transcribeModel: String {
        switch self {
        case .openAI: "gpt-transcribe"
        case .gemini: "gemini-3.8-flash"
        }
    }

    public var speechModel: String {
        switch self {
        case .openAI: "gpt-4o-mini-tts"
        case .gemini: "gemini-3.8-flash-tts"
        }
    }

    /// Longest stretch of a reply turned into audio at once: each clip has to fit one channel
    /// message (1 MiB). Gemini returns uncompressed PCM, OpenAI AAC.
    public var speechChunk: Int {
        switch self {
        case .openAI: 600
        case .gemini: 150
        }
    }

    public var voices: [String] {
        switch self {
        case .openAI: ["marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse"]
        case .gemini: ["Kore", "Puck", "Charon", "Fenrir", "Aoede", "Leda", "Orus", "Zephyr"]
        }
    }
}

/// The provider's newest model for each voice mode, as the computer last found them.
public struct VoiceDefaults: Codable, Sendable, Equatable {
    public var realtime: String
    public var transcribe: String
    public var speech: String

    public init(realtime: String, transcribe: String, speech: String) {
        self.realtime = realtime
        self.transcribe = transcribe
        self.speech = speech
    }
}

public struct VoiceStatus: Decodable, Sendable {
    public struct Provider: Decodable, Sendable {
        public var provider: VoiceProvider
        public var configured: Bool
        /// Missing from hosts before 2.6.3.
        public var defaults: VoiceDefaults?
    }

    public var providers: [Provider]
    /// Missing from hosts that cannot configure search for a call.
    public var googleSearch: Bool?

    public func configured(_ provider: VoiceProvider) -> Bool {
        providers.first { $0.provider == provider }?.configured == true
    }
}

public struct VoiceModels: Codable, Sendable {
    /// Realtime conversation models.
    public var models: [String]
    public var transcribe: [String]
    public var speech: [String]
    /// Missing from hosts before 2.6.3.
    public var defaults: VoiceDefaults?

    public init(models: [String], transcribe: [String], speech: [String], defaults: VoiceDefaults? = nil) {
        self.models = models
        self.transcribe = transcribe
        self.speech = speech
        self.defaults = defaults
    }
}

public extension HostClient {
    func voiceStatus() async throws -> VoiceStatus { try await call("voiceStatus") }

    /// Checks the key with the provider before the computer keeps it; an empty key removes it.
    func setVoiceKey(_ key: String, for provider: VoiceProvider) async throws -> VoiceStatus {
        try await call("setVoiceKey", ["provider": provider.rawValue, "key": key], timeout: 30)
    }

    /// A short-lived credential for one call's audio session.
    func voiceSession(_ provider: VoiceProvider, model: String, voice: String) async throws -> String {
        struct Session: Decodable { var credential: String }
        let s: Session = try await call("voiceSession", ["provider": provider.rawValue, "model": model, "voice": voice], timeout: 30)
        return s.credential
    }

    /// The models the computer's key can use, per voice mode.
    func voiceModels(_ provider: VoiceProvider) async throws -> VoiceModels {
        try await call("voiceModels", ["provider": provider.rawValue], timeout: 30)
    }

    /// Speech to text for one utterance (a WAV file), with the computer's key.
    func voiceTranscribe(_ provider: VoiceProvider, model: String, wav: Data) async throws -> String {
        struct Text: Decodable { var text: String }
        let t: Text = try await call("voiceTranscribe", ["provider": provider.rawValue, "model": model,
                                                          "audio": wav.base64EncodedString()], timeout: 60)
        return t.text
    }

    /// Text to speech for a stretch of a reply: audio data and its format (`aac` or `wav`).
    func voiceSpeak(_ provider: VoiceProvider, model: String, voice: String, text: String) async throws -> (Data, String) {
        struct Clip: Decodable { var audio: String; var format: String }
        let c: Clip = try await call("voiceSpeak", ["provider": provider.rawValue, "model": model, "voice": voice,
                                                     "text": text], timeout: 90)
        guard let data = Data(base64Encoded: c.audio) else { throw HostError.http(0, "Unreadable audio.") }
        return (data, c.format)
    }
}

/// Minutes of realtime voice used this month, counted on this device: an estimate, not a bill.
public enum VoiceUsage {
    private static func key(_ provider: VoiceProvider, _ date: Date) -> String {
        "callUsage.\(provider.rawValue).\(date.formatted(.iso8601.year().month()))"
    }

    public static func add(seconds: Int, for provider: VoiceProvider, at date: Date = .now) {
        guard seconds > 0 else { return }
        let k = key(provider, date)
        UserDefaults.standard.set(UserDefaults.standard.integer(forKey: k) + seconds, forKey: k)
    }

    public static func minutes(for provider: VoiceProvider, at date: Date = .now) -> Int {
        (UserDefaults.standard.integer(forKey: key(provider, date)) + 59) / 60
    }
}
