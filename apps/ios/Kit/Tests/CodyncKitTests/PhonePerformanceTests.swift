import Testing
@testable import CodyncKit

@Test func chatOpeningRunsFromItsActionToItsOwnFirstMessagesOnce() {
    var timer = OpenToMessages()
    let opened = ContinuousClock.now
    timer.open("b1", at: opened)
    #expect(timer.finish("b2", at: opened + .milliseconds(100)) == nil)
    #expect(timer.finish("b1", at: opened + .milliseconds(1234)) == 1234)
    #expect(timer.finish("b1", at: opened + .seconds(2)) == nil)
}

@Test func leavingOrReopeningAChatDoesNotReportASecondOpening() {
    var timer = OpenToMessages()
    let opened = ContinuousClock.now
    timer.open("b1", at: opened)
    timer.open("b1", at: opened + .milliseconds(500))
    #expect(timer.finish("b1", at: opened + .milliseconds(1234)) == 1234)
    timer.open("b1", at: opened)
    timer.cancel()
    #expect(timer.finish("b1", at: opened + .seconds(2)) == nil)
}
