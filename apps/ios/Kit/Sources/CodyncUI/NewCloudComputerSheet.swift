import CodyncKit
import SwiftUI

/// "+" on the Codync Cloud card, or Start a cloud computer on the first-run pages: three steps in one sheet, the provider
/// (each with its region, and its key or what it costs), its key when it has none that works (`CloudKeyForm`), then the
/// new computer's name. A provider with a key opens it, or removes it, from its row's `⋯`. At the account's limit it
/// says so, and Create is off.
public struct NewCloudComputerSheet: View {
    let store: CloudMachineStore
    /// The cloud made it; the sheet closes after.
    var created: () -> Void = {}
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var step = Step.provider
    @State private var name = ""
    /// The provider whose key sheet is open.
    @State private var keyFor: CloudProvider?

    /// The provider a later step is for, by its id: read from the store, so a key saved meanwhile shows.
    private enum Step: Equatable { case provider, key(String), name(String) }

    private var creating: Bool { store.working == .creating }

    public init(store: CloudMachineStore, created: @escaping () -> Void = {}) {
        self.store = store
        self.created = created
    }

    public var body: some View {
        // One step at a time, swapped in place.
        ZStack {
            switch step {
            case .provider: providerStep
            case let .key(id): if let provider = listed(id) { keyStep(provider) }
            case let .name(id): if let provider = listed(id) { nameStep(provider) }
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Palette.background)
        .codyncSheet(item: $keyFor) { CloudKeySheet(store: store, provider: $0.provider) }
        // A row's key removal that failed in an earlier opening is over (a create's clears on the way to its step).
        .onAppear {
            for provider in store.providers { store.clearError(at: .key(provider.provider)) }
        }
        .onChange(of: name) {
            let limited = CloudMachineStore.limited(name)
            if limited != name { name = limited }
        }
    }

    private func listed(_ id: String) -> CloudProvider? { store.providers.first { $0.provider == id } }

    // MARK: - Steps

    private var providerStep: some View {
        VStack(spacing: 0) {
            ModalHeader("New cloud computer")
            CardForm {
                CardSection("Provider") {
                    ForEach(store.providers) { row($0) }
                }
                // A key removed from a row's `⋯` fails here, where it was asked.
                if let error = store.keyRemovalError ?? store.limitNote { ErrorText(error) }
            }
        }
    }

    private func keyStep(_ provider: CloudProvider) -> some View {
        VStack(spacing: 0) {
            ModalHeader { backTitle(L10n.fill(L10n.tr("Connect {provider}"), ["provider": provider.name])) }
            CloudKeyForm(store: store, provider: provider) { go(.name(provider.provider)) }
        }
    }

    private func nameStep(_ provider: CloudProvider) -> some View {
        VStack(spacing: 0) {
            ModalHeader { backTitle(L10n.tr("New cloud computer")) } trailing: {
                if creating {
                    Spinner()
                } else {
                    IconButton("Create", systemImage: "checkmark") { create(on: provider) }
                        .disabled(!provider.isReady || store.working != nil || store.atLimit)
                        .accessibilityIdentifier("createCloudComputer")
                }
            }
            CardForm {
                CardSection("Name", footer: CloudHosting.runsOn(providerName: provider.name, region: provider.region)) {
                    TextField(L10n.tr("Name your cloud computer"), text: $name, prompt: Text(verbatim: store.suggestedName))
                        .submitLabel(.done)
                        .onSubmit { create(on: provider) }
                        .accessibilityIdentifier("cloudComputerName")
                }
                if let error = store.error(at: .newComputer) ?? store.limitNote { ErrorText(error) }
            }
            .textFieldStyle(.plain)
        }
    }

    /// A later step's title, after the way back to the providers (off while a key saves or the computer is made).
    private func backTitle(_ title: String) -> some View {
        HStack(spacing: 4) {
            BackButton { go(.provider) }
                .disabled(store.working != nil)
                // Its glyph near the sheet's edge, as the close button's is on the other side.
                .padding(.leading, -12)
            Text(verbatim: title)
        }
    }

    // MARK: - Providers

    /// Its logo on a tile, its name and detail, and a chevron; a `⋯` on one with a key it takes.
    private func row(_ provider: CloudProvider) -> some View {
        HStack(spacing: 4) {
            Button { go(provider.isReady ? .name(provider.provider) : .key(provider.provider)) } label: {
                HStack(spacing: 12) {
                    ProviderIcon(provider.provider)
                        .frame(width: 36, height: 36)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: provider.name).foregroundStyle(Palette.text).lineLimit(1)
                        detail(provider).font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 8)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(store.working != nil)
            .accessibilityIdentifier("cloudProvider.\(provider.provider)")
            if store.working == .removingKey(provider.provider) {
                // Where the `⋯` glyph sits, so the chevron stays put.
                Spinner().frame(width: 44)
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
                // A 44 pt target in the row.
                .padding(.vertical, -6)
            }
            Image(systemName: "chevron.right")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .accessibilityHidden(true)
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

    /// Its detail, in the danger tone while the provider turns the key down.
    private func detail(_ provider: CloudProvider) -> Text {
        let detail = provider.detail
        return Text(verbatim: detail.text).foregroundStyle(detail.isProblem ? Palette.danger : Palette.secondary)
    }

    // MARK: - Actions

    /// A create that failed was for the step left.
    private func go(_ next: Step) {
        store.clearError(at: .newComputer)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { step = next }
    }

    private func create(on provider: CloudProvider) {
        guard provider.isReady, store.working == nil, !store.atLimit else { return }
        Task {
            guard await store.create(on: provider.provider, name: name) else { return }
            created()
            dismiss()
        }
    }
}
