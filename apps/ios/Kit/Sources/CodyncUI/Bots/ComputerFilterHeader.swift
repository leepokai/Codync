import CodyncKit
import SwiftUI

/// The roster's filter button (the desktop list header's): the computers to show, each with its connection, Reconnect
/// and Manage computers. `summary` is the quiet line the list shows under its title.
public struct ComputerFilterHeader: View {
    let accounts: AccountStore
    @Binding var hidden: String
    let manage: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(accounts: AccountStore, hidden: Binding<String>, manage: @escaping () -> Void) {
        self.accounts = accounts
        _hidden = hidden
        self.manage = manage
    }

    /// How the shown computers are connected ("3 connected", "Connecting…"), in the app's language.
    public static func summary(accounts: AccountStore, hidden: String) -> String {
        let stores = ComputerSelection(all: accounts.computers.map(\.id), hidden: hidden).shown.compactMap { accounts.store(for: $0) }
        return phrase(ConnectionSummary(connections: stores.map(\.shownConnection), waking: stores.contains(where: \.isWaking)).text)
    }

    private var selection: ComputerSelection {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hidden)
    }

    private var stores: [BotStore] { selection.shown.compactMap { accounts.store(for: $0) } }
    private var summary: String { Self.summary(accounts: accounts, hidden: hidden) }

    public var body: some View {
        Menu {
            Toggle(isOn: Binding(get: { selection.shown.count == accounts.computers.count }, set: { _ in update("") })) {
                Label(L10n.tr("All computers"), systemImage: "desktopcomputer")
            }
            .menuActionDismissBehavior(.disabled)
            ForEach(accounts.computers) { computer in
                Toggle(isOn: Binding(get: { selection.shown.contains(computer.id) }, set: { _ in update(selection.toggling(computer.id)) })) {
                    Label { Text(computerTitle(computer)) } icon: { ComputerBadge(computer, size: 28) }
                }
                .menuActionDismissBehavior(.disabled)
            }
            Divider()
            Button(L10n.tr("Reconnect"), systemImage: "arrow.clockwise", action: reconnect)
                .disabled(stores.isEmpty)
            Button(L10n.tr("Manage computers"), systemImage: "desktopcomputer", action: manage)
        } label: { label }
        .accessibilityLabel("Computers, \(summary). Filter conversations")
    }

    /// The filter symbol, filled while a computer is left out.
    private var label: some View {
        Label(L10n.tr("Computers"), systemImage: selection.shown.count < accounts.computers.count
            ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease")
            .help(summary)
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
        if isWaking { return "Waking the computer…" }
        if isAsleep { return "Sleeping" }
        return switch shownConnection {
        case .online where isCloudUpdating: "Updating…"
        case .online where mismatch != nil: "Needs update"
        case .online: "Connected"
        case .connecting: "Connecting…"
        case .computerOffline, .offline: "Offline"
        case .unauthorized: "No access"
        case .unpaired: "Not paired"
        }
    }
}
