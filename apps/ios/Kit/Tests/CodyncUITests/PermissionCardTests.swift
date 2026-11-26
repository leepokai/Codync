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
