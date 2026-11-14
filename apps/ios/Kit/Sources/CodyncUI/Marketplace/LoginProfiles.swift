import CodyncKit
import SwiftUI

/// An agent's login profiles in one place: the default one (the login the computer already has, with its sign-in
/// options as `options`) and the others, each an account (the agent's own sign-in) or an API key. Each can be named;
/// others are added (an account's sign-in or an API key's form follows), changed or removed. Bots pick theirs in
/// their own settings.
struct LoginProfiles<Options: View>: View {
    let backend: Backend
    /// The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again.
    let status: String
    let checking: Bool
    let check: () -> Void
    /// Opens the sign-in terminal for an account login profile.
    let signIn: (String) -> Void
    /// Opens the key form of an API key login profile.
    let editKeys: (String) -> Void
    /// An account login profile whose sign-in was asked for (a chat's sign-in card), started here once
    /// (`signInStarted`) when its agent signs itself in.
    var signInNow: String?
    var signInStarted: () -> Void = {}
    @ViewBuilder let options: Options
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The login profile being changed, `own` or one of the Add rows.
    @State private var busy: String?
    @State private var error: String?
    @State private var renaming: Renaming?
    @State private var newName = ""

    private let own = ""
    private let addingAccount = "+account"
    private let addingKey = "+apiKey"
    private let nameLimit = 60

    /// The login profile being named (`loginProfile` nil: the default one) and the name the user gave it so far.
    private struct Renaming {
        var loginProfile: String?
        var label: String?
    }

    var body: some View {
        CardSection("Login profiles", footer: "A bot picks its login profile under Agent in its settings. Bots that pick none use the default one.") {
            let mine = backend.defaultLoginProfile
            LoginProfileLine(
                name: mine?.name ?? L10n.tr("Default login profile"),
                detail: [L10n.tr("Default"), mine?.label == nil ? nil : mine?.email, L10n.tr(status)],
                rename: { rename(nil, label: mine?.label) }
            ) {
                CheckAgain(checking: checking, check: check)
            }
            options
            ForEach(backend.loginProfiles ?? []) { profile in
                LoginProfileLine(
                    name: profile.name ?? L10n.tr("New login profile"),
                    detail: detail(profile),
                    rename: { rename(profile.id, label: profile.label) }
                ) {
                    switch profile.kind {
                    case .account:
                        if !profile.signedIn {
                            StepAction(label: "Sign in") { run(profile.id) { try await start(profile.id) } }
                        }
                    case .apiKey:
                        IconButton("Edit keys", systemImage: "key") { editKeys(profile.id) }
                    }
                    if busy == profile.id {
                        Spinner()
                    } else {
                        IconButton("Remove login profile", systemImage: "trash") {
                            run(profile.id) { try await model.removeLoginProfile(profile.id) }
                        }
                    }
                }
            }
            // An account signs in with the agent's own CLI, so it needs that installed; an agent that keeps one
            // sign-in of its own takes API key profiles only, which any agent can have.
            if backend.loginProfileSignIn != nil, backend.installed == true {
                SetupOptionRow(
                    title: "Add account",
                    detail: L10n.fill(
                        L10n.tr(backend.loginProfilesSignInInBrowser
                            ? "Signs in another {name} account in a browser on {other}."
                            : "Signs in another {name} account in a terminal on {other}."),
                        ["name": backend.name, "other": model.hostName]),
                    icon: "plus", busy: busy == addingAccount
                ) {
                    run(addingAccount) { try await start(try await model.addLoginProfile(backend.id, kind: .account).id) }
                }
            }
            SetupOptionRow(
                title: "Add API key",
                detail: L10n.fill(L10n.tr("Runs {name} on keys you type or import from your login shell."), ["name": backend.name]),
                icon: "key", busy: busy == addingKey
            ) {
                run(addingKey) { editKeys(try await model.addLoginProfile(backend.id, kind: .apiKey).id) }
            }
            if let error {
                Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled).transition(.opacity)
            }
        }
        .disabled(busy != nil)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: backend.loginProfiles)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: error)
        .codyncDialog("Rename login profile", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } }),
                      field: DialogField("Login profile name", text: $newName)) {
            // Read as the dialog shows: closing clears `renaming`.
            let target = renaming
            return [DialogAction("Save") {
                // An empty name goes back to who it signs in.
                guard let target, newName.trimmingCharacters(in: .whitespacesAndNewlines) != target.label ?? "" else { return }
                run(target.loginProfile ?? own) {
                    try await model.renameLoginProfile(backend.id, loginProfile: target.loginProfile, name: newName)
                }
            }]
        }
        .onChange(of: newName) {
            if newName.count > nameLimit { newName = String(newName.prefix(nameLimit)) }
        }
        .task {
            // Once: the section is built again each time the sheet comes back to its overview.
            guard let signInNow, backend.loginProfilesSignInInBrowser else { return }
            signInStarted()
            if let profile = backend.loginProfiles?.first(where: { $0.id == signInNow }), profile.kind == .account, !profile.signedIn {
                run(signInNow) { try await start(signInNow) }
            }
        }
    }

    private func detail(_ profile: LoginProfile) -> [String?] {
        switch profile.kind {
        case .account:
            return [profile.label == nil ? nil : profile.email, signInState(profile)]
        case .apiKey:
            return [L10n.tr("API key"), profile.vars.isEmpty ? L10n.tr("No key yet") : profile.vars.joined(separator: ", ")]
        }
    }

    private func signInState(_ profile: LoginProfile) -> String {
        if busy == profile.id, !profile.signedIn, backend.loginProfilesSignInInBrowser {
            return L10n.fill(L10n.tr("Finish signing in in the browser on {name}…"), ["name": model.hostName])
        }
        return L10n.tr(profile.signedIn ? "Signed in" : "Not signed in")
    }

    private func rename(_ loginProfile: String?, label: String?) {
        newName = label ?? ""
        renaming = Renaming(loginProfile: loginProfile, label: label)
    }

    /// Its agent signs the account in itself (a browser on the computer; the login profile is `busy` until that
    /// ends), or its CLI does in a terminal.
    private func start(_ loginProfile: String) async throws {
        guard backend.loginProfilesSignInInBrowser else { return signIn(loginProfile) }
        busy = loginProfile
        try await model.signInLoginProfile(backend.id, loginProfile: loginProfile)
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

/// One login profile: its name, what's known of it, the pencil that names it, and its actions.
private struct LoginProfileLine<Actions: View>: View {
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
            IconButton("Rename login profile", systemImage: "pencil", action: rename)
            actions
        }
    }
}
