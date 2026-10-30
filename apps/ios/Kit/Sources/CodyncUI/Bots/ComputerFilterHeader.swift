import CodyncKit
import SwiftUI

/// The roster's only connection surface: a quiet summary, with details and filters on demand.
public struct ComputerFilterHeader: View {
    let accounts: AccountStore
    @Binding var hidden: String
    let manage: () -> Void
    let compact: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(accounts: AccountStore, hidden: Binding<String>, compact: Bool = false, manage: @escaping () -> Void) {
        self.accounts = accounts
        _hidden = hidden
        self.manage = manage
        self.compact = compact
    }

    private var selection: ComputerSelection {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hidden)
    }

    private var stores: [BotStore] { selection.shown.compactMap { accounts.store(for: $0) } }
    private var summary: String { ConnectionSummary(connections: stores.map(\.shownConnection)).text }

    public var body: some View {
        Menu {
            Toggle(isOn: Binding(get: { selection.shown.count == accounts.computers.count }, set: { _ in update("") })) {
                Label(L10n.tr("All computers"), systemImage: "desktopcomputer")
            }
            ForEach(accounts.computers) { computer in
                Toggle(isOn: Binding(get: { selection.shown.contains(computer.id) }, set: { _ in update(selection.toggling(computer.id)) })) {
                    Label { Text(computerTitle(computer)) } icon: { ComputerBadge(computer, size: 28) }
                }
                .menuActionDismissBehavior(.disabled)
            }
            if accounts.computers.count > 1 {
                Menu(L10n.tr("Show only")) {
                    ForEach(accounts.computers) { computer in
                        Button { update(selection.only(computer.id)) } label: {
                            Label { Text(computer.name) } icon: { ComputerBadge(computer, size: 28) }
                        }
                    }
                }
            }
            Divider()
            Button(L10n.tr("Reconnect"), systemImage: "arrow.clockwise", action: reconnect)
                .disabled(stores.isEmpty)
            Button(L10n.tr("Manage computers"), systemImage: "desktopcomputer", action: manage)
        } label: { label }
        .accessibilityLabel("Computers, \(summary). Filter conversations")
    }

    private var label: some View {
        HStack(spacing: 4) {
            if compact {
                Image(systemName: "desktopcomputer")
            } else {
                Text(Self.phrase(summary)).lineLimit(1).contentTransition(.opacity)
            }
            Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold))
        }
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(Palette.secondary)
        .help(Self.phrase(summary))
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: summary)
    }

    /// "3 connected" and the other summaries, in the app's language.
    private static func phrase(_ raw: String) -> String {
        let parts = raw.split(separator: " ", maxSplits: 1).map(String.init)
        if parts.count == 2, let count = Int(parts[0]) {
            if parts[1] == "connected" { return String(format: L10n.tr("%d connected"), count) }
            if parts[1] == "offline" { return String(format: L10n.tr("%d offline"), count) }
        }
        if parts.count == 2, parts[1] == "connected" {
            let ends = parts[0].split(separator: "/").map(String.init)
            if ends.count == 2, let online = Int(ends[0]), let total = Int(ends[1]) {
                return String(format: L10n.tr("%d/%d connected"), online, total)
            }
        }
        return L10n.tr(raw)
    }

    private func computerTitle(_ computer: Computer) -> String {
        guard let store = accounts.store(for: computer.id) else { return computer.name }
        return "\(store.hostName) · \(L10n.tr(store.connectionLabel))"
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
