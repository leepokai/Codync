import CodyncKit
import CodyncUI
import SwiftUI

/// Computer management: every computer this iPhone can use or ask for — paired here and in the
/// account, merged by computer ID — and the way to pair another.
struct ComputersView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @Environment(\.dismissModal) private var dismissModal
    @State private var addingComputer = false
    @State private var confirmForget: Computer?
    @State private var confirmRevoke: CloudComputer?
    @State private var access: AccessTarget?

    var body: some View {
        // One card per computer, so each one's rows read as its own.
        CardForm {
            ForEach(accounts.computers) { computer in
                if let store = accounts.store(for: computer.id) {
                    CardSection {
                        ComputerRows(store: store, inAccount: cloudComputer(computer.id),
                                     openScreen: { closeSheet { store.screenRequest = ScreenRequest() } },
                                     openMarketplace: { closeSheet { app.marketplace = store.computer.id } },
                                     ask: cloudComputer(computer.id).map { c in { access = AccessTarget(computer: c) } },
                                     revoke: cloudComputer(computer.id).flatMap { c in c.isGranted ? { confirmRevoke = c } : nil },
                                     remove: { confirmForget = computer })
                    }
                }
            }
            ForEach(accountOnly) { computer in
                CardSection {
                    if accounts.isOlderCopy(computer) {
                        OlderCopyRow(computer: computer)
                    } else {
                        AccountComputerRow(computer: computer, ticket: accounts.pendingAccess[computer.computerId],
                                           ask: { access = AccessTarget(computer: computer) },
                                           revoke: { confirmRevoke = computer })
                    }
                }
            }
            if !hiddenBots.isEmpty {
                CardSection("Hidden bots") {
                    ForEach(hiddenBots) { item in
                        let store = item.store, bot = item.bot
                        HStack {
                            CharacterAvatar(bot: bot, size: 28, animated: false)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(bot.name)
                                Text(store.hostName).font(.caption).foregroundStyle(Palette.tertiary)
                            }
                            Spacer()
                            IconButton("Unhide", systemImage: "eye") {
                                withAnimation(Motion.layout) { store.setHidden(bot, false) }
                            }
                        }
                    }
                }
            }
            CardSection(footer: app.account.isSignedIn
                        ? "Computers in your account need your OK on the computer before this iPhone can use them."
                        : "Each bot runs on its own computer. Sign in to see the computers in your account.") {
                Button {
                    addingComputer = true
                } label: {
                    Label(L10n.tr("Pair a computer"), systemImage: "qrcode.viewfinder")
                        .foregroundStyle(Palette.text)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .refreshable { await accounts.refreshCloud() }
        .page("Computer management", icon: "gearshape")
        .tracked(.screenViewed(.computers))
        .task { await accounts.refreshCloud() }
        .codyncSheet(isPresented: $addingComputer) {
            PairingView(inModal: true)
        }
        .codyncSheet(item: $access) { target in
            AccessRequestView(computer: target.computer, pending: accounts.pendingAccess[target.id])
        }
        .codyncDialog(L10n.tr("Remove {name}?").replacingOccurrences(of: "{name}", with: confirmForget?.name ?? "computer"),
                      isPresented: Binding(get: { confirmForget != nil }, set: { if !$0 { confirmForget = nil } }),
                      message: "Its bots and conversations stay on that computer. You can pair again any time.") {
            [DialogAction("Remove", destructive: true) {
                if let c = confirmForget { Task { await app.removeComputer(c.id) } }
            }]
        }
        .codyncDialog("Revoke this iPhone's access to \(confirmRevoke?.name ?? "the computer")?",
                      isPresented: Binding(get: { confirmRevoke != nil }, set: { if !$0 { confirmRevoke = nil } }),
                      message: "You can ask for access again; the computer will show a new code to confirm.") {
            [DialogAction("Revoke", destructive: true) {
                if let c = confirmRevoke { Task { await app.revokeAccess(c.computerId) } }
            }]
        }
        // What failed here (removing, revoking, refreshing) shows over this sheet, not under it at the root,
        // where the same error clears with this one. In place: a failure right after a confirmation comes
        // while that dialog's cover is still fading out, and a second cover can't present over it.
        .codyncDialog("Something went wrong", isPresented: Binding(
            get: { accounts.lastError != nil }, set: { if !$0 { accounts.lastError = nil } }
        ), message: accounts.lastError, cancel: "OK", inPlace: true) { [] }
    }

    private struct AccessTarget: Identifiable {
        let computer: CloudComputer
        var id: ComputerID { computer.computerId }
    }

    private func cloudComputer(_ id: ComputerID) -> CloudComputer? {
        accounts.cloudComputers.first { $0.computerId == id }
    }

    /// Account computers this iPhone isn't set up for yet.
    private var accountOnly: [CloudComputer] {
        accounts.cloudComputers.filter { c in !accounts.computers.contains { $0.id == c.computerId } }
    }

    private struct HiddenBot: Identifiable {
        let store: BotStore
        let bot: Bot
        let id: String
    }

    private var hiddenBots: [HiddenBot] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }.flatMap { store in
            store.hiddenBots.map { HiddenBot(store: store, bot: $0, id: "\(store.computer.id)/\($0.id)") }
        }
    }

    /// The marketplace and screen open over the app, once this sheet has slid away.
    private func closeSheet(then open: @escaping @MainActor () -> Void) {
        dismissModal()
        Task {
            try? await Task.sleep(for: .milliseconds(450))
            open()
        }
    }
}

