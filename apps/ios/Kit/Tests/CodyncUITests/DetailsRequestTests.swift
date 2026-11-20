import Foundation
import Testing
@testable import CodyncKit
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

/// The view of all bots' work asks a bot's conversation to open its details; only that conversation takes it, once.
@MainActor @Test func aDetailsRequestIsTakenOnceByItsOwnConversation() {
    let suite = "DetailsRequestTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "test"),
                         storage: SharedStore.Context(accountID: UUID().uuidString, suite: suite)) { _ in FakeRemote(.ready(.direct)) }
    defer { store.retire() }
    store.requestDetails(of: "b1")
    #expect(!store.takeDetailsRequest(for: "b2"))
    #expect(store.takeDetailsRequest(for: "b1"))
    #expect(!store.takeDetailsRequest(for: "b1"))
    // The same for the card a task waits on.
    store.requestCard(of: "b1", task: WorkTask(id: "i1", title: "Fix login", status: .needsYou))
    #expect(store.takeCardRequest(for: "b2") == nil)
    #expect(store.takeCardRequest(for: "b1")?.id == "i1")
    #expect(store.takeCardRequest(for: "b1") == nil)
}
