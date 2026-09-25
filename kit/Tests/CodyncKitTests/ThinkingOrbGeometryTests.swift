import Foundation
import Testing
@testable import CodyncKit

/// Independent upstream vectors catch porting errors in projection, density,
/// preset scaling, sorting and deterministic noise at both tuned sizes.
@Test func thinkingOrbsMatchUpstreamGoldenVectors() throws {
    struct Sample: Decodable { let index: Int; let values: [Double] }
    struct Case: Decodable {
        let key, state: String
        let size, t: Double
        let dotCount, lineCount: Int
        let dots, lines: [Sample]
    }
    struct Fixture: Decodable { let tolerance: Double; let cases: [Case] }
    let url = try #require(Bundle.module.url(forResource: "thinking-orbs-golden", withExtension: "json", subdirectory: "Fixtures"))
    let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    for vector in fixture.cases {
        let state = try #require(ThinkingOrb.State(rawValue: vector.state))
        let frame = ThinkingOrbGeometry.frame(state: state, size: vector.size, time: vector.t)
        #expect(frame.dots.count == vector.dotCount, "\(vector.key) dots")
        #expect(frame.lines.count == vector.lineCount, "\(vector.key) lines")
        for sample in vector.dots {
            guard sample.index < frame.dots.count else { continue }
            let d = frame.dots[sample.index]
            for (actual, expected) in zip([d.x, d.y, d.z, d.radius, d.white, d.alpha], sample.values) {
                #expect(abs(actual - expected) <= fixture.tolerance, "\(vector.key), dot \(sample.index): \(actual) != \(expected)")
            }
        }
        for sample in vector.lines {
            guard sample.index < frame.lines.count else { continue }
            let l = frame.lines[sample.index]
            for (actual, expected) in zip([l.x1, l.y1, l.x2, l.y2, l.white, l.alpha, l.width], sample.values) {
                #expect(abs(actual - expected) <= fixture.tolerance, "\(vector.key), line \(sample.index)")
            }
        }
    }
}
