import CodyncKit
import SwiftUI

/// Which of the agent's login profiles a bot runs on: the default one (the login the computer already has), or one
/// of its others. "Add login profile…" opens the agent's setup, where a new one signs in or gets its keys.
struct LoginProfileRow: View {
    let backend: Backend
    @Binding var loginProfile: String?
    /// The login profiles that can be picked (nil: the default one), when not all can: a fallback's, whose chain has
    /// the others.
    var free: [String?]?
    @State private var adding = false

    private static let own = ""
    private static let add = "+"

    private var options: [(id: String, label: String)] {
        let open = { (id: String?) in free?.contains(id) ?? true }
        return (open(nil) ? [(Self.own, backend.defaultLoginProfile?.name ?? "Default login profile")] : [])
            + backend.loginProfileChoices(picked: loginProfile).filter { open($0.id) }.map { ($0.id, $0.name ?? "New login profile") }
            + [(Self.add, "Add login profile…")]
    }

    var body: some View {
        ValueRow("Login profile") {
            ChoicePicker(selection: Binding(get: { loginProfile ?? Self.own }, set: { id in
                if id == Self.add { adding = true } else { loginProfile = id.isEmpty ? nil : id }
            }), options: options, fitsAvailableWidth: true)
        }
        .codyncSheet(isPresented: $adding) { AgentSheet(initial: backend) }
    }
}
