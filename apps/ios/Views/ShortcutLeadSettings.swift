import CodyncKit
import CodyncUI
import SwiftUI

/// Which lead Siri, the Action button and Open Live Mode without a bot call (`ShortcutBotQuery.lead`), shown once
/// more than one computer has a lead. Automatic: the lead of the first computer in the list that has one.
struct ShortcutLeadSettings: View {
    @Environment(AppStore.self) private var app
    /// A computer id, or empty for Automatic.
    @State private var chosen = SharedStore.activeContext.shortcutLeadComputer ?? ""

    var body: some View {
        let leads = app.accounts.roster.filter(\.bot.lead)
        if leads.count > 1 {
            CardSection("Shortcuts", footer: "Siri, the Action button and Open Live Mode without a bot call this lead. Automatic calls the lead of the first computer in your list.") {
                ValueRow("Lead to call") {
                    ChoicePicker(
                        // A computer whose lead is gone shows as Automatic, which is what the shortcut does then.
                        selection: Binding(get: { leads.contains { $0.ref.computerId == chosen } ? chosen : "" }, set: { chosen = $0 }),
                        options: [(id: "", label: "Automatic")] + leads.map { (id: $0.ref.computerId, label: "\($0.bot.name) · \($0.computer.name)") }
                    )
                }
            }
            .onChange(of: chosen) { _, id in SharedStore.activeContext.shortcutLeadComputer = id.isEmpty ? nil : id }
        }
    }
}
