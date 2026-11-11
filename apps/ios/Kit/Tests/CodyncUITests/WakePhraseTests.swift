import Foundation
import Testing
@testable import CodyncUI

@Test func wakePhraseRequiresAPrefixAndStripsItFromTheCommand() {
    var gate = WakePhrase(enabled: true)
    #expect(gate.accept("Please say Hey Hermes") == nil)
    #expect(gate.accept("Hey Hermesian") == nil)
    #expect(gate.accept("HEY, Hermes! Open the report") == "Open the report")
    #expect(gate.accept("unrelated room speech") == nil)
}

@Test func wakePhraseOnlyOpensOneShortWindowAndResetsOnLifecycleChanges() {
    let now = Date(timeIntervalSince1970: 100)
    var gate = WakePhrase(enabled: true)
    #expect(gate.accept("Hey Hermes", now: now) == nil)
    #expect(gate.accept("next command", now: now) == "next command")
    #expect(gate.accept("another command", now: now) == nil)
    #expect(gate.accept("Hey Hermes", now: now) == nil)
    #expect(gate.accept("too late", now: now.addingTimeInterval(16)) == nil)
    #expect(gate.accept("Hey Hermes", now: now) == nil)
    gate.reset()
    #expect(gate.accept("after backgrounding", now: now) == nil)
    var off = WakePhrase(enabled: false)
    #expect(off.accept("any text") == "any text")
}
