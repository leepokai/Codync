import Testing
@testable import CodyncKit

@Test func chatOpeningReportsElapsedMillisecondsOnlyOnce() {
    var timer = OpenToMessages()
    let opened = ContinuousClock.now
    timer.begin(at: opened)
    #expect(timer.finish(at: opened + .milliseconds(1234)) == 1234)
    #expect(timer.finish(at: opened + .seconds(2)) == nil)
}

@Test func aChatLayoutBeforeItsOpeningReportsNothing() {
    var timer = OpenToMessages()
    #expect(timer.finish() == nil)
}
