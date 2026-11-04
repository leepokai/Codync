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
