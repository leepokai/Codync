import Foundation
import Speech

/// On-device speech recognition as a call and dictation both use it: the recognizer, its request, the permission, and
/// what a failure means (the same as on Android).
enum OnDeviceSpeech {
    static let unavailable = "Speech recognition isn't available for this language right now."
    static let unheard = "Couldn't hear you. Check the microphone and Speech Recognition in Settings."

    /// The language picked in the call settings, else the phone's.
    static func recognizer() -> SFSpeechRecognizer? {
        UserDefaults.standard.string(forKey: OnDeviceEngine.languageKey)
            .flatMap { $0.isEmpty ? nil : SFSpeechRecognizer(locale: Locale(identifier: $0)) } ?? SFSpeechRecognizer()
    }

    /// Words as they're heard, punctuated, recognized on the phone when the recognizer can.
    static func request(for recognizer: SFSpeechRecognizer) -> SFSpeechAudioBufferRecognitionRequest {
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
        return request
    }

    /// Whether the user lets the app use Speech Recognition (asked the first time).
    static func authorized() async -> Bool {
        await withCheckedContinuation { done in
            SFSpeechRecognizer.requestAuthorization { done.resume(returning: $0 == .authorized) }
        }
    }

    /// Only silence was heard ("No speech detected"): dictation just ends, without a message.
    static func isSilence(_ error: any Error) -> Bool {
        let error = error as NSError
        return error.domain == "kAFAssistantErrorDomain" && error.code == 1110
    }
}

/// A call's recognitions that fail right after listening starts: `limit` in a row means listening again won't help
/// (no speech assets, Siri and Dictation off).
struct RecognitionFailures {
    private static let quick: TimeInterval = 2
    private static let limit = 3
    private var quickInARow = 0

    /// A recognition failed `elapsed` seconds in; true once it's broken.
    mutating func failed(after elapsed: TimeInterval) -> Bool {
        quickInARow = elapsed < Self.quick ? quickInARow + 1 : 0
        return quickInARow >= Self.limit
    }

    /// The call stopped a recognition `elapsed` seconds in: one that ran a while works.
    mutating func stopped(after elapsed: TimeInterval) {
        if elapsed >= Self.quick { quickInARow = 0 }
    }

    /// Words came through: the recognizer works.
    mutating func reset() { quickInARow = 0 }
}
