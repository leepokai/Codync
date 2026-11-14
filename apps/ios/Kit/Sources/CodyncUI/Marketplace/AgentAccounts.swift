import CodyncKit
import SwiftUI

/// An agent's accounts in one place: the default one (the login the computer already has, with its sign-in options
/// as `options`) and the others. Each can be named; others are added (their sign-in follows), signed in or removed.
/// Bots pick theirs in their own settings.
struct AgentAccounts<Options: View>: View {
    let backend: Backend
    /// The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again.
    let status: String
    let checking: Bool
    let check: () -> Void
    /// Opens the sign-in terminal for an account.
    let signIn: (String) -> Void
    /// An account whose sign-in was asked for (a chat's sign-in card), started here once (`signInStarted`) when its
    /// agent signs itself in.
    var signInNow: String?
    var signInStarted: () -> Void = {}
    @ViewBuilder let options: Options
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The account being changed, `own` or `adding`.
    @State private var busy: String?
    @State private var error: String?
    @State private var renaming: Renaming?
    @State private var newName = ""

    private let own = ""
    private let adding = "+"
    private let nameLimit = 60

    /// The account being named (`account` nil: the default one) and the name the user gave it so far.
    private struct Renaming {
        var account: String?
        var label: String?
    }

    var body: some View {
        CardSection("Accounts", footer: "A bot picks its account under Agent in its settings. Bots that pick none use the default account.") {
            let mine = backend.ownAccount
            AccountLine(
                name: mine?.name ?? L10n.tr("Default account"),
                detail: [L10n.tr("Default"), mine?.label == nil ? nil : mine?.email, L10n.tr(status)],
                rename: { rename(nil, label: mine?.label) }
            ) {
                CheckAgain(checking: checking, check: check)
            }
            options
            ForEach(backend.accounts ?? []) { account in
                AccountLine(
                    name: account.name ?? L10n.tr("New account"),
                    detail: [account.label == nil ? nil : account.email, signInState(account)],
                    rename: { rename(account.id, label: account.label) }
                ) {
                    if !account.signedIn {
                        StepAction(label: "Sign in") { run(account.id) { try await start(account.id) } }
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
                detail: L10n.fill(
                    L10n.tr(backend.accountsSignInInBrowser
                        ? "Signs in another {name} account in a browser on {other}."
                        : "Signs in another {name} account in a terminal on {other}."),
                    ["name": backend.name, "other": model.hostName]),
                icon: "plus", busy: busy == adding
            ) {
                run(adding) { try await start(try await model.addAgentAccount(backend.id).id) }
            }
            if let error {
                Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled).transition(.opacity)
            }
        }
        .disabled(busy != nil)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: backend.accounts)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: error)
        .codyncDialog("Rename account", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } }),
                      field: DialogField("Account name", text: $newName)) {
            // Read as the dialog shows: closing clears `renaming`.
            let target = renaming
            return [DialogAction("Save") {
                // An empty name goes back to who it signs in.
                guard let target, newName.trimmingCharacters(in: .whitespacesAndNewlines) != target.label ?? "" else { return }
                run(target.account ?? own) { try await model.renameAgentAccount(backend.id, account: target.account, name: newName) }
            }]
        }
        .onChange(of: newName) {
            if newName.count > nameLimit { newName = String(newName.prefix(nameLimit)) }
        }
        .task {
            // Once: the section is built again each time the sheet comes back to its overview.
            guard let signInNow, backend.accountsSignInInBrowser else { return }
            signInStarted()
            if backend.accounts?.first(where: { $0.id == signInNow })?.signedIn == false {
                run(signInNow) { try await start(signInNow) }
            }
        }
    }

    private func signInState(_ account: AgentAccount) -> String {
        if busy == account.id, !account.signedIn, backend.accountsSignInInBrowser {
            return L10n.fill(L10n.tr("Finish signing in in the browser on {name}…"), ["name": model.hostName])
        }
        return L10n.tr(account.signedIn ? "Signed in" : "Not signed in")
    }

    private func rename(_ account: String?, label: String?) {
        newName = label ?? ""
        renaming = Renaming(account: account, label: label)
    }

    /// Its agent signs the account in itself (a browser on the computer; the account is `busy` until that ends), or
    /// its CLI does in a terminal.
    private func start(_ account: String) async throws {
        guard backend.accountsSignInInBrowser else { return signIn(account) }
        busy = account
        try await model.signInAgentAccount(backend.id, account: account)
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

/// One account: its name, what's known of it, the pencil that names it, and its actions.
private struct AccountLine<Actions: View>: View {
    let name: String
    let detail: [String?]
    let rename: () -> Void
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(name).font(.body.weight(.medium)).foregroundStyle(Palette.text).lineLimit(1)
                Text(detail.compactMap { $0 }.joined(separator: " · ")).font(.subheadline).foregroundStyle(Palette.secondary).textSelection(.enabled)
            }
            Spacer(minLength: 8)
            IconButton("Rename account", systemImage: "pencil", action: rename)
            actions
        }
    }
}
