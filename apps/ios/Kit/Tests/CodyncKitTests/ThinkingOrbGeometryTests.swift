import Foundation
import Testing
@testable import CodyncKit

/// The fixture is written by the desktop's thinking-orb test and read by Android's too, so the
/// three clients draw the same dots.
@Test func thinkingOrbMatchesSharedFrames() throws {
    struct Case: Decodable { let state: String; let size, t: Double; let dots: [[Double]] }
    struct Fixture: Decodable { let tolerance: Double; let rest: [String: Double]; let cases: [Case] }
    let url = try #require(Bundle.module.url(forResource: "thinking-orb-frames", withExtension: "json", subdirectory: "Fixtures"))
    let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    for state in ThinkingOrb.State.allCases {
        #expect(ThinkingOrbGeometry.rest(state) == fixture.rest[state.rawValue], "\(state) still pose")
    }
    for vector in fixture.cases {
        let state = try #require(ThinkingOrb.State(rawValue: vector.state))
        let dots = ThinkingOrbGeometry.frame(state: state, size: vector.size, time: vector.t)
        let key = "\(vector.state) \(vector.size) \(vector.t)"
        try #require(dots.count == vector.dots.count, "\(key) dots")
        for (index, (dot, expected)) in zip(dots, vector.dots).enumerated() {
            for (actual, value) in zip([dot.x, dot.y, dot.radius, dot.alpha], expected) {
                #expect(abs(actual - value) <= fixture.tolerance, "\(key), dot \(index): \(actual) != \(value)")
            }
        }
    }
}

@Test func thinkingOrbDotsStayPutWhileTheirSizeChanges() {
    for state in ThinkingOrb.State.allCases {
        let start = ThinkingOrbGeometry.frame(state: state, size: 16, time: 0)
        let later = ThinkingOrbGeometry.frame(state: state, size: 16, time: 1.5)
        #expect(start.map { [$0.x, $0.y] } == later.map { [$0.x, $0.y] })
        #expect(start.map(\.radius) != later.map(\.radius), "\(state)")
    }
    #expect([13, 20, 32].map { ThinkingOrbGeometry.frame(state: .working, size: $0, time: 0).count } == [21, 37, 69])
}

@Test func thinkingOrbStatesRestOnDistinctPoses() {
    let poses = ThinkingOrb.State.allCases.map { state in
        ThinkingOrbGeometry.frame(state: state, size: 16, time: ThinkingOrbGeometry.rest(state)).map { ($0.radius * 1e4).rounded() }
    }
    #expect(Set(poses).count == ThinkingOrb.State.allCases.count)
}
