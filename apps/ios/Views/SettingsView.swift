import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications

/// Every computer this iPhone can use or ask for — paired here and in the account, merged by
/// computer ID — plus settings for the selected account.
struct SettingsView: View {
    /// Pushed inside Accounts rather than opened as its own sheet.
    var pushed = false
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @Environment(\.dismissModal) private var dismissModal
    @Environment(\.openURL) private var openURL
    @State private var page: Page?
    @State private var notificationsAllowed: Bool?
    @State private var addingComputer = false
    @State private var confirmForget: Computer?
    @State private var confirmRevoke: CloudComputer?
    @State private var access: AccessTarget?

    var body: some View {
        CardForm {
            CardSection("Computers", footer: app.account.isSignedIn
                        ? "Computers in your account need your OK on the computer before this iPhone can use them."
                        : "Each bot runs on its own computer. Sign in to see the computers in your account.") {
                ForEach(accounts.computers) { computer in
                    if let store = accounts.store(for: computer.id) {
                        ComputerRow(store: store, inAccount: cloudComputer(computer.id),
                                    openScreen: { openScreen(store) },
                                    revoke: cloudComputer(computer.id).flatMap { c in c.access == "granted" ? { confirmRevoke = c } : nil },
                                    remove: { confirmForget = computer })
                    }
                }
                ForEach(accountOnly) { computer in
                    AccountComputerRow(computer: computer, ticket: accounts.pendingAccess[computer.computerId],
                                       ask: { access = AccessTarget(computer: computer) },
                                       revoke: { confirmRevoke = computer })
                }
                Button {
                    addingComputer = true
                } label: {
                    Label("Pair a computer", systemImage: "qrcode.viewfinder")
                        .foregroundStyle(Palette.text)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }

            if !onlineStores.isEmpty {
                CardSection {
                    ForEach(onlineStores, id: \.computer.id) { store in
                        Button {
                            let id = store.computer.id
                            closeSheets { app.marketplace = id }
                        } label: {
                            LinkRow {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(onlineStores.count > 1 ? "Marketplace on \(store.hostName)" : "Marketplace")
                                    Text("Agents, connectors and skills for your bots").font(.subheadline).foregroundStyle(Palette.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            CardSection {
                Button { page = .widgets } label: {
                    LinkRow { Label("Widgets", systemImage: "square.grid.2x2") }
                }
                .buttonStyle(.plain)
                Button { page = .activity } label: {
                    LinkRow { Label("Live Activity & Dynamic Island", systemImage: "waveform") }
                }
                .buttonStyle(.plain)
                notificationsRow
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

            CardSection {
                ValueRow("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                Button("Source code") { openURL(URL(string: "https://github.com/leepokai/Codync")!) }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.text)
            }
        }
        .refreshable { await accounts.refreshCloud() }
        .page("Computers & settings", pushed: pushed)
        .navigationDestination(item: $page) { page in
            switch page {
            case .widgets: WidgetGalleryView(pushed: true)
            case .activity: ActivityGalleryView()
            }
        }
        .task { await accounts.refreshCloud() }
        .codyncSheet(isPresented: $addingComputer) {
            PairingView(inModal: true)
        }
        .codyncSheet(item: $access) { target in
            AccessRequestView(computer: target.computer, pending: accounts.pendingAccess[target.id] != nil)
        }
        .codyncDialog("Remove \(confirmForget?.name ?? "computer")?",
                      isPresented: Binding(get: { confirmForget != nil }, set: { if !$0 { confirmForget = nil } }),
                      message: "Its bots and conversations stay on that computer. You can pair again any time.") {
            [DialogAction("Remove", destructive: true) {
                if let c = confirmForget { accounts.forget(c.id) }
            }]
        }
        .codyncDialog("Revoke this iPhone's access to \(confirmRevoke?.name ?? "the computer")?",
                      isPresented: Binding(get: { confirmRevoke != nil }, set: { if !$0 { confirmRevoke = nil } }),
                      message: "You can ask for access again; the computer will show a new code to confirm.") {
            [DialogAction("Revoke", destructive: true) {
                if let c = confirmRevoke { Task { await app.revokeAccess(c.computerId) } }
            }]
        }
        .task {
            let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
            notificationsAllowed = switch status {
            case .authorized, .provisional, .ephemeral: true
            case .denied: false
            default: nil
            }
        }
    }

    private enum Page: Hashable { case widgets, activity }

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

    private var onlineStores: [BotStore] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }.filter { $0.connection == .online }
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

    /// This screen is a sheet of its own or inside Accounts; the marketplace and screen open over the app,
    /// once that sheet has slid away.
    private func closeSheets(then open: @escaping @MainActor () -> Void) {
        dismissModal()
        Task {
            try? await Task.sleep(for: .milliseconds(450))
            open()
        }
    }

    private func openScreen(_ store: BotStore) {
        closeSheets { store.screenRequest = ScreenRequest() }
    }

    @ViewBuilder private var notificationsRow: some View {
        if notificationsAllowed == true {
            ValueRow("Notifications", value: "On")
        } else {
            Button {
                if notificationsAllowed == false {
                    UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
                } else {
                    Task { notificationsAllowed = await PushRegistrar.shared.requestAuthorization() }
                }
            } label: {
                ValueRow("Notifications", value: notificationsAllowed == false ? "Off in Settings" : "Turn on")
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }
}

/// A computer this iPhone can use: its connection, how it's reached, and whether it's in the account.
private struct ComputerRow: View {
    let store: BotStore
    let inAccount: CloudComputer?
    let openScreen: () -> Void
    /// Set when this iPhone's access to the computer can be revoked.
    let revoke: (() -> Void)?
    let remove: () -> Void
    @Environment(AccountStore.self) private var accounts
    @State private var coloring = false

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(store.computer, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(store.hostName).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                HStack(spacing: 4) {
                    Text(detail).lineLimit(1)
                    if store.connection == .online { RouteIcon(route: store.hostRoute) }
                }
                .font(.subheadline)
                .foregroundStyle(store.isOffline ? Palette.warning : Palette.secondary)
            }
            Spacer()
            if store.screen != nil, store.connection == .online {
                IconButton("Screen", systemImage: "display", action: openScreen)
            }
            if inAccount != nil {
                Image(systemName: "person.crop.circle.badge.checkmark")
                    .foregroundStyle(Palette.tertiary)
                    .accessibilityLabel("In your account")
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .contextActions {
            var items = [
                MenuItem("Color", icon: "paintpalette") {
                    // ponytail: waits for the menu to fade out before opening the swatches; one overlay at a time.
                    Task {
                        try? await Task.sleep(for: .milliseconds(350))
                        coloring = true
                    }
                },
            ]
            if store.screen != nil, store.connection == .online {
                items.append(MenuItem("Screen", icon: "display", action: openScreen))
            }
            if let revoke {
                items.append(MenuItem("Revoke this iPhone's access", icon: "lock.slash", destructive: true, divider: true, action: revoke))
            }
            items.append(MenuItem("Remove", icon: "trash", destructive: true, divider: revoke == nil, action: remove))
            return items
        }
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

    private var detail: String {
        guard store.connection == .online else { return store.statusText }
        let count = store.roster.count
        let route = switch store.hostRoute {
        case .direct: " · Direct"
        case .relay: " · Cloudflare"
        default: ""
        }
        return "\(count) bot\(count == 1 ? "" : "s")\(route)"
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
                Text(detail).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(2)
            }
            Spacer()
            if ticket != nil {
                Button("Code", action: ask).buttonStyle(.secondary)
            } else if computer.access == "granted" {
                Button("Revoke", role: .destructive, action: revoke).buttonStyle(.secondary)
            } else {
                Button("Ask for access", action: ask).buttonStyle(.secondary)
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

/// A navigation row label with a trailing chevron.
private struct LinkRow<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        HStack {
            content.foregroundStyle(Palette.text)
            Spacer(minLength: 8)
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
        }
        .contentShape(Rectangle())
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
