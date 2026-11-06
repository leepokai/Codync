import CodyncKit
import SwiftUI

/// Selects host-owned agent accounts; this view only keeps its presentation state.
struct AgentAccountPicker: View {
    let backend: String
    /// The agent keeps isolated accounts beside the computer's own sign-in.
    var supported = true
    /// The computer's own sign-in can be removed.
    var canSignOut = false
    @Binding var selection: String?
    @Environment(BotStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var accounts: [AgentAccount] = []
    @State private var loading = false
    @State private var signingIn = false
    @State private var removing: String?
    @State private var signingOut = false
    @State private var failure: String?
    @State private var retry = 0
    @State private var signInTask: Task<Void, Never>?
    @State private var confirmRemove: AgentAccount?
    @State private var confirmSignOut = false

    private var scope: String {
        "\(ObjectIdentifier(store))/\(store.storage.id)/\(store.computer.id)/\(backend)"
    }

    private var requestKey: String { "\(scope)/\(store.connection == .online)/\(retry)" }

    private var choices: [(String, String)] {
        var result = [("", "Default (computer)")] + accounts.map { ($0.id, $0.name) }
        if let selection, !result.contains(where: { $0.0 == selection }) {
            result.append((selection, "Saved account (unavailable)"))
        }
        return result
    }

    private var selected: AgentAccount? {
        accounts.first { $0.id == selection }
    }

    private var value: Binding<String> {
        Binding(get: { selection ?? "" }, set: { newValue in
            withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
                selection = newValue.isEmpty ? nil : newValue
            }
        })
    }

    var body: some View {
        Group {
            if supported { picker }
        }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: 8) {
            ValueRow("Account", detail: selection == nil
                     ? "Uses this computer's default account."
                     : "Uses a dedicated account. Changing it starts a new agent session; chat history is kept.") {
                ChoicePicker(selection: value, options: choices, fitsAvailableWidth: true)
                    .disabled(signingIn || removing != nil || signingOut)
            }
            HStack(spacing: 8) {
                Button("Sign in with another account") { startSignIn() }
                    .buttonStyle(SecondaryButtonStyle())
                    .disabled(loading || signingIn || removing != nil || signingOut || store.connection != .online)
                IconButton("Refresh accounts", systemImage: "arrow.clockwise") { retry += 1 }
                    .disabled(loading || signingIn || removing != nil || signingOut)
                if let selected {
                    IconButton("Remove \(selected.name)", systemImage: "trash") { confirmRemove = selected }
                        .disabled(loading || signingIn || removing != nil || signingOut || store.connection != .online)
                }
                if loading || signingIn || removing != nil || signingOut {
                    Spinner(size: 12)
                        .accessibilityLabel(signingIn ? "Waiting for sign-in on the computer"
                                             : removing != nil ? "Removing the account"
                                             : signingOut ? "Signing out"
                                             : "Loading accounts")
                }
            }
            if signingIn {
                Text("Finish signing in in the paired computer's browser. Keep this screen open while it completes.")
                    .font(.caption).foregroundStyle(Palette.secondary)
            }
            if canSignOut {
                HStack(spacing: 8) {
                    Text("The computer's own sign-in is the default account.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                    Spacer(minLength: 8)
                    Button(signingOut ? "Signing out…" : "Sign out default") { confirmSignOut = true }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(loading || signingIn || removing != nil || signingOut || store.connection != .online)
                }
            }
            if let failure {
                Text(failure).font(.caption).foregroundStyle(Palette.warning)
                Button("Retry loading accounts") { retry += 1 }
                    .buttonStyle(SecondaryButtonStyle())
                    .disabled(loading || signingIn || removing != nil || signingOut)
            } else if !loading && !signingIn && accounts.isEmpty {
                Text("No dedicated accounts on this computer. Use Default or sign in with another account.")
                    .font(.caption).foregroundStyle(Palette.secondary)
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: loading)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: signingIn)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: failure)
        .task(id: requestKey) { await load() }
        .onChange(of: scope) { _, _ in
            signInTask?.cancel()
            signingIn = false
            removing = nil
            signingOut = false
            accounts = []
        }
        .onDisappear {
            signInTask?.cancel()
            signingIn = false
        }
        .codyncDialog("Remove account?", isPresented: Binding(
            get: { confirmRemove != nil },
            set: { if !$0 { confirmRemove = nil } }
        ), message: confirmRemove.map {
            "\($0.name) will be signed out and its saved sign-in removed from this computer. Bots using it will need another account."
        }, actions: {
            guard let account = confirmRemove else { return [] }
            return [DialogAction("Remove", destructive: true) { remove(account) }]
        })
        .codyncDialog("Sign out of the default account?", isPresented: $confirmSignOut,
                      message: "Bots that use the computer's own sign-in will need to sign in again. Isolated accounts are not affected.",
                      actions: {
                          [DialogAction("Sign out", destructive: true) { signOutDefault() }]
                      })
    }

    @MainActor private func load() async {
        let requestedScope = scope
        loading = true
        failure = nil
        do {
            let response = try await store.loadAgentAccounts(backend: backend)
            try Task.checkCancellation()
            guard requestedScope == scope else { return }
            accounts = response
        } catch {
            guard !Task.isCancelled, requestedScope == scope else { return }
            failure = "Couldn't load accounts: \(error.localizedDescription)"
        }
        guard requestedScope == scope else { return }
        loading = false
    }

    @MainActor private func startSignIn() {
        guard !signingIn else { return }
        let requestedScope = scope
        signingIn = true
        failure = nil
        signInTask = Task {
            do {
                let account = try await store.authenticateAgentAccount(backend: backend)
                try Task.checkCancellation()
                guard requestedScope == scope else { return }
                withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
                    accounts.removeAll { $0.id == account.id }
                    accounts.append(account)
                    selection = account.id
                }
            } catch {
                guard !Task.isCancelled, requestedScope == scope else { return }
                failure = "Couldn't sign in: \(error.localizedDescription)"
            }
            guard requestedScope == scope else { return }
            signingIn = false
        }
    }

    @MainActor private func remove(_ account: AgentAccount) {
        let requestedScope = scope
        removing = account.id
        failure = nil
        Task {
            do {
                try await store.deleteAgentAccount(id: account.id)
                try Task.checkCancellation()
                guard requestedScope == scope else { return }
                withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
                    accounts.removeAll { $0.id == account.id }
                    if selection == account.id { selection = nil }
                }
            } catch {
                guard !Task.isCancelled, requestedScope == scope else { return }
                failure = "Couldn't remove the account: \(error.localizedDescription)"
            }
            guard requestedScope == scope else { return }
            removing = nil
        }
    }

    @MainActor private func signOutDefault() {
        let requestedScope = scope
        signingOut = true
        failure = nil
        Task {
            do {
                try await store.signOutAgent(backend: backend)
                try Task.checkCancellation()
            } catch {
                guard !Task.isCancelled, requestedScope == scope else { return }
                failure = "Couldn't sign out: \(error.localizedDescription)"
            }
            guard requestedScope == scope else { return }
            signingOut = false
        }
    }
}
