import Foundation
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

@Test func openTimingUsesHostDiagnosticWireShape() throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys
    let step = PhonePerformance.OpenStep(phase: .catchUp, open: .foreground, ms: 1250, chats: 8, messages: 31)
    let data = try encoder.encode(PhonePerformance.openTiming(step))
    #expect(String(decoding: data, as: UTF8.self)
        == #"{"chats":8,"messages":31,"ms":1250,"open":"foreground","phase":"catch_up","platform":"ios","type":"open_timing"}"#)
}
