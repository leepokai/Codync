import CodyncKit
import SwiftUI

/// Computers → Codync Cloud: the key to the user's own Boat account, then the account's cloud computer, where it runs
/// and whether it's awake, with deleting its snapshots or the whole computer; or creating one. Hidden while this
/// account can't have one.
public struct CloudComputerSection: View {
    let store: CloudMachineStore
    @State private var confirm: Confirm?

    private enum Confirm { case snapshots, computer }

    public init(store: CloudMachineStore) { self.store = store }

    public var body: some View {
        switch store.lookup {
        // Nothing at all, so the settings list keeps its spacing.
        case .loading, .unavailable:
            EmptyView()
        case .creatable:
            section {
                CloudKeyRows(store: store)
                if store.canCreate {
                    actionRow("Create cloud computer", icon: "plus", work: .creating, destructive: false) { Task { await store.create() } }
                }
            }
        case let .machine(machine):
            section {
                machineRows(machine)
                CloudKeyRows(store: store)
            }
        }
    }

    private func section(@ViewBuilder _ rows: () -> some View) -> some View {
        CardSection("Codync Cloud", footer: "Runs on your own Boat account, which bills you while it's awake. It sleeps when idle and wakes when you message it.") {
            rows()
            if let error = store.error {
                Text(L10n.tr(error))
                    .font(.footnote)
                    .foregroundStyle(Palette.danger)
                    .fixedSize(horizontal: false, vertical: true)
                    .transition(.opacity)
            }
        } accessory: {
            // Codync Cloud is a beta feature.
            BetaTag()
        }
        .codyncDialog(confirm == .computer ? "Delete your cloud computer?" : "Delete snapshots?",
                      isPresented: Binding(get: { confirm != nil }, set: { if !$0 { confirm = nil } }),
                      message: confirm == .computer
                          ? "Erases everything on it: bots, chats, files and sign-ins. This can't be undone."
                          : "Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay.") {
            let deleting = confirm
            return [DialogAction(deleting == .computer ? "Delete cloud computer" : "Delete snapshots", destructive: true) {
                Task { deleting == .computer ? await store.delete() : await store.deleteSnapshots() }
            }]
        }
    }

    @ViewBuilder private func machineRows(_ machine: CloudMachine) -> some View {
        let runsOn = machine.hosting.runsOn
        if store.isPreparing {
            ValueRow(runsOn, detail: "Preparing your cloud computer…") { Spinner() }
        } else {
            ValueRow(runsOn, value: machine.hosting.state.label)
        }
        actionRow("Delete snapshots", icon: "clock.arrow.circlepath", work: .deletingSnapshots) {
            confirm = .snapshots
        }
        .disabled(store.isPreparing)
        actionRow("Delete cloud computer", icon: "trash", work: .deleting) { confirm = .computer }
    }

    private func actionRow(_ title: String, icon: String, work: CloudMachineStore.Work, destructive: Bool = true,
                           action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Label(L10n.tr(title), systemImage: icon)
                    .foregroundStyle(destructive ? Palette.danger : Palette.text)
                Spacer(minLength: 8)
                if store.working == work { Spinner() }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(store.working != nil)
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
