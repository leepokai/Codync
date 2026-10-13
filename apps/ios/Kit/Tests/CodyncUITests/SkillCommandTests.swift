import Testing
@testable import CodyncKit
@testable import CodyncUI

@Test func aLeadingSlashOffersTheBotsSkills() {
    #expect(SkillCommand.query("/") == "")
    #expect(SkillCommand.query("/pd") == "pd")
    #expect(SkillCommand.query("/pdf merge") == nil)
    #expect(SkillCommand.query("hi /pdf") == nil)
    let installed = [("pdf", "PDF"), ("code-review", "Code Review"), ("xlsx", "xlsx")].map {
        InstalledSkill(id: $0.0, name: $0.1, description: "", source: "custom", path: "")
    }
    #expect(SkillCommand.matches(installed, enabled: ["pdf", "code-review"], query: "").map(\.id) == ["pdf", "code-review"])
    #expect(SkillCommand.matches(installed, enabled: ["pdf", "code-review"], query: "review").map(\.id) == ["code-review"])
    #expect(SkillCommand.matches(installed, enabled: ["pdf"], query: "xl").isEmpty)
}
