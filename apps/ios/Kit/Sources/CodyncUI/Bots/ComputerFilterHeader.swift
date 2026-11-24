import CodyncKit
import SwiftUI

/// A list's filter button (the desktop list header's): the list's own layout `options` first (each keeps the menu open:
/// `ListLayout.option`), then the computers to show, each with its connection, Reconnect and Manage computers.
public struct ComputerFilterHeader<Options: View>: View {
    let accounts: AccountStore
    @Binding var hidden: String
    let manage: () -> Void
    let options: Options
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(accounts: AccountStore, hidden: Binding<String>, manage: @escaping () -> Void, @ViewBuilder options: () -> Options) {
        self.accounts = accounts
        _hidden = hidden
        self.manage = manage
        self.options = options()
    }

    private var selection: ComputerSelection {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hidden)
    }

    private var stores: [BotStore] { selection.shown.compactMap { accounts.store(for: $0) } }
    private var summary: String { accounts.connectionSummary(hidden: hidden) }

    public var body: some View {
        Menu {
            options
            Divider()
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

    private func computerTitle(_ computer: Computer) -> String {
        guard let store = accounts.store(for: computer.id) else { return computer.name }
        return "\(store.hostName) · \(L10n.tr(store.connectionLabel))"
    }

    private func update(_ value: String) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { hidden = value }
    }

    private func reconnect() { for store in stores { store.restartStream() } }
}

public extension AccountStore {
    /// How the computers `hidden` leaves shown are connected ("3 connected", "Connecting…"), in the app's language:
    /// the quiet line a list shows under its title.
    func connectionSummary(hidden: String) -> String {
        let stores = ComputerSelection(all: computers.map(\.id), hidden: hidden).shown.compactMap { store(for: $0) }
        return phrase(ConnectionSummary(connections: stores.map(\.shownConnection), waking: stores.contains(where: \.isWaking)).text)
    }
}

/// "3 connected" and the other summaries, in the app's language.
private func phrase(_ raw: String) -> String {
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

/// How the lists are laid out on this device (their filter menu's options, the desktop's prefs): the keys each list
/// keeps them under, and the menu's toggles for them.
public enum ListLayout {
    public static let botsByComputer = "botsByComputer"
    public static let botsByStatus = "botsByStatus"
    public static let threadsGroup = "threadsGroup"
    public static let threadsByStatus = "threadsByStatus"
    public static let threadsByActivity = "threadsByActivity"

    /// One of a list's options in its filter menu, which stays open when it's toggled.
    @MainActor public static func option(_ title: String, _ isOn: Binding<Bool>) -> some View {
        Toggle(L10n.tr(title), isOn: isOn).menuActionDismissBehavior(.disabled)
    }

    /// `value`, reporting a change (`list_layout_changed`) and animating the list to it.
    @MainActor public static func tracked(_ value: Binding<Bool>, _ list: AnalyticsEvent.ListName, _ option: AnalyticsEvent.ListOption,
                               reduceMotion: Bool) -> Binding<Bool> {
        Binding(get: { value.wrappedValue }, set: { on in
            Analytics.shared.capture(.listLayoutChanged(list, option: option, on: on))
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { value.wrappedValue = on }
        })
    }
}
