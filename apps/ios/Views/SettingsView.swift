import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications

/// Every computer this iPhone can use or ask for — paired here and in the account, merged by
/// computer ID — plus settings for the selected account.
struct SettingsView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @Environment(\.dismiss) private var dismiss
    @State private var notificationsAllowed: Bool?
    @State private var addingComputer = false
    @State private var confirmForget: Computer?
    @State private var confirmRevoke: CloudComputer?
    @State private var access: AccessTarget?

    var body: some View {
        Form {
            Section {
                ForEach(accounts.computers) { computer in
                    if let store = accounts.store(for: computer.id) {
                        ComputerRow(store: store, inAccount: cloudComputer(computer.id))
                            .swipeActions {
                                Button("Remove", systemImage: "trash", role: .destructive) { confirmForget = computer }
                            }
                            .contextMenu { menu(for: computer, store: store) }
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
                }
            } header: {
                Text("Computers")
            } footer: {
                Text(app.account.isSignedIn
                     ? "Computers in your account need your OK on the computer before this iPhone can use them."
                     : "Each bot runs on its own computer. Sign in to see the computers in your account.")
            }

            Section {
                ForEach(onlineStores, id: \.computer.id) { store in
                    Button {
                        closeSheets()
                        app.marketplace = store.computer.id
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(onlineStores.count > 1 ? "Marketplace on \(store.hostName)" : "Marketplace").foregroundStyle(Palette.text)
                            Text("Agents, connectors and skills for your bots").font(.subheadline).foregroundStyle(Palette.secondary)
                        }
                    }
                }
            }

            Section {
                NavigationLink {
                    WidgetGalleryView()
                } label: {
                    Label("Widgets", systemImage: "square.grid.2x2")
                }
                notificationsRow
            }

            if !hiddenBots.isEmpty {
                Section("Hidden bots") {
                    ForEach(hiddenBots) { item in
                        let store = item.store, bot = item.bot
                        HStack {
                            CharacterAvatar(bot: bot, size: 28, animated: false)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(bot.name)
                                Text(store.hostName).font(.caption).foregroundStyle(Palette.tertiary)
                            }
                            Spacer()
                            Button("Unhide", systemImage: "eye") { store.setHidden(bot, false) }.labelStyle(.iconOnly)
                        }
                    }
                }
            }

            Section {
                LabeledContent("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                Link("Source code", destination: URL(string: "https://github.com/leepokai/Codync")!)
                    .foregroundStyle(Palette.text)
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle("Computers & settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        .refreshable { await accounts.refreshCloud() }
        .task { await accounts.refreshCloud() }
        .sheet(isPresented: $addingComputer) {
            PairingView(introductory: false) { addingComputer = false }
        }
        .sheet(item: $access) { target in
            NavigationStack { AccessRequestView(computer: target.computer, pending: accounts.pendingAccess[target.id] != nil) }
        }
        .confirmationDialog("Remove \(confirmForget?.name ?? "computer")?", isPresented: Binding(get: { confirmForget != nil }, set: { if !$0 { confirmForget = nil } }), titleVisibility: .visible) {
            Button("Remove", role: .destructive) {
                if let c = confirmForget { accounts.forget(c.id) }
            }
        } message: {
            Text("Its bots and conversations stay on that computer. You can pair again any time.")
        }
        .confirmationDialog("Revoke this iPhone's access to \(confirmRevoke?.name ?? "the computer")?", isPresented: Binding(get: { confirmRevoke != nil }, set: { if !$0 { confirmRevoke = nil } }), titleVisibility: .visible) {
            Button("Revoke", role: .destructive) {
                if let c = confirmRevoke { Task { await app.revokeAccess(c.computerId) } }
            }
        } message: {
            Text("You can ask for access again; the computer will show a new code to confirm.")
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

    @ViewBuilder private func menu(for computer: Computer, store: BotStore) -> some View {
        Menu("Color", systemImage: "paintpalette") {
            ForEach(AvatarPalette.colors) { swatch in
                Button { accounts.setColor(computer.id, swatch.id) } label: {
                    // Menus drop SwiftUI tints; an original-mode UIImage keeps the swatch colored.
                    Label {
                        Text(swatch.label)
                    } icon: {
                        Image(uiImage: UIImage(systemName: computer.color == swatch.id ? "checkmark.circle.fill" : "circle.fill")!
                            .withTintColor(UIColor(swatch.color), renderingMode: .alwaysOriginal))
                    }
                }
            }
        }
        if store.screen != nil, store.connection == .online {
            Button("Screen", systemImage: "display") { openScreen(store) }
        }
        if let cloud = cloudComputer(computer.id), cloud.access == "granted" {
            Button("Revoke this iPhone's access", systemImage: "lock.slash", role: .destructive) { confirmRevoke = cloud }
        }
        Button("Remove", systemImage: "trash", role: .destructive) { confirmForget = computer }
    }

    /// This screen is a sheet of its own or inside Accounts; the marketplace and screen open over the app.
    private func closeSheets() {
        app.showComputers = false
        app.account.showSwitcher = false
    }

    private func openScreen(_ store: BotStore) {
        closeSheets()
        Task {
            // Let the sheet finish closing before covering the screen.
            try? await Task.sleep(for: .milliseconds(450))
            store.screenRequest = ScreenRequest()
        }
    }

    @ViewBuilder private var notificationsRow: some View {
        if notificationsAllowed == true {
            LabeledContent("Notifications", value: "On")
        } else {
            Button {
                if notificationsAllowed == false {
                    UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
                } else {
                    Task { notificationsAllowed = await PushRegistrar.shared.requestAuthorization() }
                }
            } label: {
                LabeledContent("Notifications", value: notificationsAllowed == false ? "Off in Settings" : "Turn on")
                    .foregroundStyle(Palette.text)
            }
        }
    }
}

/// A computer this iPhone can use: its connection, how it's reached, and whether it's in the account.
private struct ComputerRow: View {
    let store: BotStore
    let inAccount: CloudComputer?

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
            if inAccount != nil {
                Image(systemName: "person.crop.circle.badge.checkmark")
                    .foregroundStyle(Palette.tertiary)
                    .accessibilityLabel("In your account")
            }
        }
        .padding(.vertical, 2)
    }

    private var detail: String {
        guard store.connection == .online else { return store.statusText }
        let count = store.roster.count
        let route = switch store.hostRoute {
        case .direct: " · Direct"
        case .relay: " · Relay"
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
                Button("Code", action: ask).buttonStyle(.bordered)
            } else if computer.access == "granted" {
                Button("Revoke", role: .destructive, action: revoke).buttonStyle(.bordered)
            } else {
                Button("Ask for access", action: ask).buttonStyle(.bordered)
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
