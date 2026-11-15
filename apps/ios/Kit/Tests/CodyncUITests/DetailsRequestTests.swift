import Testing
@testable import CodyncUI

/// Where the chat's links into a bot's details land.
@MainActor @Suite struct DetailsRequestTests {
    @Test func theDetailsOpenOnWhatTheBotDoes() {
        #expect(DetailsRequest().tab == .details)
    }

    @Test func addAFallbackOpensSettingsWhereTheAgentCardIs() {
        let request = DetailsRequest.fallbacks(of: "b2")
        #expect(request.tab == .settings)
        #expect(request.botId == "b2")
    }
}
