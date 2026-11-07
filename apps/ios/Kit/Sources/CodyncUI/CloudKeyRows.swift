import CodyncKit
import SwiftUI

/// Codync Cloud → the key to the user's own Boat account: pasted once, checked and kept encrypted by the cloud, and
/// shown afterwards only by its last four. While Boat turns it down, what to fix shows under it.
struct CloudKeyRows: View {
    let store: CloudMachineStore
    @State private var key = ""
    @State private var replacing = false
    @State private var copied = false
    @FocusState private var keyFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let credential = store.credential, !replacing {
                saved(credential)
                if let problem = credential.problem {
                    Label(L10n.tr(problem), systemImage: "exclamationmark.circle.fill")
                        .font(.footnote)
                        .foregroundStyle(Palette.danger)
                        .fixedSize(horizontal: false, vertical: true)
                        .transition(.opacity)
                }
            } else {
                entry
            }
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: replacing)
    }

    private func saved(_ credential: CloudCredential) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "key.fill")
                .foregroundStyle(credential.problem == nil ? Palette.secondary : Palette.danger)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(L10n.tr("Boat API key")).foregroundStyle(Palette.text).lineLimit(1)
                Text(verbatim: credential.hint).font(.caption.monospaced()).foregroundStyle(Palette.tertiary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if store.working == .removingKey {
                Spinner()
            } else {
                IconButton("Replace key", systemImage: "pencil") { replacing = true }
                if store.canRemoveKey {
                    IconButton("Remove key", systemImage: "trash") { Task { await store.removeKey() } }
                }
            }
        }
        .disabled(store.working != nil)
    }

    /// How to make the key, then a field to paste it.
    private var entry: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let setup = store.keySetup {
                Text(L10n.tr("Make a key limited to Codync in your Boat account, then paste it here. Codync keeps it encrypted and never shows it again."))
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                commands(setup.commands)
                WebLink("Open Boat", url: setup.page).font(.footnote)
            }
            HStack(spacing: 8) {
                SecureField("Boat API key", text: $key, prompt: Text(verbatim: "boat_…"))
                    .plainTextInput()
                    .textFieldStyle(.plain)
                    .focused($keyFocused)
                    .onSubmit(save)
                if store.working == .savingKey {
                    Spinner()
                } else {
                    Button(L10n.tr("Save"), action: save)
                        .buttonStyle(.primary)
                        .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.working != nil)
                }
            }
            if replacing {
                Button(L10n.tr("Cancel")) { replacing = false; key = "" }
                    .buttonStyle(.plain)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private func commands(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(verbatim: text)
                .font(.caption.monospaced())
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            IconButton(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                Pasteboard.copy(text)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = true }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }

    /// Drops the keyboard first: left up, it covers why Boat turned the key down and Cancel under the field.
    private func save() {
        let typed = key
        keyFocused = false
        Task {
            if await store.saveKey(typed) {
                key = ""
                replacing = false
            }
        }
    }
}