/// A computer this iPhone can use, as the rows of its card: its connection and whether it's in the account,
/// then Extensions, Voice chat (its keys live on the computer), Screen, whether it sends push alerts here,
/// how it's reached, the Codync it runs (and its update) and, for a stale pairing, Remove.
private struct ComputerRows: View {
    let store: BotStore
    let inAccount: CloudComputer?
    let openScreen: () -> Void
    /// Agents, connectors and skills installed on this computer.
    let openMarketplace: () -> Void
    let ask: (() -> Void)?
    /// Set when this iPhone's access to the computer can be revoked.
    let revoke: (() -> Void)?
    let remove: () -> Void
    @Environment(AccountStore.self) private var accounts
    @State private var coloring = false
    @State private var voiceSettings = false
    @State private var networkSheet = false

    var body: some View {
        summary
        if store.connection == .online {
            Button(action: openMarketplace) { RowLabel("Extensions", systemImage: "square.grid.2x2", chevron: true) }
                .buttonStyle(.plain)
            Button { voiceSettings = true } label: { RowLabel("Voice chat", systemImage: "waveform", chevron: true) }
                .buttonStyle(.plain)
            if store.screen != nil {
                Button(action: openScreen) { RowLabel("Screen", systemImage: "display", chevron: true) }
                    .buttonStyle(.plain)
            }
            if showsNetwork {
                Button { networkSheet = true } label: { RowLabel("Network", systemImage: Self.networkIcon, chevron: true) }
                    .buttonStyle(.plain)
            }
        }
        ComputerNotificationsRow(store: store)
        connection
        // Kept while an update asked from here restarts the computer: its spinner says it's updating.
        if store.connection == .online || store.isUpdating, store.hello != nil {
            CodyncVersionRow(store: store)
        }
        if accounts.isStale(store.computer.id), inAccount == nil {
            Button(action: remove) { RowLabel("Remove", systemImage: "trash", tint: Palette.danger) }
                .buttonStyle(.plain)
        }
    }

