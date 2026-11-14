import CodyncKit
import SwiftUI

/// Which of the agent's accounts a bot runs on: the default one (the login the computer already has), or one of its
/// other accounts. "Add account…" opens the agent's setup, where a new one signs in.
struct AccountRow: View {
    let backend: Backend
    @Binding var account: String?
    /// The accounts that can be picked (nil: the default one), when not all can: a fallback's, whose chain has the others.
    var free: [String?]?
    @State private var adding = false

    private static let own = ""
    private static let add = "+"

    private var options: [(id: String, label: String)] {
        let open = { (id: String?) in free?.contains(id) ?? true }
        return (open(nil) ? [(Self.own, backend.ownAccount?.name ?? "Default account")] : [])
            + backend.accountChoices(picked: account).filter { open($0.id) }.map { ($0.id, $0.name ?? "New account") }
            + [(Self.add, "Add account…")]
    }

    var body: some View {
        ValueRow("Account") {
            ChoicePicker(selection: Binding(get: { account ?? Self.own }, set: { id in
                if id == Self.add { adding = true } else { account = id.isEmpty ? nil : id }
            }), options: options, fitsAvailableWidth: true)
        }
        .codyncSheet(isPresented: $adding) { AgentSheet(initial: backend) }
    }
}
