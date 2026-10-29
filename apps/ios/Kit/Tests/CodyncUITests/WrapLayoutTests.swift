import Testing
@testable import CodyncUI

@Test func wrapLayoutSharesItemsEvenlyBetweenRows() {
    // 8 shapes of 48 with 6 apart: 6 fit in 362, so two rows of 4 (not 6 and 2).
    #expect(WrapLayout.split(Array(repeating: 48, count: 8), spacing: 6, width: 362) == [0..<4, 4..<8])
    // 11 colors of 36: 8 fit, so rows of 6 and 5.
    #expect(WrapLayout.split(Array(repeating: 36, count: 11), spacing: 6, width: 362) == [0..<6, 6..<11])
    #expect(WrapLayout.split(Array(repeating: 48, count: 3), spacing: 6, width: 362) == [0..<3])
    #expect(WrapLayout.split([], spacing: 6, width: 362).isEmpty)
}
