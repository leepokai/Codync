import CodyncKit
import SwiftUI

/// An agent's other logins, beside the one the computer already has: add one (its sign-in follows), sign in one
/// that isn't yet, or remove one. Bots pick theirs in their own settings.
struct AgentAccounts: View {
    let backend: Backend
    /// Opens the sign-in terminal for an account.
    let signIn: (String) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The account being removed, or `adding`.
    @State private var busy: String?
    @State private var error: String?

    private static let adding = "+"

    var body: some View {
        CardSection("Other accounts", footer: "A bot picks its account under Agent in its settings. Bots that pick none use the sign-in above.") {
            ForEach(backend.accounts ?? []) { account in
                HStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(account.name ?? L10n.tr("New account")).font(.body.weight(.medium)).foregroundStyle(Palette.text).lineLimit(1)
                        Text(L10n.tr(account.signedIn ? "Signed in" : "Not signed in")).font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                    Spacer(minLength: 8)
                    if !account.signedIn {
                        StepAction(label: "Sign in") { signIn(account.id) }
                    }
                    if busy == account.id {
                        Spinner()
                    } else {
                        IconButton("Remove account", systemImage: "trash") { run(account.id) { try await model.removeAgentAccount(account.id) } }
                    }
                }
            }
            SetupOptionRow(
                title: "Add account",
                detail: L10n.fill(L10n.tr("Signs in another {name} account in a terminal on {other}."), ["name": backend.name, "other": model.hostName]),
                icon: "plus", busy: busy == Self.adding
            ) {
                run(Self.adding) { signIn(try await model.addAgentAccount(backend.id).id) }
            }
            if let error {
                Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled).transition(.opacity)
            }
        }
        .disabled(busy != nil)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: backend.accounts)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: error)
    }

    private func run(_ key: String, _ change: @escaping () async throws -> Void) {
        busy = key
        Task {
            defer { busy = nil }
            do {
                try await change()
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
