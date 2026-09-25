import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications

/// The profile sheet behind the top-left button: which computer you're talking
/// to (and switching between them), notifications, hidden bots.
struct SettingsView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var notificationsAllowed: Bool?
    @State private var addingComputer = false
    @State private var confirmForget: Pairing?

    var body: some View {
        Form {
            Section {
                ForEach(model.computers, id: \.token) { computer in
                    ComputerRow(computer: computer, active: computer.token == model.pairing?.token)
                        .contentShape(Rectangle())
                        .onTapGesture { if computer.token != model.pairing?.token { model.pair(computer) } }
                        .swipeActions {
                            Button("Remove", systemImage: "trash", role: .destructive) { confirmForget = computer }
                        }
                        .contextMenu {
                            Menu("Color", systemImage: "paintpalette") {
                                ForEach(AvatarPalette.colors) { swatch in
                                    Button { model.setColor(computer, swatch.id) } label: {
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
                            Button("Remove", systemImage: "trash", role: .destructive) { confirmForget = computer }
                        }
                }
                Button {
                    addingComputer = true
                } label: {
                    Label("Add a computer", systemImage: "plus")
                        .foregroundStyle(Palette.text)
                }
            } footer: {
                Text("Each computer has its own bots. Tap one to switch.")
            }

            Section {
                NavigationLink {
                    MarketplaceView()
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Marketplace")
                        Text("Agents, connectors and skills for your bots").font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                }
            }

            Section {
                notificationsRow
            }

            if !model.hiddenBots.isEmpty {
                Section("Hidden bots") {
                    ForEach(model.hiddenBots) { bot in
                        HStack {
                            CharacterAvatar(bot: bot, size: 28, animated: false)
                            Text(bot.name)
                            Spacer()
                            Button("Unhide", systemImage: "eye") { model.setHidden(bot, false) }.labelStyle(.iconOnly)
                        }
                    }
                }
            }

            Section {
                if let hello = model.hello {
                    LabeledContent("Host version", value: hello.version)
                }
                LabeledContent("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                Link("Source code", destination: URL(string: "https://github.com/leepokai/Codync")!)
                    .foregroundStyle(Palette.text)
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        .sheet(isPresented: $addingComputer) {
            PairingView { addingComputer = false }
        }
        .confirmationDialog("Remove \(confirmForget?.name ?? "computer")?", isPresented: Binding(get: { confirmForget != nil }, set: { if !$0 { confirmForget = nil } }), titleVisibility: .visible) {
            Button("Remove", role: .destructive) {
                if let c = confirmForget { model.forget(c) }
                if model.pairing == nil { dismiss() }
            }
        } message: {
            Text("Its bots and conversations stay on that computer. You can pair again any time.")
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

private struct ComputerRow: View {
    let computer: Pairing
    let active: Bool
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(computer, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Text(status).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            Spacer()
            if !active || model.screen != nil {
                Button("Screen", systemImage: "display") { openScreen() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(Palette.text)
            }
            if active {
                Image(systemName: "checkmark").font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            }
        }
        .padding(.vertical, 2)
    }

    /// Only the active computer is connected: switch to this one first, then open its screen.
    private func openScreen() {
        if !active { model.pair(computer) }
        model.showProfile = false
        Task {
            // Let the sheet finish closing before covering the screen.
            try? await Task.sleep(for: .milliseconds(450))
            model.screenRequest = ScreenRequest()
        }
    }

    private var status: String {
        guard active else { return "\(computer.urls.count) address\(computer.urls.count == 1 ? "" : "es")" }
        return switch model.connection {
        case .online: "Connected · \(model.bots.count) bot\(model.bots.count == 1 ? "" : "s")"
        case .connecting: "Connecting…"
        case .offline: "Offline"
        case .unpaired: "Not paired"
        }
    }
}
