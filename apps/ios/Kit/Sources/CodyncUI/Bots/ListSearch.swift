import CodyncKit
import SwiftUI

/// A list's search field (the Bots, Threads and Workflows tabs): shown under the bar only once the header's Search opens it,
/// and gone again with Cancel.
public struct ListSearch: ViewModifier {
    @Binding var searching: Bool
    @Binding var query: String
    let prompt: String
    /// The field's focus, apart from the drawer being shown: focusing it as the drawer is inserted can lose
    /// to a live roster update, and the system's write-back of false then took the drawer away with it.
    @State private var active = false

    public init(searching: Binding<Bool>, query: Binding<String>, prompt: String) {
        _searching = searching
        _query = query
        self.prompt = prompt
    }

    public func body(content: Content) -> some View {
        Group {
            if searching {
                content
                    .searchable(text: $query, isPresented: $active, placement: .navigationBarDrawer(displayMode: .always), prompt: L10n.tr(prompt))
                    // Focused once the drawer is in the bar.
                    .task { active = true }
            } else {
                content
            }
        }
        // Close (or Cancel) ends the field's focus once it had it: that closes the drawer.
        .onChange(of: active) { _, isActive in if !isActive { searching = false } }
        // Outside the branch, so it still sees Search close however it closes.
        .onChange(of: searching) { if !searching { query = ""; active = false } }
    }
}
