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

/// The on-device recognizer's results as one transcript. After a pause of a second or two, on-device SFSpeechRecognizer
/// (iOS 18+, macOS 15+) starts its transcript over with only the new words and no final result; the words before stay,
/// and the new ones go after them (the same as the desktop app's `SpeechSegments`).
struct SpeechSegments {
    /// A result this long after the one before follows a pause; revisions come while the words are still being said.
    private static let pause: TimeInterval = 1
    /// The segments before the current one.
    private var done = ""
    private var current = ""
    /// The recognizer closed `current` (a final result, or metadata marking the end of what was said).
    private var ended = false
    /// When `current` came.
    private var last = Date.distantPast

    /// `said` after `before`, with one space between.
    static func join(_ before: String, _ said: String) -> String {
        if before.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return said }
        if said.isEmpty { return before }
        return before.last?.isWhitespace == true ? before + said : before + " " + said
    }

    /// Everything heard so far, given the recognizer's latest result.
    mutating func heard(_ text: String, ends: Bool, at time: Date = .now) -> String {
        if !current.isEmpty, startsOver(text, ends: ends, gap: time.timeIntervalSince(last)) { done = Self.join(done, current) }
        current = text
        ended = ends
        last = time
        return Self.join(done, text)
    }

    private func startsOver(_ text: String, ends: Bool, gap: TimeInterval) -> Bool {
        // After a closed segment, only all of its words going on are the same segment; other words are the next one.
        if ended { return !continues(text, share: 1) }
        if continues(text, share: 0.5) { return false }
        // An empty result keeps the words before it.
        if text.isEmpty { return true }
        // A result that closes the segment is its last revision.
        if ends { return false }
        // ponytail: no end marker, so other words after a pause are a new segment; a revision of more than half
        // the segment's start that comes after a pause stays beside the words it revises.
        return gap >= Self.pause
    }

    /// `text` starts with at least `share` of `current`, ignoring case, spaces and punctuation: the same words, revised
    /// or going on.
    private func continues(_ text: String, share: Double) -> Bool {
        let letters = { (text: String) in text.lowercased().filter { $0.isLetter || $0.isNumber } }
        let before = letters(current)
        let same = zip(before, letters(text)).prefix { $0.0 == $0.1 }.count
        return Double(same) >= Double(before.count) * share
    }
}

extension SFSpeechRecognitionResult {
    /// The recognizer closed what was said: a final result, or (on-device, after a pause) metadata on one that isn't.
    var endsSegment: Bool { isFinal || speechRecognitionMetadata != nil }
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
