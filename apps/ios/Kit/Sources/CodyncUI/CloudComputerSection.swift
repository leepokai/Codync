import CodyncKit
import SwiftUI

/// Computers → Codync Cloud: the account's cloud computers on the user's own provider accounts, each with where it runs
/// and whether it's awake (or what to fix while its provider turns the key down), renamed, its provider's key opened, or
/// deleted (or its snapshots) from its `⋯`; "+" for another on a provider picked there, whose key opens from there too.
/// Hidden while this account can't have one.
public struct CloudComputerSection: View {
    let store: CloudMachineStore
    @State private var confirm: Confirm?
    @State private var renaming: CloudMachine?
    @State private var newName = ""
    /// The machine whose provider's key sheet is open.
    @State private var keyFor: CloudMachine?
    @State private var showsNew = false

    /// A delete waiting for the user's OK: only its snapshots, or the whole machine.
    private enum Confirm {
        case snapshots(CloudMachine), computer(CloudMachine)

        var title: String {
            switch self {
            case .snapshots: "Delete snapshots?"
            case let .computer(machine): L10n.fill(L10n.tr("Delete {name}?"), ["name": machine.name])
            }
        }

        var message: String {
            switch self {
            case .snapshots: "Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay."
            case .computer: "Erases everything on it: bots, chats, files and sign-ins. This can't be undone."
            }
        }

        var action: String {
            switch self {
            case .snapshots: "Delete snapshots"
            case .computer: "Delete cloud computer"
            }
        }
    }

    public init(store: CloudMachineStore) { self.store = store }

    public var body: some View {
        // Nothing at all while hidden, so the settings list keeps its spacing.
        if store.isShown {
            CardSection("Codync Cloud", footer: "Runs on your own account with a cloud provider, which bills you while it's awake. It sleeps when idle and wakes when you message it.") {
                ForEach(store.machines) { machineRow($0) }
                if let error = store.error(at: .card) { ErrorText(error) }
            } accessory: {
                // Codync Cloud is a beta feature.
                BetaTag()
                // At the limit too: the sheet says why it can't make another, and keeps every provider's key in reach.
                if store.available {
                    IconButton("New cloud computer", systemImage: "plus") { showsNew = true }
                        .disabled(store.working != nil)
                        .accessibilityIdentifier("newCloudComputer")
                }
            }
            .codyncSheet(isPresented: $showsNew) { NewCloudComputerSheet(store: store) }
            .codyncSheet(item: $keyFor) { CloudKeySheet(store: store, provider: $0.hosting.provider) }
            .codyncDialog(confirm?.title ?? "", isPresented: Binding(get: { confirm != nil }, set: { if !$0 { confirm = nil } }),
                          message: confirm?.message) {
                guard let target = confirm else { return [] }
                return [DialogAction(target.action, destructive: true) {
                    Task {
                        switch target {
                        case let .snapshots(machine): await store.deleteSnapshots(machine.id)
                        case let .computer(machine): await store.delete(machine.id)
                        }
                    }
                }]
            }
            .codyncDialog("Rename cloud computer", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } }),
                          field: DialogField("Name", text: $newName)) {
                // Read as the dialog shows: closing clears `renaming`.
                let target = renaming
                return [DialogAction("Save") {
                    if let target { Task { await store.rename(target.id, to: newName) } }
                }]
            }
            .onChange(of: newName) {
                let limited = CloudMachineStore.limited(newName)
                if limited != newName { newName = limited }
            }
        }
    }

    /// Its provider's logo, its name, then where it runs and its state (or what to fix while its provider turns the key
    /// down); `⋯` for what can be done with it.
    private func machineRow(_ machine: CloudMachine) -> some View {
        let status = store.status(of: machine)
        let state = Text(L10n.tr(status.text)).foregroundStyle(status.isError ? Palette.danger : Palette.secondary)
        return HStack(spacing: 12) {
            ProviderIcon(machine.hosting.provider)
            VStack(alignment: .leading, spacing: 3) {
                Text(verbatim: machine.name).foregroundStyle(Palette.text).lineLimit(1)
                Text("\(machine.hosting.providerName) · \(machine.hosting.region) · \(state)")
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            if store.isBusy(machine.id) {
                Spinner()
            } else {
                DropdownMenu { menu(machine) } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel(L10n.tr("More"))
                .disabled(store.working != nil)
                // A 44 pt target in a row of the card's height, its glyph near the card's edge.
                .padding(.vertical, -6)
                .padding(.trailing, -12)
            }
        }
    }

    private func menu(_ machine: CloudMachine) -> [MenuItem] {
        var items = [
            MenuItem("Rename…", icon: "pencil") {
                newName = machine.name
                renaming = machine
            },
            // Its provider's key, to replace it or see what to fix.
            MenuItem(L10n.fill(L10n.tr("{provider} API key"), ["provider": machine.hosting.providerName]), icon: "key.fill",
                     verbatim: true) { keyFor = machine },
        ]
        if machine.snapshots {
            items.append(MenuItem("Delete snapshots", icon: "clock.arrow.circlepath", enabled: !machine.isPreparing) {
                confirm = .snapshots(machine)
            })
        }
        items.append(MenuItem("Delete cloud computer", icon: "trash", destructive: true) {
            confirm = .computer(machine)
        })
        return items
    }
}

/// Why the last Codync Cloud action failed, translated: on the card, in New cloud computer and in a provider's key sheet.
struct ErrorText: View {
    let error: String

    init(_ error: String) { self.error = error }

    var body: some View {
        Text(L10n.tr(error))
            .font(.footnote)
            .foregroundStyle(Palette.danger)
            .fixedSize(horizontal: false, vertical: true)
            .transition(.opacity)
    }
}

/// Under a Codync Cloud computer's name: the provider and region the cloud names.
public struct CloudHostingLine: View {
    let hosting: CloudHosting

    public init(_ hosting: CloudHosting) { self.hosting = hosting }

    public var body: some View {
        Text(verbatim: hosting.runsOn)
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
            .lineLimit(1)
    }
}

extension CloudMachineState {
    var label: String {
        switch self {
        case .starting: "Starting"
        case .running: "Running"
        case .stopping: "Going to sleep"
        case .stopped: "Sleeping"
        case .failed: "Failed"
        case .unknown: "Unknown"
        }
    }
}
