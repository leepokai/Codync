import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Suite struct PermissionCardTests {
    @Test func duplicateAllowAlwaysOptionsKeepDistinctAgentNames() {
        let options = [
            PermissionOption(optionId: "once", name: "Yes", kind: "allow_once"),
            PermissionOption(optionId: "session", name: "Always allow for this session", kind: "allow_always"),
            PermissionOption(optionId: "forever", name: "Always allow", kind: "allow_always"),
            PermissionOption(optionId: "deny", name: "No", kind: "reject_once"),
        ]
        #expect(PermissionCard.optionLabel(options[0], in: options) == L10n.tr("Allow once"))
        #expect(PermissionCard.optionLabel(options[1], in: options) == "Always allow for this session")
        #expect(PermissionCard.optionLabel(options[2], in: options) == "Always allow")
        #expect(PermissionCard.optionLabel(options[3], in: options) == L10n.tr("Deny"))

        var data = EntryData()
        data.status = "answered"
        data.selected = "session"
        data.options = options
        #expect(PermissionCard.outcome(data) == "Always allow for this session")

        var foreverData = EntryData()
        foreverData.status = "answered"
        foreverData.selected = "forever"
        foreverData.options = options
        let sessionEntry = Entry(id: "p1", seq: 1, botId: "b1", rev: 1, kind: "permission", turn: 1, data: data, createdAt: 0, updatedAt: 0)
        let foreverEntry = Entry(id: "p2", seq: 2, botId: "b1", rev: 1, kind: "permission", turn: 1, data: foreverData, createdAt: 0, updatedAt: 0)
        #expect(Request.ending(sessionEntry) != Request.ending(foreverEntry))
    }

    @Test func singleAllowAlwaysKeepsLocalizedLabel() {
        let options = [
            PermissionOption(optionId: "once", name: "Yes", kind: "allow_once"),
            PermissionOption(optionId: "always", name: "Yes, always", kind: "allow_always"),
        ]
        #expect(PermissionCard.optionLabel(options[1], in: options) == L10n.tr("Always allow"))

        var data = EntryData()
        data.status = "answered"
        data.selected = "always"
        data.options = options
        #expect(PermissionCard.outcome(data) == L10n.tr("Always allowed"))
    }
}
