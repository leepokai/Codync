import CodyncKit
import SwiftUI

/// Which of the agent's logins a bot runs on: the one the computer already has, or one of its other accounts.
/// "Add account…" opens the agent's setup, where a new one signs in.
struct AccountRow: View {
    let backend: Backend
    @Binding var account: String?
    @State private var adding = false

    private static let own = ""
    private static let add = "+"

    private var options: [(id: String, label: String)] {
        [(Self.own, "This computer's sign-in")]
            + backend.accountChoices(picked: account).map { ($0.id, $0.name ?? "New account") }
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
