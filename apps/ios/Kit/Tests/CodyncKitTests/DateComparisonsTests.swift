import CodyncKit
import Foundation
import Testing

// Reached by symbol name, as the SDK's back-deployment thunks reach them (#350), which checks the names and
// the calling convention. Without CodyncKit's definitions this fails to link only on an SDK that doesn't
// export the symbols: a 27.2 SDK binds it to Foundation's. The direct calls below fail on every SDK, and a
// definition that only loses its @_silgen_name is left to tools/check-ios-weak-imports.py.
private extension Date {
    @_silgen_name("$s10Foundation4DateV2leoiySbAC_ACtFZ")
    static func boundLessOrEqual(_ lhs: Date, _ rhs: Date) -> Bool

    @_silgen_name("$s10Foundation4DateV2geoiySbAC_ACtFZ")
    static func boundGreaterOrEqual(_ lhs: Date, _ rhs: Date) -> Bool
}

@Test func dateComparisonsAnswerUnderFoundationsSymbols() {
    let earlier = Date(timeIntervalSinceReferenceDate: 1)
    let later = Date(timeIntervalSinceReferenceDate: 2)
    #expect(Date.boundLessOrEqual(earlier, later))
    #expect(Date.boundLessOrEqual(earlier, earlier))
    #expect(!Date.boundLessOrEqual(later, earlier))
    #expect(Date.boundGreaterOrEqual(later, earlier))
    #expect(Date.boundGreaterOrEqual(later, later))
    #expect(!Date.boundGreaterOrEqual(earlier, later))
    #expect(Date._lessOrEqual(earlier, later))
    #expect(Date._greaterOrEqual(later, earlier))
}