    /// Name, connection, and an access request when this iPhone can't reach the computer.
    private var summary: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                ComputerBadge(store.computer, size: 32)
                Text(store.hostName).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if inAccount != nil {
                    Image(systemName: "person.crop.circle.badge.checkmark")
                        .foregroundStyle(Palette.tertiary)
                        .accessibilityLabel(L10n.tr("In your account"))
                }
            }
            connectionDetail
            if store.isOffline, !store.isUpdating, let ask {
                Button(accounts.pendingAccess[store.computer.id] == nil ? "Ask for access" : "Show code", action: ask)
                    .buttonStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .contextActions {
            var items = [
                MenuItem("Color", icon: "paintpalette") { coloring = true },
            ]
            if store.connection == .online {
                items.append(MenuItem("Extensions", icon: "square.grid.2x2", action: openMarketplace))
                items.append(MenuItem("Voice chat", icon: "waveform") { voiceSettings = true })
            }
            if store.screen != nil, store.connection == .online {
                items.append(MenuItem("Screen", icon: "display", action: openScreen))
            }
            if store.connection == .online, showsNetwork {
                items.append(MenuItem("Network", icon: Self.networkIcon) { networkSheet = true })
            }
            if let revoke {
                items.append(MenuItem("Revoke this iPhone's access", icon: "lock.slash", destructive: true, divider: true, action: revoke))
            }
            if inAccount == nil {
                items.append(MenuItem("Remove", icon: "trash", destructive: true, divider: revoke == nil, action: remove))
            }
            return items
        }
        .codyncSheet(isPresented: $voiceSettings) { VoiceChatSettingsView().environment(store) }
        .codyncSheet(isPresented: $networkSheet) { NetworkSheet().environment(store) }
        .codyncOverlay(isPresented: $coloring) { close in
            ZStack {
                Color.black.opacity(0.35).ignoresSafeArea().onTapGesture(perform: close)
                SwatchPanel(selected: store.computer.color) { id in
                    close()
                    accounts.setColor(store.computer.id, id)
                }
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .shadow(color: .black.opacity(0.25), radius: 24, y: 10)
            }
            .accessibilityAddTraits(.isModal)
        }
    }

    /// How this iPhone reaches the computer: the short name on the row, the whole route in the menu.
    private var connection: some View {
        DropdownMenu {
            ConnectionRoute.allCases.map { route in
                MenuItem(RelayCopy.routeTitle(route, cloud: store.computer.cloud), icon: route.icon, selected: route == preference) {
                    accounts.setRoute(store.computer.id, route)
                }
            }
        } label: {
            HStack(spacing: 12) {
                Image(systemName: preference.icon).font(.system(size: 17)).frame(width: 26)
                Text(L10n.tr("Connection"))
                Spacer(minLength: 8)
                Text(L10n.tr(RelayCopy.routeShortTitle(preference, cloud: store.computer.cloud)))
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
            .foregroundStyle(Palette.text)
            .contentShape(Rectangle())
        }
    }

    private var connectionDetail: some View {
        Text(L10n.tr(detail)).font(.footnote)
            .foregroundStyle(store.isOffline && !store.isUpdating ? Palette.warning : Palette.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var preference: ConnectionRoute { store.computer.route ?? .automatic }

    private static let networkIcon = "point.3.connected.trianglepath.dotted"

    /// In a network of the user's computers, or cloud assistants on (hosts before it say nothing).
    private var showsNetwork: Bool { store.network?.hasContent == true }

    private var detail: String {
        guard store.connection == .online else { return accounts.statusText(store) }
        let count = store.roster.count
        let route = RelayCopy.detailSuffix(route: store.hostRoute, cloud: store.computer.cloud)
        return "\(count) bot\(count == 1 ? "" : "s")\(route)"
    }
}

/// The Codync a computer runs, and its update when one is due (`HostVersionLine`).
private struct CodyncVersionRow: View {
    let store: BotStore
    /// Moved on when an update asked from here runs out of time, so one that never finished is offered again.
    @State private var now = Date.now

    var body: some View {
        if let line = store.hostVersionLine(now: max(now, .now)) {
            HStack(spacing: 12) {
                Image(systemName: "arrow.down.circle").font(.system(size: 17)).frame(width: 26)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 8) {
                        Text(verbatim: "Codync")
                        Text(verbatim: line.version).foregroundStyle(Palette.secondary).monospacedDigit()
                    }
                    if !line.detail.isEmpty {
                        Text(line.detail.map { L10n.tr($0) }.joined(separator: " "))
                            .font(.footnote)
                            .foregroundStyle(line.warns ? Palette.warning : Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                switch line.accessory {
                case .updating: Spinner(size: 16)
                case let .button(title): Button(L10n.tr(title), action: store.requestHostUpdate).buttonStyle(.primary)
                case nil: EmptyView()
                }
            }
            .foregroundStyle(Palette.text)
            .animation(Motion.layout, value: line)
            .task(id: store.hostUpdateWaitEnds) {
                guard let ends = store.hostUpdateWaitEnds else { return }
                try? await Task.sleep(for: .seconds(ends.timeIntervalSinceNow))
                now = .now
            }
        }
    }
}

/// A computer in the account that this iPhone can't use yet.
private struct AccountComputerRow: View {
    let computer: CloudComputer
    let ticket: AccessTicket?
    let ask: () -> Void
    let revoke: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 40)
                .opacity(0.5)
            VStack(alignment: .leading, spacing: 2) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(2)
            }
            Spacer()
            if ticket != nil {
                Button(L10n.tr("Code"), action: ask).buttonStyle(.secondary)
            } else if computer.isGranted {
                Button(L10n.tr("Revoke"), role: .destructive, action: revoke).buttonStyle(.secondary)
            } else {
                Button(L10n.tr("Ask for access"), action: ask).buttonStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }

    private var detail: String {
        let online = computer.isOnline ? "Online" : "Offline"
        if let ticket { return "Waiting for your OK on the computer · \(ticket.code)" }
        return switch computer.access {
        case "granted": "Approved for an earlier install of Codync. Revoke it, then ask again."
        case "pending": "\(online) · A request is waiting"
        default: "\(online) · In your account"
        }
    }
}

/// An older account identity stays visible; only account management can remove it.
private struct OlderCopyRow: View {
    let computer: CloudComputer

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 40)
                .opacity(0.35)
            VStack(alignment: .leading, spacing: 2) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.secondary)
                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.tertiary).lineLimit(2)
            }
            Spacer()
        }
        .padding(.vertical, 2)
    }

    private var detail: String {
        let seen = computer.lastSeenAt.map { " · last seen \(RelativeTime.day(Date(milliseconds: $0)))" } ?? ""
        return "Older copy of this computer\(seen)"
    }
}

