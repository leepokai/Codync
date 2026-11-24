import CodyncKit
import SwiftUI

/// iMate's personal token is one value on the host, not a named login profile.
struct ImateTokenForm: View {
    let backend: Backend
    let back: () -> Void
    @Environment(BotStore.self) private var model
    @State private var token = ""
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                BackButton(action: back).keyboardShortcut(.cancelAction)
            } title: {
                Text(L10n.tr("iMate token")).font(.body.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            } trailing: {
                if busy { Spinner() } else {
                    IconButton("Save", systemImage: "checkmark") { save() }.disabled(token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            CardForm {
                CardSection(footer: L10n.fill(L10n.tr("Values are kept on {name} only and never shown again."), ["name": model.hostName])) {
                    SecureField(L10n.tr("Personal token"), text: $token)
                        .plainTextInput()
                    Text(L10n.fill(L10n.tr("Create a personal token on the {name} open platform."), ["name": "iMate"]))
                        .font(.footnote).foregroundStyle(Palette.secondary)
                }
                if let error {
                    CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
                }
            }
        }
        .background(Palette.background)
    }

    private func save() {
        guard let client = model.client else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                _ = try await client.imateSetToken(token.trimmingCharacters(in: .whitespacesAndNewlines))
                await model.refreshBackends()
                back()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
