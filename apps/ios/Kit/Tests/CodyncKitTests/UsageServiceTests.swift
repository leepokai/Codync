import Foundation
import Testing
@testable import CodyncKit

@Suite struct UsageServiceTests {
    @Test func theChooserListDecodes() throws {
        let json = #"{"providers":[],"services":[{"id":"cursor","name":"Cursor","shown":false}]}"#
        let usage = try JSONDecoder().decode(Usage.self, from: Data(json.utf8))
        #expect(usage.services == [UsageService(id: "cursor", name: "Cursor", shown: false)])
        #expect(usage.services?.first?.provider.windows == [])
    }

    @Test func aHostOlderThanTheChooserSendsNoList() throws {
        let usage = try JSONDecoder().decode(Usage.self, from: Data(#"{"providers":[]}"#.utf8))
        #expect(usage.services == nil)
    }

    @Test func aMoveOnItsWayHoldsTheOrderAndAgentsItDoesNotNameFollow() {
        let service = { (id: String) in UsageService(id: id, name: id, shown: true) }
        let usage = Usage(services: ["claude", "codex", "cursor"].map(service))
        #expect(usage.services(in: nil).map(\.id) == ["claude", "codex", "cursor"])
        #expect(usage.services(in: ["cursor", "gone", "claude"]).map(\.id) == ["cursor", "claude", "codex"])
        #expect(Usage().services(in: ["claude"]).isEmpty)
    }

    @Test func onlyAnAgentThatIsOnCanLackAReading() {
        let read = UsageProvider(id: "claude", name: "Claude", windows: [], source: "claude", updatedAt: 1)
        let usage = Usage(providers: [read])
        #expect(!usage.hasNoReading(UsageService(id: "claude", name: "Claude", shown: true)))
        #expect(usage.hasNoReading(UsageService(id: "cursor", name: "Cursor", shown: true)))
        #expect(!usage.hasNoReading(UsageService(id: "cursor", name: "Cursor", shown: false)))
    }

    @Test func everyReadersLogoLoadsFromTheKit() {
        let named = ["claude-acp", "codex-acp", "antigravity-acp", "cursor", "github-copilot-cli", "grok-build", "amp-acp", "auggie", "junie", "kilo", "kimi", "mistral-vibe"]
        for registry in named {
            let service = UsageService(id: "agent", name: "Agent", shown: true, registry: registry)
            #expect(service.provider.logoAsset == "agent-\(registry)")
        }
        for id in ["kiro", "omp", "opencode"] {
            #expect(UsageService(id: id, name: id, shown: true).provider.logoAsset == "agent-\(id)")
        }
    }

    @Test func aHostThatNamesNoRegistryIdStillFindsClaudesLogo() {
        #expect(UsageService(id: "claude", name: "Claude", shown: true).provider.logoAsset == "agent-claude-acp")
        #expect(UsageService(id: "codex", name: "Codex", shown: true).provider.logoAsset == "agent-codex-acp")
    }

    @Test func anAgentWithNoLogoHasNoAsset() {
        #expect(UsageService(id: "hermes", name: "Hermes Agent", shown: true).provider.logoAsset == nil)
    }
}
