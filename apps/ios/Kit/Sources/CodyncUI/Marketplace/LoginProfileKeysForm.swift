import CodyncKit
import SwiftUI

/// The variables of an API key login profile: typed here, or imported from the computer's login shell. Values are
/// kept on the computer and never come back; a saved one shows only its name.
struct LoginProfileKeysForm: View {
    let backend: Backend
    let profile: LoginProfile
    let back: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var rows: [Row]
    /// Saved variables the user removed: sent with an empty value on Save.
    @State private var removed: [String] = []
    /// The login shell's variable names, once Import was asked for.
    @State private var shell: [String]?
    @State private var picked: Set<String> = []
    @State private var busy: Busy?
    @State private var error: String?

    private enum Busy { case saving, loading, importing }

    private struct Row: Identifiable {
        let id = UUID()
        var name: String
        var value = ""
        /// The profile already has it: its name is fixed and its value replaced only when typed.
        var saved = false
    }

    init(backend: Backend, profile: LoginProfile, back: @escaping () -> Void) {
        self.backend = backend
        self.profile = profile
        self.back = back
        let saved = !profile.vars.isEmpty
        _rows = State(initialValue: profile.keyNames(suggested: backend.loginKeys ?? []).map { Row(name: $0, saved: saved) })
    }

    private var changes: [String: String] {
        LoginProfile.keysToSave(rows: rows.map { (name: $0.name, value: $0.value) }, removed: removed)
    }

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                BackButton(action: leave).keyboardShortcut(.cancelAction)
            } title: {
                Text(profile.name ?? L10n.tr("API key")).font(.body.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            } trailing: {
                if busy == .saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") { send(.saving, importing: []) }
                        .disabled(changes.isEmpty || busy != nil)
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var form: some View {
        CardForm {
            CardSection(footer: L10n.fill(L10n.tr("Values are kept on {name} only and never shown again."), ["name": model.hostName])) {
                ForEach($rows) { $row in
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 4) {
                            if row.saved {
                                Text(row.name).font(.caption).foregroundStyle(Palette.secondary).textSelection(.enabled)
                            } else {
                                TextField(L10n.tr("Variable name"), text: $row.name).plainTextInput().font(.caption)
                            }
                            SecureField(L10n.tr("Value"), text: $row.value,
                                        prompt: Text(L10n.tr(row.saved ? "Saved (type to replace)" : "Value")))
                                .plainTextInput()
                        }
                        if row.saved {
                            IconButton("Remove", systemImage: "trash") { remove(row) }
                        }
                    }
                    .transition(.opacity)
                }
                SetupOptionRow(title: "Add variable", icon: "plus") {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { rows.append(Row(name: "")) }
                }
                SetupOptionRow(title: "Import from login shell", icon: "square.and.arrow.down", busy: busy == .loading) { loadShell() }
            }
            if let shell {
                CardSection(footer: L10n.fill(L10n.tr("Pick the variables this login profile should use. Their values are copied now and kept on {name}."), ["name": model.hostName])) {
                    ForEach(shell, id: \.self) { name in
                        Button { picked.formSymmetricDifference([name]) } label: {
                            HStack(spacing: 10) {
                                Text(name).foregroundStyle(Palette.text).lineLimit(1)
                                Spacer(minLength: 8)
                                Image(systemName: "checkmark")
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(Palette.text)
                                    .opacity(picked.contains(name) ? 1 : 0)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(picked.contains(name) ? .isSelected : [])
                    }
                    HStack {
                        Spacer(minLength: 0)
                        if busy == .importing {
                            Spinner()
                        } else {
                            StepAction(label: "Import") { send(.importing, importing: picked.sorted()) }
                                .disabled(picked.isEmpty)
                        }
                    }
                }
                .transition(.opacity)
            }
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
                    .transition(.opacity)
            }
        }
        .textFieldStyle(.plain)
        .disabled(busy != nil)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: error)
    }

    private func remove(_ row: Row) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
            removed.append(row.name)
            rows.removeAll { $0.id == row.id }
        }
    }

    /// Back to the profile's row, with the values emptied first: iOS offers to keep a filled secure field that goes
    /// away in the phone's Passwords, and a key stays on the computer.
    private func leave() {
        for index in rows.indices { rows[index].value = "" }
        back()
    }

    private func loadShell() {
        run(.loading) {
            let names = try await model.loginShellVariables()
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { shell = names }
        }
    }

    /// Saves what was typed and removed, with `importing` read from the login shell; then back to the profile's row.
    private func send(_ kind: Busy, importing: [String]) {
        let vars = changes
        run(kind) {
            try await model.setLoginProfileKeys(profile.id, vars: vars, importing: importing)
            leave()
        }
    }

    private func run(_ kind: Busy, _ work: @escaping () async throws -> Void) {
        busy = kind
        Task {
            defer { busy = nil }
            do {
                try await work()
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
