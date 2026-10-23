import CodyncKit
import CodyncUI
import SwiftUI

/// What the section name dialog names: a bot's new section, or a section to rename with `save`.
enum SectionNaming {
    case new(Bot, BotStore)
    case rename(String, save: @MainActor (String) -> Void)

    var title: String {
        switch self {
        case .new: "New section"
        case .rename: "Rename section"
        }
    }

    /// The name the dialog opens with: none for a new section, the current one for a rename.
    var startName: String {
        if case let .rename(old, _) = self { old } else { "" }
    }

    @MainActor func save(_ name: String) {
        switch self {
        case let .new(bot, store): store.setSection(bot, to: name)
        case let .rename(_, save): save(name)
        }
    }
}

extension View {
    /// Asks for a section's name while `naming` is set (desktop and Android `SectionNameDialog`).
    func sectionNameDialog(_ naming: Binding<SectionNaming?>) -> some View {
        modifier(SectionNameDialog(naming: naming))
    }
}

private struct SectionNameDialog: ViewModifier {
    @Binding var naming: SectionNaming?
    @State private var name = ""

    func body(content: Content) -> some View {
        content
            .codyncDialog(naming?.title ?? "", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } }),
                          field: DialogField("Section name", text: $name)) {
                // Read as the dialog shows: closing clears `naming`.
                let target = naming
                return [DialogAction("Save") {
                    guard let target, let typed = RosterSection.name(typed: name) else { return }
                    target.save(typed)
                }]
            }
            // Each opening starts from its own name.
            .onChange(of: naming?.startName) { if let start = naming?.startName { name = start } }
            // Typing stops where the computer's limit is.
            .onChange(of: name) {
                let limited = RosterSection.limited(name)
                if limited != name { name = limited }
            }
    }
}
