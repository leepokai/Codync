import Foundation
import Testing
@testable import CodyncUI

/// A call's recognizer is broken only once it fails right after listening three times running: words, a failure later
/// on, or a recognition the call stopped after a while start the count over. Dictation ends quietly on silence alone.
@Test func aRecognizerIsBrokenOnlyWhenItFailsRightAwayThreeTimesRunning() {
    var failures = RecognitionFailures()
    func failed(after elapsed: TimeInterval...) -> [Bool] { elapsed.map { failures.failed(after: $0) } }
    #expect(failed(after: 0.5, 0.5) == [false, false])
    failures.stopped(after: 1)
    #expect(failed(after: 0.5) == [true])

    failures.reset()
    #expect(failed(after: 0.5, 0.5) == [false, false])
    failures.stopped(after: 30)
    #expect(failed(after: 0.5, 5, 0.5, 0.5) == [false, false, false, false])

    #expect(OnDeviceSpeech.isSilence(NSError(domain: "kAFAssistantErrorDomain", code: 1110)))
    #expect(!OnDeviceSpeech.isSilence(NSError(domain: "kLSRErrorDomain", code: 201)))
}

/// On-device recognition starts its transcript over after a pause without a final result (iOS 18+): the words before
/// stay, revised words replace the last ones, and a closed segment heard again isn't doubled.
@Test func wordsAfterAPauseGoAfterTheWordsBeforeIt() {
    /// Results as (text, ends, seconds since the one before); a pause is a second or more.
    func heard(_ results: [(String, Bool, TimeInterval)]) -> String {
        var segments = SpeechSegments()
        var time = Date(timeIntervalSince1970: 0)
        return results.map { text, ends, gap in
            time += gap
            return segments.heard(text, ends: ends, at: time)
        }.last ?? ""
    }
    // The issue's sequence: the recognizer starts over after the pause.
    #expect(heard([("I fixed", false, 0.1), ("I fixed the parser", false, 0.1), ("now", false, 3), ("now run the tests", false, 0.1)])
        == "I fixed the parser now run the tests")
    #expect(heard([("I fix", false, 0.1), ("I fixed the", false, 0.1), ("I fixed the parser", false, 0.1)]) == "I fixed the parser")
    #expect(heard([("I fixed the parser", false, 0.1), ("I", false, 3), ("I ran the tests", false, 0.1)])
        == "I fixed the parser I ran the tests")
    #expect(heard([("I fixed the parser", false, 0.1), ("", false, 0.1)]) == "I fixed the parser")
    // A revision that shrinks the words is still a revision; after a pause, one keeping the start is too.
    #expect(heard([("Three hundred", false, 0.1), ("300", false, 0.1), ("300 dollars", false, 0.1)]) == "300 dollars")
    #expect(heard([("Hi", false, 0.1), ("I fixed", false, 0.1)]) == "I fixed")
    #expect(heard([("I want to", false, 0.1), ("I want two tickets", false, 1.5)]) == "I want two tickets")
    // A short segment before a pause stays.
    #expect(heard([("Yes", false, 0.1), ("Run", false, 3), ("Run the tests", false, 0.1)]) == "Yes Run the tests")
    // A closed segment: new words go after it, even right away; the same words again aren't doubled.
    #expect(heard([("I fixed the parser", true, 0.1), ("now run all the tests", false, 0.1)])
        == "I fixed the parser now run all the tests")
    #expect(heard([("OK", true, 0.1), ("now", false, 0.1)]) == "OK now")
    #expect(heard([("in the parser", true, 0.1), ("in the parser", true, 0.1)]) == "in the parser")
    #expect(heard([("in the", true, 0.1), ("in the parser", false, 0.1)]) == "in the parser")
    // Only all of the closed words going on are the same segment: a phrase sharing its start is the next one.
    #expect(heard([("run the tests", true, 0.1), ("run the build", false, 0.1)]) == "run the tests run the build")
    // The result that closes a segment, after the pause, is its last revision.
    #expect(heard([("Three hundred", false, 0.1), ("300", true, 1.5), ("now", false, 0.1)]) == "300 now")
}
