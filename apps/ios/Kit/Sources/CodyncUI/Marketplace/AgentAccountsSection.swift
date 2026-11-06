import CodyncKit
import SwiftUI

/// The accounts an agent keeps on this computer: the computer's own sign-in and the isolated
/// profiles bots can pick. Account sign-in can take a few minutes; the host call waits for it.
struct AgentAccountsSection: View {
    let backend: Backend
    var onChanged: () -> Void = {}
    @Environment(BotStore.self) private var model
    @State private var accounts: [AgentAccount] = []
    @State private var loading = true
    @State private var adding = false
    @State private var removing: String?
    @State private var disconnecting = false
    @State private var failure: String?
    @State private var confirmRemove: AgentAccount?
    @State private var confirmSignOut = false

    private var host: String { model.hostName }
    private var busy: Bool { loading || adding || removing != nil || disconnecting }

    private var defaultDetail: String {
        switch backend.signedIn {
        case true?: backend.email.map { "Signed in as \($0) on \(host)." } ?? "Signed in on \(host)."
        case false?: "Not signed in on \(host)."
        case nil: "Couldn't tell whether \(host) is signed in."
        }
    }

    var body: some View {
        CardSection("Accounts", footer: "Each account keeps its own sign-in on \(host). Bots can use the default account or any isolated one.") {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr("Default account")).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                    Text(defaultDetail).font(.subheadline).foregroundStyle(Palette.secondary).textSelection(.enabled)
                }
                Spacer(minLength: 8)
                if backend.signOut == true {
                    Button(disconnecting ? L10n.tr("Signing out…") : L10n.tr("Disconnect")) { confirmSignOut = true }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(busy)
                }
            }
            .padding(.vertical, 2)
            if loading {
                HStack(spacing: 8) {
                    Spinner(size: 12)
                    Text(L10n.tr("Loading accounts…")).font(.subheadline).foregroundStyle(Palette.secondary)
                }
                .padding(.vertical, 2)
            } else {
                ForEach(accounts) { account in
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(account.name).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                            Text("Isolated account on \(host).").font(.subheadline).foregroundStyle(Palette.secondary)
                        }
                        Spacer(minLength: 8)
                        IconButton("Remove \(account.name)", systemImage: "trash") { confirmRemove = account }
                            .disabled(busy)
                    }
                    .padding(.vertical, 2)
                }
            }
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr("Add account")).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                    Text(adding ? "Finish signing in in the browser on \(host)…"
                         : L10n.tr("Sign in to another account without signing out of this one."))
                        .font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
                }
                Spacer(minLength: 8)
                Button(adding ? L10n.tr("Signing in…") : "+ Add account") { add() }
                    .buttonStyle(SecondaryButtonStyle())
                    .disabled(busy)
            }
            .padding(.vertical, 2)
            if let failure {
                VStack(alignment: .leading, spacing: 6) {
                    Text(L10n.tr(failure)).font(.caption).foregroundStyle(Palette.warning)
                    Button(L10n.tr("Retry loading accounts")) { Task { await load() } }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(busy)
                }
                .padding(.vertical, 2)
            }
        }
        .task(id: backend.id) { await load() }
        .codyncDialog("Remove account?", isPresented: Binding(
            get: { confirmRemove != nil },
            set: { if !$0 { confirmRemove = nil } }
        ), message: confirmRemove.map {
            "\($0.name) will be signed out and its saved sign-in removed from \(host). Bots using it will need another account."
        }, actions: {
            guard let account = confirmRemove else { return [] }
            return [DialogAction("Remove", destructive: true) { remove(account) }]
        })
        .codyncDialog("Sign out of \(backend.name)?", isPresented: $confirmSignOut,
                      message: "This removes the computer's own sign-in on \(host), as if you no longer have this account. Isolated accounts are not affected.",
                      actions: {
                          [DialogAction("Sign out", destructive: true) { signOut() }]
                      })
    }

    @MainActor private func load() async {
        loading = true
        failure = nil
        do {
            accounts = try await model.loadAgentAccounts(backend: backend.id)
        } catch {
            failure = "Couldn't load accounts: \(error.localizedDescription)"
        }
        loading = false
    }

    @MainActor private func add() {
        guard !adding else { return }
        adding = true
        failure = nil
        Task {
            do {
                let account = try await model.authenticateAgentAccount(backend: backend.id)
                accounts.removeAll { $0.id == account.id }
                accounts.append(account)
                onChanged()
            } catch {
                failure = "Couldn't add the account: \(error.localizedDescription)"
            }
            adding = false
        }
    }

    @MainActor private func remove(_ account: AgentAccount) {
        removing = account.id
        failure = nil
        Task {
            do {
                try await model.deleteAgentAccount(id: account.id)
                accounts.removeAll { $0.id == account.id }
                onChanged()
            } catch {
                failure = "Couldn't remove the account: \(error.localizedDescription)"
            }
            removing = nil
        }
    }

    @MainActor private func signOut() {
        disconnecting = true
        failure = nil
        Task {
            do {
                try await model.signOutAgent(backend: backend.id)
                onChanged()
            } catch {
                failure = "Couldn't sign out: \(error.localizedDescription)"
            }
            disconnecting = false
        }
    }
}
