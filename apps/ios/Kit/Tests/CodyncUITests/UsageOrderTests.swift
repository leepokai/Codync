import Foundation
import Testing
@testable import CodyncUI

@Suite struct UsageOrderTests {
    @Test func aDroppedRowLandsWhereItWasLetGo() {
        let ids = ["claude", "codex", "cursor"]
        #expect(UsageOrder.moved(ids, from: [0], to: 2) == ["codex", "claude", "cursor"])
        #expect(UsageOrder.moved(ids, from: [2], to: 0) == ["cursor", "claude", "codex"])
    }

    @Test func aRowDroppedInItsOwnPlaceSendsNothing() {
        #expect(UsageOrder.moved(["claude", "codex"], from: [0], to: 0) == nil)
        #expect(UsageOrder.moved(["claude", "codex"], from: [0], to: 1) == nil)
    }
}
