import CodyncKit
import SwiftUI

/// "+" on the Codync Cloud card: names another cloud computer and picks the provider it runs on. A provider without a
/// key it takes opens that provider's key sheet to add one; back here, its row shows the saved key and it's picked. One
/// with a key opens it, or removes it, from its row's `⋯`. At the account's limit it says so, and Create is off.
struct NewCloudComputerSheet: View {
    let store: CloudMachineStore
    @Environment(\.dismissModal) private var dismiss
    @State private var name = ""
    @State private var picked: String?
    /// The provider whose key sheet is open.
    @State private var keyFor: CloudProvider?

    private var selected: String? { store.provider(picked: picked) }

    private var creating: Bool { store.working == .creating }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("New cloud computer") {
                if creating {
                    Spinner()
                } else {
                    IconButton("Create", systemImage: "checkmark", action: create)
                        .disabled(selected == nil || store.working != nil || store.atLimit)
                        .accessibilityIdentifier("createCloudComputer")
                }
            }
            CardForm {
                CardSection("Name") {
                    TextField(L10n.tr("Name your cloud computer"), text: $name, prompt: Text(verbatim: store.suggestedName))
                        .submitLabel(.done)
                        .onSubmit(create)
                        .accessibilityIdentifier("cloudComputerName")
                }
                CardSection("Provider") {
                    ForEach(store.providers) { row($0) }
                }
                // A key removed from a row's `⋯` fails here, where it was asked.
                if let error = store.error(at: .newComputer) ?? store.keyRemovalError ?? store.limitNote { ErrorText(error) }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
        .codyncSheet(item: $keyFor) { CloudKeySheet(store: store, provider: $0.provider) }
        // A failure from an earlier opening is over, a row's key removal too.
        .onAppear {
            store.clearError(at: .newComputer)
            for provider in store.providers { store.clearError(at: .key(provider.provider)) }
        }
        .onChange(of: name) {
            let limited = CloudMachineStore.limited(name)
            if limited != name { name = limited }
        }
    }

    /// Its logo on a tile, its name and its key; a checkmark on the picked one, and a `⋯` on one with a key it takes.
    private func row(_ provider: CloudProvider) -> some View {
        HStack(spacing: 4) {
            pickButton(provider)
            if store.working == .removingKey(provider.provider) {
                // Where the `⋯` glyph sits, so the checkmark stays put.
                Spinner().frame(width: 44).padding(.trailing, -12)
            } else if provider.isReady {
                DropdownMenu { menu(provider) } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel(L10n.tr("More"))
                .accessibilityIdentifier("cloudProviderKey.\(provider.provider)")
                .disabled(store.working != nil)
                // A 44 pt target in the row, its glyph near the card's edge (like a cloud computer's `⋯`).
                .padding(.vertical, -6)
                .padding(.trailing, -12)
            }
        }
    }

    /// Its key (replace it or see what to fix), and removing it while no cloud computer runs there.
    private func menu(_ provider: CloudProvider) -> [MenuItem] {
        var items = [
            MenuItem(L10n.fill(L10n.tr("{provider} API key"), ["provider": provider.name]), icon: "key.fill", verbatim: true) {
                keyFor = provider
            },
        ]
        if store.canRemoveKey(provider.provider) {
            items.append(MenuItem("Remove key", icon: "trash", destructive: true) {
                Task { await store.removeKey(provider: provider.provider) }
            })
        }
        return items
    }

    private func pickButton(_ provider: CloudProvider) -> some View {
        let isSelected = selected == provider.provider
        return Button { tap(provider) } label: {
            HStack(spacing: 12) {
                ProviderIcon(provider.provider)
                    .frame(width: 36, height: 36)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: provider.name).foregroundStyle(Palette.text).lineLimit(1)
                    detail(provider).font(.caption).fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                if provider.isReady {
                    Image(systemName: "checkmark")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Palette.text)
                        .opacity(isSelected ? 1 : 0)
                        .accessibilityHidden(true)
                } else {
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(Palette.tertiary)
                        .accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(creating)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier("cloudProvider.\(provider.provider)")
    }

    /// Where it runs and the key's hint; what to fix while the provider turns the key down; or that it needs one.
    private func detail(_ provider: CloudProvider) -> Text {
        if let problem = provider.credential?.problem {
            return Text(L10n.tr(problem)).foregroundStyle(Palette.danger)
        }
        let key = provider.credential?.hint ?? L10n.tr(provider.offer == nil ? "Add a key first" : "Free sandbox available")
        return Text(verbatim: "\(provider.region) · \(key)").foregroundStyle(Palette.secondary)
    }

    private func tap(_ provider: CloudProvider) {
        Motion.animate { picked = provider.provider }
        if !provider.isReady { keyFor = provider }
    }

    private func create() {
        guard let provider = selected, store.working == nil, !store.atLimit else { return }
        Task { if await store.create(on: provider, name: name) { dismiss() } }
    }
}
