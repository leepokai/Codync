import Foundation
import Testing
@testable import CodyncKit

struct AppEnvironmentTests {
    @Test func developmentWidgetsAndDataStaySeparateFromProduction() {
        #expect(AppEnvironment.main.appGroup == "group.com.pokai.Codync")
        #expect(AppEnvironment.main.link("computers").absoluteString == "codync://computers")
        #expect(AppEnvironment.dev.appGroup != AppEnvironment.main.appGroup)
        #expect(AppEnvironment.dev.link("computers").absoluteString == "codync-dev://computers")
        #expect(AppEnvironment.dev.link("usage").scheme != AppEnvironment.main.link("usage").scheme)
    }
}
