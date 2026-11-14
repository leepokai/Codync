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
        let weekly = UsageWindow(id: "w", label: "Weekly", percent: 10)
        let read = UsageProvider(id: "claude", name: "Claude", windows: [weekly], source: "claude", updatedAt: 1)
        // Listed only for a login profile that hasn't been read.
        let unread = UsageLoginProfile(id: "work", name: "Work", windows: [], updatedAt: 0)
        let listed = UsageProvider(id: "codex", name: "Codex", windows: [], source: "codex", updatedAt: 0, loginProfiles: [unread])
        let usage = Usage(providers: [read, listed])
        #expect(!usage.hasNoReading(UsageService(id: "claude", name: "Claude", shown: true)))
        #expect(usage.hasNoReading(UsageService(id: "codex", name: "Codex", shown: true)))
        #expect(usage.hasNoReading(UsageService(id: "cursor", name: "Cursor", shown: true)))
        #expect(!usage.hasNoReading(UsageService(id: "cursor", name: "Cursor", shown: false)))
    }

    @Test func anAgentWithOneLoginDecodesAsBefore() throws {
        let json = #"{"id":"claude","name":"Claude","windows":[{"id":"w","label":"Weekly","percent":10}],"source":"claude","updatedAt":1}"#
        let provider = try JSONDecoder().decode(UsageProvider.self, from: Data(json.utf8))
        #expect(provider.defaultLoginProfile == nil)
        #expect(provider.loginProfiles.isEmpty)
        #expect(provider.logins.count == 1)
        #expect(provider.tightestLogin.id == nil)
        #expect(provider.tightest?.percent == 10)
        #expect(try JSONDecoder().decode(UsageProvider.self, from: JSONEncoder().encode(provider)) == provider)
    }

    @Test func theTightestLoginIsTheOneAllCompactViewsShow() throws {
        let json = #"""
        {"id":"claude","name":"Claude","windows":[{"id":"w","label":"Weekly","percent":10}],"source":"claude","updatedAt":1,
         "defaultLoginProfile":"me@example.com",
         "loginProfiles":[{"id":"work","name":"Work","windows":[{"id":"w","label":"Weekly","percent":90}],"updatedAt":0},
                          {"id":"new","name":"New","windows":[],"updatedAt":0}]}
        """#
        let provider = try JSONDecoder().decode(UsageProvider.self, from: Data(json.utf8))
        #expect(provider.logins.map(\.id) == [nil, "work", "new"])
        #expect(provider.logins.map(\.name) == ["me@example.com", "Work", "New"])
        #expect(provider.tightestLogin.id == "work")
        #expect(provider.limits.map(\.percent) == [90])
        #expect(provider.tightest?.percent == 90)
    }

    @Test func aNoteNeverMakesALoginTheTightest() {
        let window = { (percent: Double, text: String?) in UsageWindow(id: "w", label: "Weekly", percent: percent, text: text) }
        let note = UsageLoginProfile(id: "key", name: "Key", windows: [window(100, "$4.20")], updatedAt: 0)
        var provider = UsageProvider(id: "claude", name: "Claude", windows: [window(10, nil)], source: "claude", updatedAt: 1, loginProfiles: [note])
        #expect(provider.tightestLogin.id == nil)
        #expect(provider.tightest?.percent == 10)
        provider.windows = []
        #expect(provider.tightestLogin.id == nil)
        #expect(provider.tightest == nil)
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
