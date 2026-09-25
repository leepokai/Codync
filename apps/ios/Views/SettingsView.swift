import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications

/// Computer connections and settings for the currently selected account.
struct SettingsView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var notificationsAllowed: Bool?
    @State private var addingComputer = false
    @State private var confirmForget: Pairing?

    var body: some View {
        CardForm {
            CardSection("Computers on this iPhone", footer: "Each bot runs on its own computer. Tap a computer to view its bots.") {
                ForEach(model.computers, id: \.token) { computer in
                    ComputerRow(computer: computer, active: computer.token == model.pairing?.token) { confirmForget = computer }
                }
                Button {
                    addingComputer = true
                } label: {
                    Label("Add a computer", systemImage: "plus")
                        .foregroundStyle(Palette.text)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }

            CardSection {
                NavigationLink {
                    MarketplaceView()
                } label: {
                    LinkRow {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Marketplace")
                            Text("Agents, connectors and skills for your bots").font(.subheadline).foregroundStyle(Palette.secondary)
                        }
                    }
                }
                .buttonStyle(.plain)
            }

            CardSection {
                NavigationLink {
                    WidgetGalleryView()
                } label: {
                    LinkRow { Label("Widgets", systemImage: "square.grid.2x2") }
                }
                .buttonStyle(.plain)
                NavigationLink { ActivityGalleryView() } label: {
                    LinkRow { Label("Live Activity & Dynamic Island", systemImage: "waveform") }
                }
                .buttonStyle(.plain)
                notificationsRow
            }

            if !model.hiddenBots.isEmpty {
                CardSection("Hidden bots") {
                    ForEach(model.hiddenBots) { bot in
                        HStack {
                            CharacterAvatar(bot: bot, size: 28, animated: false)
                            Text(bot.name)
                            Spacer()
                            Button("Unhide", systemImage: "eye") { model.setHidden(bot, false) }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.plain)
                                .foregroundStyle(Palette.text)
                        }
                    }
                }
            }

            CardSection {
                if let hello = model.hello {
                    ValueRow("Host version", value: hello.version)
                }
                ValueRow("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                Link("Source code", destination: URL(string: "https://github.com/leepokai/Codync")!)
                    .foregroundStyle(Palette.text)
            }
        }
        .navigationTitle("Computers & settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        .sheet(isPresented: $addingComputer) {
            PairingView(introductory: false) { addingComputer = false }
        }
        .codyncDialog("Remove \(confirmForget?.name ?? "computer")?",
                      isPresented: Binding(get: { confirmForget != nil }, set: { if !$0 { confirmForget = nil } }),
                      message: "Its bots and conversations stay on that computer. You can pair again any time.") {
            [DialogAction("Remove", destructive: true) {
                if let c = confirmForget { model.forget(c) }
                if model.pairing == nil { dismiss() }
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

private struct ComputerRow: View {
    let computer: Pairing
    let active: Bool
    let onRemove: () -> Void
    @Environment(BotStore.self) private var model
    @State private var coloring = false

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
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.text)
            }
            if active {
                Image(systemName: "checkmark").font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture { if !active { model.pair(computer) } }
        .contextActions {
            [
                MenuItem("Color", icon: "paintpalette") {
                    // ponytail: waits for the menu popover to close before opening the swatches; one popover at a time.
                    Task {
                        try? await Task.sleep(for: .milliseconds(350))
                        coloring = true
                    }
                },
                MenuItem("Remove", icon: "trash", destructive: true, divider: true, action: onRemove),
            ]
        }
        .background {
            Color.clear.popover(isPresented: $coloring, arrowEdge: .bottom) {
                SwatchPanel(selected: computer.color) { id in
                    coloring = false
                    model.setColor(computer, id)
                }
                .presentationCompactAdaptation(.popover)
                .presentationBackground(Palette.bubbleAgent)
            }
        }
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