/// The computer color choices: a grid of colored dots, the current one checked.
private struct SwatchPanel: View {
    let selected: String?
    let pick: (String) -> Void

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(36), spacing: 10), count: 6), spacing: 10) {
            ForEach(AvatarPalette.colors) { swatch in
                Button { pick(swatch.id) } label: {
                    Circle().fill(swatch.color)
                        .frame(width: 32, height: 32)
                        .overlay {
                            if selected == swatch.id {
                                Image(systemName: "checkmark").font(.caption.weight(.bold)).foregroundStyle(.white)
                            }
                        }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(swatch.label)
                .accessibilityAddTraits(selected == swatch.id ? .isSelected : [])
            }
        }
        .padding(14)
    }
}

private extension ConnectionRoute {
    var icon: String {
        switch self {
        case .automatic: "arrow.triangle.branch"
        case .cloudflareFirst: "cloud"
        case .directOnly: "wifi"
        }
    }
}

/// System permission gates the computer preference without changing its saved value.
private struct ComputerNotificationsRow: View {
    let store: BotStore
    @Environment(AppStore.self) private var app
    @Environment(\.scenePhase) private var scenePhase
    @State private var allowed: Bool?
    @State private var checked = false

    var body: some View {
        Group {
            if allowed == true {
                Toggle(isOn: Binding(get: { store.computer.notifications ?? true }, set: { app.setNotifications(store.computer.id, $0) })) {
                    RowLabel("Push notifications", systemImage: "bell")
                }
                .toggleStyle(.codync)
            } else {
                Button {
                    if allowed == false {
                        UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
                    } else {
                        Task {
                            allowed = await PushRegistrar.shared.requestAuthorization(from: .settings)
                        }
                    }
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        RowLabel("Push notifications", systemImage: "bell", chevron: true)
                        Text(L10n.tr("Turn on notifications in Settings"))
                            .font(.footnote).foregroundStyle(Palette.secondary)
                    }
                }
                .buttonStyle(.plain)
                .disabled(!checked)
            }
        }
        .animation(Motion.layout, value: allowed)
        .task { await refresh() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await refresh() } }
        }
    }

    private func refresh() async {
        allowed = await PushRegistrar.shared.isAllowed()
        checked = true
    }
}
