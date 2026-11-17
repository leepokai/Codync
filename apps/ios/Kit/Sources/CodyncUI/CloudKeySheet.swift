import CodyncKit
import SwiftUI

/// One provider's key, from New cloud computer or a cloud computer's `⋯`: the key to the user's own account there,
/// pasted once, checked and kept encrypted by the cloud, and shown afterwards only by its last four; while the provider
/// turns it down, what to fix shows under it. Saving one closes it.
public struct CloudKeySheet: View {
    let store: CloudMachineStore
    let provider: String
    @Environment(\.dismissModal) private var dismiss

    public init(store: CloudMachineStore, provider: String) {
        self.store = store
        self.provider = provider
    }

    public var body: some View {
        VStack(spacing: 0) {
            if let provider = store.providers.first(where: { $0.provider == self.provider }) {
                ModalHeader { Text(verbatim: L10n.fill(L10n.tr("{provider} API key"), ["provider": provider.name])) }
                CardForm {
                    CardSection(footer: "Your cloud computer runs on your own account. Keys stay encrypted; only their last four show.") {
                        CloudKeyRow(store: store, provider: provider) { dismiss() }
                        if let offer = provider.offer { CloudOfferRow(provider: provider.provider, offer: offer) }
                        CloudKeySetupRow(provider: provider)
                    }
                }
            }
        }
        .background(Palette.background)
        // Closed, its failure was seen: New cloud computer under it shows only a removal asked from its own row.
        .onDisappear { store.clearError(at: .key(provider)) }
    }
}

/// One provider's key: its last four with replace and remove, or a field to paste it; `done` once one is saved.
private struct CloudKeyRow: View {
    let store: CloudMachineStore
    let provider: CloudProvider
    let done: () -> Void
    @State private var key = ""
    @State private var replacing = false
    @FocusState private var keyFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let credential = provider.credential, !replacing {
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
            if let error = store.error(at: .key(provider.provider)) { ErrorText(error) }
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: replacing)
    }

    private func saved(_ credential: CloudCredential) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "key.fill")
                .foregroundStyle(credential.problem == nil ? Palette.secondary : Palette.danger)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(L10n.tr("API key")).foregroundStyle(Palette.text).lineLimit(1)
                Text(verbatim: credential.hint).font(.caption.monospaced()).foregroundStyle(Palette.tertiary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if store.working == .removingKey(provider.provider) {
                Spinner()
            } else {
                IconButton("Replace key", systemImage: "pencil") { replacing = true }
                if store.canRemoveKey(provider.provider) {
                    IconButton("Remove key", systemImage: "trash") { Task { await store.removeKey(provider: provider.provider) } }
                }
            }
        }
        .disabled(store.working != nil)
    }

    private var entry: some View {
        HStack(spacing: 8) {
            SecureField(L10n.fill(L10n.tr("{provider} API key"), ["provider": provider.name]), text: $key,
                        prompt: Text(verbatim: provider.keyPrompt))
                .plainTextInput()
                .textFieldStyle(.plain)
                .focused($keyFocused)
                .onSubmit(save)
            if replacing {
                Button(L10n.tr("Cancel")) { replacing = false; key = "" }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.secondary)
            }
            if store.working == .savingKey(provider.provider) {
                Spinner()
            } else {
                Button(L10n.tr("Save"), action: save)
                    .buttonStyle(.primary)
                    .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.working != nil)
            }
        }
    }

    /// Drops the keyboard first: left up, it covers why the provider turned the key down.
    private func save() {
        let typed = key
        keyFocused = false
        Task {
            if await store.saveKey(typed, provider: provider.provider) {
                key = ""
                replacing = false
                done()
            }
        }
    }
}

/// The provider's own free plan: what it includes, and its page.
private struct CloudOfferRow: View {
    let provider: String
    let offer: CloudProviderOffer

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L10n.tr(offer.detail))
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
            WebLink(offer.label, url: offer.url) { Analytics.shared.capture(.cloudOfferOpened(provider: provider)) }
                .font(.footnote)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// How to make the key, folded away: the cloud's own steps, to copy, or the provider's page.
private struct CloudKeySetupRow: View {
    let provider: CloudProvider
    @State private var open = false
    @State private var copied = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Disclosure(isExpanded: $open) {
            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.fill(L10n.tr("Make a key with only what Codync needs in your {provider} account, then paste it above."), ["provider": provider.name]))
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                commands
                WebLink(L10n.fill(L10n.tr("Open {provider}"), ["provider": provider.name]), url: provider.setup.page).font(.footnote)
            }
        } label: {
            Text(L10n.tr("How to make a key")).foregroundStyle(Palette.text)
        }
    }

    /// One command a line, scrolled sideways rather than wrapped: wrapping hyphenates them into commands that don't exist.
    private var commands: some View {
        HStack(alignment: .top, spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                Text(verbatim: provider.setup.commands)
                    .font(.caption.monospaced())
                    .foregroundStyle(Palette.text)
                    .textSelection(.enabled)
                    .fixedSize()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            IconButton(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                Pasteboard.copy(provider.setup.commands)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = true }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
