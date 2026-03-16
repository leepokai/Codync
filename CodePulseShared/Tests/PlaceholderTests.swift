import XCTest
@testable import CodePulseShared

final class PlaceholderTests: XCTestCase {
    func testVersion() {
        XCTAssertEqual(CodePulseSharedVersion.current, "0.1.0")
    }
}
