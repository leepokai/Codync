import XCTest

/// The real app against a real host, which logs what the phone sends: the app's own cold open's first
/// milestones and a swipe of its bot list land in the host's `phone-performance.jsonl`. Driven by
/// `host/tests/phone_performance_e2e.rs`, which runs only when asked:
/// `TEST_RUNNER_CODYNC_PERFORMANCE_DIR=<dir>` holds the host's pairing link.
final class PhonePerformanceDeliveryUITests: XCTestCase {
    func testAnOpenAndAScrollReachTheHostsPerformanceLog() throws {
        guard let dir = ProcessInfo.processInfo.environment["CODYNC_PERFORMANCE_DIR"].map(URL.init(fileURLWithPath:)) else {
            throw XCTSkip("needs a host's pairing link (CODYNC_PERFORMANCE_DIR)")
        }
        let pairing = try String(contentsOf: dir.appendingPathComponent("pairing"), encoding: .utf8)
        let app = XCUIApplication()
        // Signed out and new: the welcome would ask for an account, and this host pairs the build itself.
        app.launchArguments += ["-withoutAccount", "YES"]
        app.launchEnvironment["CODYNC_PAIR_URL"] = pairing
        // Simulators skip delivery unless a test asks; the host here is this test's own.
        app.launchEnvironment["CODYNC_RECORD_PERFORMANCE"] = "1"
        app.launch()
        let list = app.collectionViews["botList"]
        XCTAssertTrue(list.waitForExistence(timeout: 120), "the bot list never appeared")
        let firstBot = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Performance bot")).firstMatch
        XCTAssertTrue(firstBot.waitForExistence(timeout: 120), "the host's bots never arrived")
        list.swipeUp(velocity: .slow)
        list.swipeDown(velocity: .slow)
        // Delivery is fire-and-forget and a scroll reports after its tail: keep the app alive for it.
        Thread.sleep(forTimeInterval: 3)
    }
}
