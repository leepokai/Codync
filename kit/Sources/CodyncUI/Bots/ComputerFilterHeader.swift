import CodyncKit
import SwiftUI

/// The roster's only connection surface: a quiet summary, with details and filters on demand.
public struct ComputerFilterHeader: View {
    let accounts: AccountStore
    @Binding var hidden: String
    let manage: () -> Void
    let compact: Bool
    let connectingComputers: [ComputerConnectionProgress]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(accounts: AccountStore, hidden: Binding<String>, compact: Bool = false,
                connectingComputers: [ComputerConnectionProgress] = [], manage: @escaping () -> Void) {
        self.accounts = accounts
        _hidden = hidden
        self.manage = manage
        self.compact = compact
        self.connectingComputers = connectingComputers
    }

    private var selection: ComputerSelection {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hidden)
    }

    private var stores: [BotStore] { selection.shown.compactMap { accounts.store(for: $0) } }
    private var progress: [ComputerConnectionProgress] {
        connectingComputers.filter { pending in
            guard let id = pending.computerId, accounts.computers.contains(where: { $0.id == id }) else { return true }
            return selection.shown.contains(id)
        }
    }
    private var unattached: [ComputerConnectionProgress] {
        progress.filter { pending in !accounts.computers.contains { $0.id == pending.computerId } }
    }
    private var summary: String {
        #if os(iOS)
        ConnectionSummary(connections: stores.map(\.shownConnection)).text
        #else
        ConnectionSummary(computers: Dictionary(uniqueKeysWithValues: stores.map { ($0.computer.id, $0.shownConnection) }),
                          progress: progress).progressText
        #endif
    }

    public var body: some View {
        #if os(iOS)
        Menu {
            Button("All computers", systemImage: selection.shown.count == accounts.computers.count ? "checkmark" : "desktopcomputer") { update("") }
            ForEach(accounts.computers) { computer in
                Button {
                    update(selection.toggling(computer.id))
                } label: {
                    Label(computerTitle(computer), systemImage: selection.shown.contains(computer.id) ? "checkmark" : "desktopcomputer")
                }
                .menuActionDismissBehavior(.disabled)
            }
            if accounts.computers.count > 1 {
                Menu("Show only") {
                    ForEach(accounts.computers) { computer in
                        Button(computer.name) { update(selection.only(computer.id)) }
                    }
                }
            }
            Divider()
            Button("Reconnect", systemImage: "arrow.clockwise", action: reconnect)
                .disabled(stores.isEmpty)
            Button("Manage computers", systemImage: "desktopcomputer", action: manage)
        } label: { label }
        .accessibilityLabel("Computers, \(summary). Filter conversations")
        #else
        DropdownMenu {
            var items = [MenuItem("All computers", selected: selection.shown.count == accounts.computers.count) { update("") }]
            items += accounts.computers.map { computer in
                MenuItem(computerTitle(computer), selected: selection.shown.contains(computer.id)) {
                    update(selection.toggling(computer.id))
                }
            }
            items += unattached.map { pending in
                MenuItem("\(pending.name) · \(pending.detail)", icon: "terminal", action: manage)
            }
            if accounts.computers.count > 1 {
                items += accounts.computers.enumerated().map { index, computer in
                    MenuItem("Only \(computer.name)", divider: index == 0) { update(selection.only(computer.id)) }
                }
            }
            if !stores.isEmpty {
                items.append(MenuItem("Reconnect", icon: "arrow.clockwise", divider: true, action: reconnect))
            }
            items.append(MenuItem("Manage computers", icon: "desktopcomputer", action: manage))
            return items
        } label: { label }
        .accessibilityLabel("Computers, \(summary). Filter conversations")
        #endif
    }

    private var label: some View {
        HStack(spacing: 4) {
            if compact {
                Image(systemName: "desktopcomputer")
            } else {
                Text(summary).lineLimit(1).contentTransition(.opacity)
            }
            Image(systemName: "chevron.down").appFont(.system(size: 8, weight: .semibold))
        }
        .appFont(.system(size: 12, weight: .medium))
        .foregroundStyle(Palette.secondary)
        .help(summary)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: summary)
    }

    private func computerTitle(_ computer: Computer) -> String {
        guard let store = accounts.store(for: computer.id) else { return computer.name }
        #if os(macOS)
        if store.shownConnection != .online, let pending = progress.first(where: { $0.computerId == computer.id }) {
            return "\(store.hostName) · \(pending.detail)"
        }
        #endif
        return "\(store.hostName) · \(store.connectionLabel)"
    }

    private func update(_ value: String) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { hidden = value }
    }

    private func reconnect() { for store in stores { store.restartStream() } }
}

public extension BotStore {
    var connectionLabel: String {
        switch shownConnection {
        case .online where mismatch != nil: "Needs update"
        case .online: "Connected"
        case .connecting: "Connecting…"
        case .computerOffline, .offline: "Offline"
        case .unauthorized: "No access"
        case .unpaired: "Not paired"
        }
    }
}
