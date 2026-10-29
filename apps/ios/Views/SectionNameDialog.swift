import CodyncKit
import CodyncUI
import SwiftUI

/// An open section name dialog (desktop and Android `SectionNaming`): its title, the name it opens
/// with, and what Save does with the typed one.
struct SectionNaming {
    let title: String
    let name: String
    let save: @MainActor (String) -> Void
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
            .onChange(of: naming?.name) { if let start = naming?.name { name = start } }
            // Typing stops where the computer's limit is.
            .onChange(of: name) {
                let limited = RosterSection.limited(name)
                if limited != name { name = limited }
            }
    }
}
